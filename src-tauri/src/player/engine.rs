//! Symphonia decode + cpal output (WASAPI).
//!
//! Pipeline: decode worker → bounded block channel (stereo f32 @ device rate) → cpal callback.
//! Block queue outputs silence on gap (no ring-buffer cache garbage).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, Stream};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use symphonia::core::audio::AudioBufferRef;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::sample::{i24, u24};
use symphonia::core::units::Time;

use super::{PlayerEngine, PlayStatus, PlayerSnapshot, QueueItem, RepeatMode, TrackInfo};

const STATUS_STOPPED: u8 = 0;
const STATUS_PLAYING: u8 = 1;
const STATUS_PAUSED: u8 = 2;
/// Stereo interleaved samples per block.
const BLOCK_SAMPLES: usize = 2048;
/// Blocks kept in flight (~0.15s at 1024 frames).
const BLOCK_QUEUE: usize = 8;

enum Cmd {
    /// 原子切歌：写队列 + 打开 + 播放，避免多条指令交错导致出声与 UI 不一致
    PlayQueue { items: Vec<QueueItem>, start: usize },
    Open { path: PathBuf, index: Option<usize> },
    Play,
    Pause,
    Seek { ms: u64 },
    SetVolume { v: f32 },
    SetQueue { items: Vec<QueueItem>, start: usize },
    /// 启动恢复：装入队列 + 打开当前曲 + 跳进度，**不自动播放**
    RestoreSession {
        items: Vec<QueueItem>,
        start: usize,
        position_ms: u64,
    },
    Next,
    Prev,
    #[allow(dead_code)] // 预留：停止/清理通道（gapless、换输出设备时用）
    Stop,
}

struct Shared {
    status: AtomicU8,
    position_frames: AtomicU64,
    /// 设备输出采样率；0 = worker 尚未发布（启动等待以此为就绪信号）
    sample_rate: AtomicU64,
    duration_ms: AtomicU64,
    volume_bits: AtomicU32,
    track_ended: AtomicBool,
    /// 音频世代号：每次「应丢弃已缓冲音频」（seek/切歌/显式 flush）+1。
    /// 块带世代戳、回调只播当前世代——单布尔标志会把 flush 后新到的有效块一并冲掉
    audio_gen: AtomicU64,
    /// 切歌确认序号：worker 处理完 Next/Prev 后 +1，engine 据此同步等待后再返回
    switch_seq: AtomicU64,
    play_mode: AtomicU8,
    track: Mutex<Option<TrackInfo>>,
    queue: Mutex<Vec<QueueItem>>,
    queue_index: Mutex<Option<usize>>,
    error: Mutex<Option<String>>,
}

/// 锁中毒恢复：音频回调/worker 里 panic 过也不至于连环炸，错误信息照样能写
fn set_error(shared: &Shared, msg: Option<String>) {
    match shared.error.lock() {
        Ok(mut g) => *g = msg,
        Err(p) => *p.into_inner() = msg,
    }
}

impl Shared {
    fn new() -> Self {
        Self {
            status: AtomicU8::new(STATUS_STOPPED),
            position_frames: AtomicU64::new(0),
            sample_rate: AtomicU64::new(0),
            duration_ms: AtomicU64::new(0),
            volume_bits: AtomicU32::new(0.8f32.to_bits()),
            track_ended: AtomicBool::new(false),
            audio_gen: AtomicU64::new(0),
            switch_seq: AtomicU64::new(0),
            play_mode: AtomicU8::new(0),
            track: Mutex::new(None),
            queue: Mutex::new(Vec::new()),
            queue_index: Mutex::new(None),
            error: Mutex::new(None),
        }
    }

    fn shuffle(&self) -> bool {
        self.play_mode.load(Ordering::SeqCst) & 1 != 0
    }

    fn repeat(&self) -> RepeatMode {
        match self.play_mode.load(Ordering::SeqCst) >> 1 {
            1 => RepeatMode::All,
            2 => RepeatMode::One,
            _ => RepeatMode::Off,
        }
    }

    /// bit0 = shuffle，高 2 位 = repeat（0 off / 1 all / 2 one）
    fn play_mode_raw(&self) -> u8 {
        self.play_mode.load(Ordering::SeqCst)
    }

    fn status(&self) -> PlayStatus {
        match self.status.load(Ordering::SeqCst) {
            STATUS_PLAYING => PlayStatus::Playing,
            STATUS_PAUSED => PlayStatus::Paused,
            _ => PlayStatus::Stopped,
        }
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::SeqCst))
    }

    fn position_ms(&self) -> u64 {
        let sr = self.sample_rate.load(Ordering::SeqCst).max(1);
        self.position_frames.load(Ordering::SeqCst) * 1000 / sr
    }

    fn queue_index(&self) -> Option<usize> {
        self.queue_index.lock().map(|g| *g).unwrap_or(None)
    }

    fn request_flush(&self) {
        self.audio_gen.fetch_add(1, Ordering::SeqCst);
    }
}

pub struct SymphoniaPlayer {
    shared: Arc<Shared>,
    cmd_tx: Sender<Cmd>,
    _worker: Option<std::thread::JoinHandle<()>>,
    output_sample_rate: u32,
}

impl SymphoniaPlayer {
    pub fn new() -> Result<Self> {
        let (cmd_tx, cmd_rx) = unbounded::<Cmd>();
        let shared = Arc::new(Shared::new());
        let worker_shared = Arc::clone(&shared);

        let worker = std::thread::Builder::new()
            .name("axmusic-player".into())
            .spawn(move || worker_main(cmd_rx, worker_shared))
            .context("启动播放线程失败")?;

        // 等 worker 发布真实设备采样率（初始 0，非 0 即就绪）或启动报错；超时兜底继续
        for _ in 0..500 {
            if shared.error.lock().map(|e| e.is_some()).unwrap_or(true) {
                break;
            }
            if shared.sample_rate.load(Ordering::SeqCst) != 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }

        Ok(Self {
            output_sample_rate: shared.sample_rate.load(Ordering::SeqCst) as u32,
            shared,
            cmd_tx,
            _worker: Some(worker),
        })
    }

    pub fn current_track(&self) -> Option<TrackInfo> {
        self.shared.track.lock().ok().and_then(|t| t.clone())
    }

    pub fn snapshot(&self) -> PlayerSnapshot {
        let track = self.current_track();
        let duration_ms = track
            .as_ref()
            .map(|t| t.duration_ms)
            .unwrap_or_else(|| self.shared.duration_ms.load(Ordering::SeqCst));
        let queue = self.shared.queue.lock().map(|q| q.clone()).unwrap_or_default();
        let queue_index = self.shared.queue_index();
        PlayerSnapshot {
            status: self.shared.status(),
            position_ms: self.shared.position_ms().min(duration_ms),
            duration_ms,
            volume: self.shared.volume(),
            track,
            queue,
            queue_index,
            shuffle: self.shared.shuffle(),
            repeat: self.shared.repeat(),
        }
    }

    pub fn set_shuffle(&mut self, on: bool) {
        let raw = self.shared.play_mode_raw();
        let next = if on { raw | 1 } else { raw & !1 };
        self.shared.play_mode.store(next, Ordering::SeqCst);
    }

    pub fn set_repeat(&mut self, mode: RepeatMode) {
        let rep = match mode {
            RepeatMode::Off => 0u8,
            RepeatMode::All => 1,
            RepeatMode::One => 2,
        };
        let raw = self.shared.play_mode_raw();
        let next = (raw & 1) | (rep << 1);
        self.shared.play_mode.store(next, Ordering::SeqCst);
    }

    /// 同步写入 shared（UI 立刻对准本次点击），再下发原子 PlayQueue 打开解码。
    pub fn play_queue_at(&mut self, items: Vec<QueueItem>, start: usize) -> Result<TrackInfo> {
        let item = items
            .get(start)
            .cloned()
            .ok_or_else(|| anyhow!("队列中无此曲目"))?;
        let info = TrackInfo {
            path: item.path.clone(),
            title: item.title.clone(),
            duration_ms: item.duration_ms,
            sample_rate: self.output_sample_rate,
            channels: 2,
        };
        if let Ok(mut q) = self.shared.queue.lock() {
            *q = items.clone();
        }
        if let Ok(mut qi) = self.shared.queue_index.lock() {
            *qi = Some(start);
        }
        if let Ok(mut t) = self.shared.track.lock() {
            *t = Some(info.clone());
        }
        self.shared
            .duration_ms
            .store(info.duration_ms.max(1), Ordering::SeqCst);
        self.shared.position_frames.store(0, Ordering::SeqCst);
        self.shared.track_ended.store(false, Ordering::SeqCst);
        self.shared.status.store(STATUS_PLAYING, Ordering::SeqCst);
        self.shared.request_flush();
        let _ = self.cmd_tx.send(Cmd::PlayQueue { items, start });
        Ok(info)
    }

    /// 追加到当前播放队列末尾（不打断正在播的曲目；空队列时仅入队等播）。
    pub fn enqueue(&mut self, items: Vec<QueueItem>) {
        if items.is_empty() {
            return;
        }
        if let Ok(mut q) = self.shared.queue.lock() {
            q.extend(items);
        }
        // 不动音频缓冲：flush 会把当前曲目已解码的队尾冲掉，造成可闻断音
    }

    /// 启动恢复上次播放列表：写入队列/当前曲/进度，状态为暂停（不自动播）。
    pub fn restore_session(
        &mut self,
        items: Vec<QueueItem>,
        start: usize,
        position_ms: u64,
    ) -> Result<Option<TrackInfo>> {
        if let Ok(mut q) = self.shared.queue.lock() {
            *q = items.clone();
        }
        if let Ok(mut qi) = self.shared.queue_index.lock() {
            *qi = if items.is_empty() { None } else { Some(start) };
        }
        let info = items.get(start).map(|item| TrackInfo {
            path: item.path.clone(),
            title: item.title.clone(),
            duration_ms: item.duration_ms,
            sample_rate: self.output_sample_rate,
            channels: 2,
        });
        if let Some(info) = info.as_ref() {
            if let Ok(mut t) = self.shared.track.lock() {
                *t = Some(info.clone());
            }
            self.shared
                .duration_ms
                .store(info.duration_ms.max(1), Ordering::SeqCst);
        }
        let rate = self.shared.sample_rate.load(Ordering::SeqCst).max(1);
        self.shared
            .position_frames
            .store(position_ms.saturating_mul(rate) / 1000, Ordering::SeqCst);
        self.shared.track_ended.store(false, Ordering::SeqCst);
        self.shared.status.store(STATUS_PAUSED, Ordering::SeqCst);
        self.shared.request_flush();
        let _ = self.cmd_tx.send(Cmd::RestoreSession {
            items,
            start,
            position_ms,
        });
        Ok(info)
    }

    /// 同步写入 shared.track 后打开单文件，保证 snapshot 与点击一致。
    pub fn play_path_at(&mut self, path: &Path) -> Result<TrackInfo> {
        let info = quick_track_info(path);
        if let Ok(mut t) = self.shared.track.lock() {
            *t = Some(info.clone());
        }
        self.shared
            .duration_ms
            .store(info.duration_ms.max(1), Ordering::SeqCst);
        self.shared.position_frames.store(0, Ordering::SeqCst);
        self.shared.track_ended.store(false, Ordering::SeqCst);
        self.shared.status.store(STATUS_PLAYING, Ordering::SeqCst);
        self.shared.request_flush();
        let _ = self.cmd_tx.send(Cmd::Open {
            path: path.to_path_buf(),
            index: None,
        });
        let _ = self.cmd_tx.send(Cmd::Play);
        Ok(info)
    }

    /// 发切歌指令并等 worker 真正切完（switch_seq 确认），返回的是**新曲**信息；
    /// 超时（解码打开慢，~0.6s）则返回现状，调用方快照/轮询兜底。
    fn switch_and_wait(&self, cmd: Cmd) {
        let before = self.shared.switch_seq.load(Ordering::SeqCst);
        if self.cmd_tx.send(cmd).is_err() {
            return;
        }
        for _ in 0..120 {
            if self.shared.switch_seq.load(Ordering::SeqCst) != before {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn next(&mut self) -> Result<Option<TrackInfo>> {
        self.switch_and_wait(Cmd::Next);
        Ok(self.current_track())
    }

    pub fn prev(&mut self) -> Result<Option<TrackInfo>> {
        self.switch_and_wait(Cmd::Prev);
        Ok(self.current_track())
    }

    #[allow(dead_code)] // 预留：停止/清理（与 Cmd::Stop 配对）
    pub fn stop(&mut self) {
        let _ = self.cmd_tx.send(Cmd::Stop);
    }

    pub fn poll_track_ended(&mut self) -> Option<TrackInfo> {
        if self.shared.track_ended.swap(false, Ordering::SeqCst) {
            return self.current_track();
        }
        None
    }

    pub fn play_pause(&mut self) {
        if self.shared.status() == PlayStatus::Playing {
            self.pause_inner();
        } else {
            self.play_inner();
        }
    }

    pub fn play_inner(&mut self) {
        let _ = self.cmd_tx.send(Cmd::Play);
    }

    pub fn pause_inner(&mut self) {
        let _ = self.cmd_tx.send(Cmd::Pause);
    }

    pub fn seek_to(&mut self, ms: u64) -> Result<()> {
        // 乐观对准目标进度：snapshot 立刻反映本次 seek，避免 UI 松手回弹
        // （真正的解码跳转在 worker 的 Cmd::Seek 里完成，成功后会再写一次 position）
        let rate = self.shared.sample_rate.load(Ordering::SeqCst).max(1);
        self.shared
            .position_frames
            .store(ms.saturating_mul(rate) / 1000, Ordering::SeqCst);
        self.shared.request_flush();
        let _ = self.cmd_tx.send(Cmd::Seek { ms });
        Ok(())
    }

    pub fn set_volume_f32(&mut self, v: f32) {
        let v = v.clamp(0.0, 1.0);
        self.shared.volume_bits.store(v.to_bits(), Ordering::SeqCst);
        let _ = self.cmd_tx.send(Cmd::SetVolume { v });
    }
}

impl PlayerEngine for SymphoniaPlayer {
    fn open(&mut self, path: &Path) -> Result<TrackInfo> {
        self.play_path_at(path)
    }

    fn play(&mut self) {
        self.play_inner();
    }

    fn pause(&mut self) {
        self.pause_inner();
    }

    fn seek(&mut self, ms: u64) -> Result<()> {
        self.seek_to(ms)
    }

    fn set_queue(&mut self, items: Vec<QueueItem>) {
        let _ = self.cmd_tx.send(Cmd::SetQueue { items, start: 0 });
    }

    fn set_output_device(&mut self, _id: &str) -> Result<()> {
        Ok(())
    }

    fn position(&self) -> u64 {
        self.shared.position_ms()
    }

    fn volume(&self) -> f32 {
        self.shared.volume()
    }

    fn set_volume(&mut self, v: f32) {
        self.set_volume_f32(v);
    }
}

fn quick_track_info(path: &Path) -> TrackInfo {
    let mut info = TrackInfo {
        path: path.to_string_lossy().to_string(),
        title: path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        duration_ms: 0,
        sample_rate: 44_100,
        channels: 2,
    };
    if let Ok(tagged) = lofty::read_from_path(path) {
        use lofty::file::{AudioFile, TaggedFileExt};
        use lofty::prelude::Accessor;
        let props = tagged.properties();
        info.duration_ms = props.duration().as_millis() as u64;
        info.sample_rate = props.sample_rate().unwrap_or(44_100);
        info.channels = props.channels().map(|c| c as u16).unwrap_or(2);
        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            if let Some(title) = tag.title() {
                let t = title.to_string();
                if !t.is_empty() {
                    info.title = t;
                }
            }
        }
    }
    info
}

// ── worker + audio callback ───────────────────────────────────────

struct DecoderState {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    duration_ms: u64,
    src_sample_rate: u32,
    /// 源声道数（展示用；输出端统一转立体声再按设备映射）
    src_channels: u16,
    end: bool,
}

fn worker_main(cmd_rx: Receiver<Cmd>, shared: Arc<Shared>) {
    let host = cpal::default_host();
    let Some(device) = host.default_output_device() else {
        set_error(&shared, Some("无默认音频输出设备".into()));
        return;
    };
    let Ok(default_config) = device.default_output_config() else {
        set_error(&shared, Some("读取输出配置失败".into()));
        return;
    };
    let out_rate = default_config.sample_rate().0;
    let out_channels = usize::from(default_config.channels()).max(1);
    shared.sample_rate.store(out_rate as u64, Ordering::SeqCst);

    // 块 = (世代号, 段起点锚[帧], 立体声交错样本)；世代过滤过期块，锚点校准进度
    let (block_tx, block_rx) = bounded::<(u64, Option<u64>, Vec<f32>)>(BLOCK_QUEUE);
    let mut audio = AudioOut {
        cur: Vec::new(),
        pos: 0,
        block_rx,
        shared: Arc::clone(&shared),
        cur_gen: 0,
        out_channels,
    };

    let stream = match default_config.sample_format() {
        SampleFormat::F32 => device.build_output_stream(
            &default_config.into(),
            move |data: &mut [f32], _| audio.fill(data),
            |e| eprintln!("audio stream error: {e}"),
            None,
        ),
        SampleFormat::I16 => {
            let mut tmp = Vec::new();
            device.build_output_stream(
                &default_config.into(),
                move |data: &mut [i16], _| {
                    tmp.clear();
                    tmp.resize(data.len(), 0.0);
                    audio.fill(&mut tmp);
                    for (o, i) in data.iter_mut().zip(tmp.iter()) {
                        *o = Sample::from_sample(*i);
                    }
                },
                |e| eprintln!("audio stream error: {e}"),
                None,
            )
        }
        SampleFormat::U16 => {
            let mut tmp = Vec::new();
            device.build_output_stream(
                &default_config.into(),
                move |data: &mut [u16], _| {
                    tmp.clear();
                    tmp.resize(data.len(), 0.0);
                    audio.fill(&mut tmp);
                    for (o, i) in data.iter_mut().zip(tmp.iter()) {
                        *o = Sample::from_sample(*i);
                    }
                },
                |e| eprintln!("audio stream error: {e}"),
                None,
            )
        }
        _ => {
            set_error(&shared, Some("不支持的采样格式".into()));
            return;
        }
    };

    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            set_error(&shared, Some(format!("打开音频输出失败: {e}")));
            return;
        }
    };
    if let Err(e) = stream.play() {
        set_error(&shared, Some(format!("启动音频输出失败: {e}")));
        return;
    }

    // cpal::Stream is !Send — keep on this thread.
    let _stream: Stream = stream;
    decode_loop(cmd_rx, shared, block_tx, out_rate);
}

struct AudioOut {
    cur: Vec<f32>,
    pos: usize,
    block_rx: Receiver<(u64, Option<u64>, Vec<f32>)>,
    shared: Arc<Shared>,
    /// 已消费到的音频世代；与 shared.audio_gen 不同即先清空手上缓冲
    cur_gen: u64,
    /// 设备声道数（解码端统一出立体声，这里按设备映射：单声道混音、多声道补零）
    out_channels: usize,
}

impl AudioOut {
    /// 拉下一块：丢弃过期世代的块；段起点块（新曲/seek 后首块）用锚点校准进度
    fn pull_next(&mut self) {
        loop {
            match self.block_rx.try_recv() {
                Ok((gen, anchor, block)) => {
                    if gen < self.cur_gen {
                        continue;
                    }
                    self.cur = block;
                    self.pos = 0;
                    if let Some(frames) = anchor {
                        self.shared.position_frames.store(frames, Ordering::SeqCst);
                    }
                    return;
                }
                Err(_) => {
                    self.cur.clear();
                    self.pos = 0;
                    return;
                }
            }
        }
    }

    fn fill(&mut self, data: &mut [f32]) {
        let gen = self.shared.audio_gen.load(Ordering::SeqCst);
        if gen != self.cur_gen {
            self.cur_gen = gen;
            self.cur.clear();
            self.pos = 0;
        }
        let vol = self.shared.volume();
        let paused = self.shared.status.load(Ordering::SeqCst) != STATUS_PLAYING;
        let ch = self.out_channels;
        let frames = data.len() / ch;
        let mut frames_written = 0u64;

        for f in 0..frames {
            let o = f * ch;
            if paused {
                data[o..o + ch].fill(0.0);
                continue;
            }
            if self.pos >= self.cur.len() {
                self.pull_next();
            }
            if self.pos + 1 < self.cur.len() {
                // 源固定立体声交错；按设备声道数映射
                let l = self.cur[self.pos] * vol;
                let r = self.cur[self.pos + 1] * vol;
                self.pos += 2;
                match ch {
                    1 => data[o] = (l + r) * 0.5,
                    2 => {
                        data[o] = l;
                        data[o + 1] = r;
                    }
                    _ => {
                        data[o] = l;
                        data[o + 1] = r;
                        data[o + 2..o + ch].fill(0.0);
                    }
                }
                frames_written += 1;
            } else {
                data[o..o + ch].fill(0.0);
            }
        }
        if frames_written > 0 {
            self.shared
                .position_frames
                .fetch_add(frames_written, Ordering::SeqCst);
        }
    }
}

struct DecoderState2 {
    inner: DecoderState,
    /// pending samples at device rate (stereo interleaved)
    pending: Vec<f32>,
    resample_pos: f64,
}

fn open_decoder(path: &Path) -> Result<DecoderState> {
    let file = File::open(path).with_context(|| format!("无法打开 {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .context("无法解析音频容器")?;
    let reader = probed.format;
    let track = reader
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("无可用音频轨"))?
        .clone();
    let track_id = track.id;
    let params = track.codec_params.clone();
    let duration_ms = match (params.n_frames, params.sample_rate) {
        (Some(frames), Some(rate)) if rate > 0 => (frames as u128 * 1000 / rate as u128) as u64,
        _ => 0,
    };
    let src_sample_rate = params.sample_rate.unwrap_or(44_100).max(1);
    let src_channels = params.channels.map(|c| c.count() as u16).unwrap_or(2);
    let decoder = symphonia::default::get_codecs()
        .make(&params, &DecoderOptions::default())
        .context("不支持的编解码器")?;
    Ok(DecoderState {
        reader,
        decoder,
        track_id,
        duration_ms,
        src_sample_rate,
        src_channels,
        end: false,
    })
}

/// `flush`：是否丢弃已缓冲音频。显式切歌/seek 要；EOF 自动连播不要（保留队尾，无缝衔接）。
fn open_track_full(
    shared: &Shared,
    path: &Path,
    index: Option<usize>,
    queue_item: Option<&QueueItem>,
    flush: bool,
) -> Option<DecoderState2> {
    if flush {
        shared.request_flush();
        shared.position_frames.store(0, Ordering::SeqCst);
    }
    // EOF 连播（flush=false）：缓冲队尾继续播、进度不归零，
    // 由新曲首块的锚点在队尾播完那一刻校准（gapless）
    shared.track_ended.store(false, Ordering::SeqCst);

    match open_decoder(path) {
        Ok(st) => {
            let title = queue_item
                .map(|q| q.title.clone())
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| {
                    path.file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default()
                });
            let info = TrackInfo {
                path: path.to_string_lossy().to_string(),
                title,
                duration_ms: st
                    .duration_ms
                    .max(queue_item.map(|q| q.duration_ms).unwrap_or(0)),
                sample_rate: st.src_sample_rate,
                channels: st.src_channels,
            };
            shared
                .duration_ms
                .store(info.duration_ms.max(1), Ordering::SeqCst);
            if let Ok(mut t) = shared.track.lock() {
                *t = Some(info);
            }
            if let Ok(mut qi) = shared.queue_index.lock() {
                *qi = index;
            }
            set_error(shared, None);
            Some(DecoderState2 {
                inner: st,
                pending: Vec::new(),
                resample_pos: 0.0,
            })
        }
        Err(err) => {
            set_error(shared, Some(format!("{err:#}")));
            if let Ok(mut t) = shared.track.lock() {
                *t = None;
            }
            shared.status.store(STATUS_STOPPED, Ordering::SeqCst);
            None
        }
    }
}

fn seek_decoder(st: &mut DecoderState, ms: u64) -> Result<()> {
    let time = Time::from(ms as f64 / 1000.0);
    st.reader.seek(
        SeekMode::Coarse,
        SeekTo::Time {
            time,
            track_id: Some(st.track_id),
        },
    )?;
    st.decoder.reset();
    st.end = false;
    Ok(())
}

fn decode_chunk_native(st: &mut DecoderState) -> std::result::Result<Option<Vec<f32>>, SymError> {
    if st.end {
        return Ok(None);
    }
    loop {
        let packet = match st.reader.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                st.end = true;
                return Ok(None);
            }
            Err(SymError::ResetRequired) => {
                st.end = true;
                return Ok(None);
            }
            Err(e) => return Err(e),
        };
        if packet.track_id() != st.track_id {
            continue;
        }
        match st.decoder.decode(&packet) {
            Ok(buf) => return Ok(Some(to_interleaved_stereo(&buf))),
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(e),
        }
    }
}

fn pack_planes<T: Copy>(planes: &[&[T]], convert: impl Fn(T) -> f32) -> Vec<f32> {
    match planes.len() {
        0 => Vec::new(),
        1 => {
            let mono = planes[0];
            let mut out = Vec::with_capacity(mono.len() * 2);
            for &s in mono {
                let v = convert(s);
                out.push(v);
                out.push(v);
            }
            out
        }
        _ => {
            let l = planes[0];
            let r = planes[1];
            let n = l.len().min(r.len());
            let mut out = Vec::with_capacity(n * 2);
            for i in 0..n {
                out.push(convert(l[i]));
                out.push(convert(r[i]));
            }
            out
        }
    }
}

fn to_interleaved_stereo(buf: &AudioBufferRef<'_>) -> Vec<f32> {
    match buf {
        AudioBufferRef::F32(b) => pack_planes(b.planes().planes(), |s: f32| s),
        AudioBufferRef::U8(b) => {
            pack_planes(b.planes().planes(), |s: u8| (s as f32 - 128.0) / 128.0)
        }
        AudioBufferRef::U16(b) => {
            pack_planes(b.planes().planes(), |s: u16| (s as f32 / 32768.0) - 1.0)
        }
        AudioBufferRef::U24(b) => pack_planes(b.planes().planes(), |s: u24| {
            (s.inner() as f32 / 8_388_608.0) - 1.0
        }),
        AudioBufferRef::U32(b) => pack_planes(b.planes().planes(), |s: u32| {
            (s as f32 / 2_147_483_648.0) - 1.0
        }),
        AudioBufferRef::S8(b) => pack_planes(b.planes().planes(), |s: i8| s as f32 / 128.0),
        AudioBufferRef::S16(b) => pack_planes(b.planes().planes(), |s: i16| s as f32 / 32768.0),
        AudioBufferRef::S24(b) => pack_planes(b.planes().planes(), |s: i24| {
            s.inner() as f32 / 8_388_608.0
        }),
        AudioBufferRef::S32(b) => pack_planes(b.planes().planes(), |s: i32| {
            s as f32 / 2_147_483_648.0
        }),
        AudioBufferRef::F64(b) => pack_planes(b.planes().planes(), |s: f64| s as f32),
    }
}

/// Convert a source-rate stereo block to device rate (linear interpolation).
fn resample_stereo(input: &[f32], in_rate: u32, out_rate: u32, pos: &mut f64) -> Vec<f32> {
    if in_rate == out_rate {
        return input.to_vec();
    }
    let frames_in = input.len() / 2;
    if frames_in == 0 {
        return Vec::new();
    }
    let step = in_rate as f64 / out_rate as f64;
    let mut out = Vec::with_capacity(((frames_in as f64) / step).ceil() as usize * 2 + 4);
    // pos is the fractional read cursor in input frames
    while *pos + 1.0 < frames_in as f64 {
        let i0 = (*pos).floor() as usize;
        let i1 = i0 + 1;
        let frac = (*pos) - i0 as f64;
        for ch in 0..2 {
            let a = input[i0 * 2 + ch];
            let b = input[i1 * 2 + ch];
            out.push(a + (b - a) * frac as f32);
        }
        *pos += step;
    }
    // keep leftover position relative to next buffer
    *pos -= frames_in as f64;
    if *pos < 0.0 {
        *pos = 0.0;
    }
    out
}

fn fill_block(dec: &mut DecoderState2, out_rate: u32) -> Option<Vec<f32>> {
    while dec.pending.len() < BLOCK_SAMPLES {
        match decode_chunk_native(&mut dec.inner) {
            Ok(Some(native)) => {
                let rs = resample_stereo(
                    &native,
                    dec.inner.src_sample_rate,
                    out_rate,
                    &mut dec.resample_pos,
                );
                dec.pending.extend_from_slice(&rs);
            }
            Ok(None) => break,
            Err(SymError::DecodeError(_)) => {}
            Err(_) => {
                dec.inner.end = true;
                break;
            }
        }
    }
    if dec.pending.is_empty() {
        return None;
    }
    let n = BLOCK_SAMPLES.min(dec.pending.len());
    let block: Vec<f32> = dec.pending.drain(..n).collect();
    Some(block)
}

/// 轻量伪随机（无外部依赖）：时间 + 当前下标混合
fn shuffle_pick(len: usize, current: Option<usize>) -> Option<usize> {
    if len == 0 {
        return None;
    }
    if len == 1 {
        return Some(0);
    }
    let mut x = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
        ^ (current.unwrap_or(0) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    let mut idx = (x as usize) % len;
    if Some(idx) == current {
        idx = (idx + 1) % len;
    }
    Some(idx)
}

fn decode_loop(
    cmd_rx: Receiver<Cmd>,
    shared: Arc<Shared>,
    block_tx: Sender<(u64, Option<u64>, Vec<f32>)>,
    out_rate: u32,
) {
    let mut dec: Option<DecoderState2> = None;
    let mut playing = false;
    let mut queue: Vec<QueueItem> = Vec::new();
    let mut queue_index: Option<usize> = None;
    let mut pending_block: Option<(u64, Option<u64>, Vec<f32>)> = None;
    let mut history: Vec<usize> = Vec::new();
    // 段起点锚（设备帧）：新曲 = 0，seek = 目标位置；由段内首块带上，回调播到它时校准进度
    let mut seg_anchor: Option<u64> = None;

    loop {
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Cmd::PlayQueue { items, start } => {
                    queue = items;
                    queue_index = Some(start);
                    history.clear();
                    if let Ok(mut q) = shared.queue.lock() {
                        *q = queue.clone();
                    }
                    if let Ok(mut qi) = shared.queue_index.lock() {
                        *qi = Some(start);
                    }
                    pending_block = None;
                    if let Some(item) = queue.get(start).cloned() {
                        let path = PathBuf::from(&item.path);
                        dec = open_track_full(&shared, &path, Some(start), Some(&item), true);
                        playing = dec.is_some();
                        seg_anchor = if playing { Some(0) } else { None };
                        if playing {
                            shared.status.store(STATUS_PLAYING, Ordering::SeqCst);
                        }
                    } else {
                        dec = None;
                        playing = false;
                        seg_anchor = None;
                        shared.status.store(STATUS_STOPPED, Ordering::SeqCst);
                    }
                }
                Cmd::Open { path, index } => {
                    pending_block = None;
                    let item = index.and_then(|i| queue.get(i).cloned());
                    dec = open_track_full(&shared, &path, index, item.as_ref(), true);
                    seg_anchor = if dec.is_some() { Some(0) } else { None };
                }
                Cmd::Play => {
                    // 业界惯例：播完停住后再点播放 = 从头再听，而不是假播放（无解码器）
                    // - 单曲/停在某曲末尾 → 重播当前曲
                    // - 列表播完停住（末首）→ 从第一首再听一遍
                    let dur = shared.duration_ms.load(Ordering::SeqCst);
                    let pos = shared.position_ms();
                    let at_end = dur > 0 && pos.saturating_add(80) >= dur;
                    let need_restart = dec.is_none() || at_end;
                    if !need_restart {
                        playing = true;
                        shared.status.store(STATUS_PLAYING, Ordering::SeqCst);
                    } else {
                        let finished_list = dec.is_none()
                            && queue.len() > 1
                            && queue_index.map(|i| i + 1 >= queue.len()).unwrap_or(false);
                        let idx = if finished_list {
                            Some(0)
                        } else {
                            queue_index.filter(|i| *i < queue.len()).or(if queue.is_empty() {
                                None
                            } else {
                                Some(0)
                            })
                        };
                        let item = idx.and_then(|i| queue.get(i).cloned()).or_else(|| {
                            // 单文件播放（不在队列里）：用当前 track 重开
                            shared
                                .track
                                .lock()
                                .ok()
                                .and_then(|t| t.clone())
                                .map(|t| QueueItem {
                                    path: t.path,
                                    title: t.title,
                                    duration_ms: t.duration_ms,
                                })
                        });
                        if let Some(item) = item {
                            let path = PathBuf::from(&item.path);
                            pending_block = None;
                            if finished_list {
                                queue_index = Some(0);
                                if let Ok(mut qi) = shared.queue_index.lock() {
                                    *qi = Some(0);
                                }
                            }
                            dec = open_track_full(&shared, &path, idx, Some(&item), true);
                            playing = dec.is_some();
                            seg_anchor = if playing { Some(0) } else { None };
                            shared.status.store(
                                if playing { STATUS_PLAYING } else { STATUS_STOPPED },
                                Ordering::SeqCst,
                            );
                        } else {
                            playing = false;
                            seg_anchor = None;
                            shared.status.store(STATUS_STOPPED, Ordering::SeqCst);
                        }
                    }
                }
                Cmd::Pause => {
                    playing = false;
                    shared.status.store(STATUS_PAUSED, Ordering::SeqCst);
                }
                Cmd::Seek { ms } => {
                    if let Some(d) = dec.as_mut() {
                        if seek_decoder(&mut d.inner, ms).is_ok() {
                            d.pending.clear();
                            d.resample_pos = 0.0;
                            shared.request_flush();
                            pending_block = None;
                            let rate = shared.sample_rate.load(Ordering::SeqCst).max(1);
                            let frames = ms.saturating_mul(rate) / 1000;
                            shared.position_frames.store(frames, Ordering::SeqCst);
                            seg_anchor = Some(frames);
                        }
                    }
                }
                Cmd::SetVolume { v } => {
                    shared.volume_bits.store(v.to_bits(), Ordering::SeqCst);
                }
                Cmd::SetQueue { items, start } => {
                    queue = items;
                    queue_index = Some(start);
                    history.clear();
                    if let Ok(mut qi) = shared.queue_index.lock() {
                        *qi = Some(start);
                    }
                    if let Ok(mut q) = shared.queue.lock() {
                        *q = queue.clone();
                    }
                }
                Cmd::RestoreSession {
                    items,
                    start,
                    position_ms,
                } => {
                    queue = items;
                    queue_index = if queue.is_empty() { None } else { Some(start) };
                    history.clear();
                    if let Ok(mut q) = shared.queue.lock() {
                        *q = queue.clone();
                    }
                    if let Ok(mut qi) = shared.queue_index.lock() {
                        *qi = queue_index;
                    }
                    pending_block = None;
                    if let Some(item) = queue.get(start).cloned() {
                        let path = PathBuf::from(&item.path);
                        dec = open_track_full(&shared, &path, Some(start), Some(&item), true);
                        if let Some(d) = dec.as_mut() {
                            if seek_decoder(&mut d.inner, position_ms).is_ok() {
                                d.pending.clear();
                                d.resample_pos = 0.0;
                                let rate = shared.sample_rate.load(Ordering::SeqCst).max(1);
                                let frames = position_ms.saturating_mul(rate) / 1000;
                                shared.position_frames.store(frames, Ordering::SeqCst);
                                seg_anchor = Some(frames);
                            }
                        }
                        playing = false;
                        shared.status.store(STATUS_PAUSED, Ordering::SeqCst);
                    } else {
                        dec = None;
                        playing = false;
                        seg_anchor = None;
                        shared.status.store(STATUS_STOPPED, Ordering::SeqCst);
                    }
                }
                Cmd::Next => {
                    // 手动下一首：随机则抽下一首；顺序则下标 +1 末尾回绕（不受循环模式限制）
                    let target = if queue.is_empty() {
                        None
                    } else if shared.shuffle() {
                        shuffle_pick(queue.len(), queue_index)
                    } else {
                        Some(match queue_index {
                            Some(i) => (i + 1) % queue.len(),
                            None => 0,
                        })
                    };
                    if let Some(target) = target {
                        if let Some(item) = queue.get(target).cloned() {
                            if let Some(prev) = queue_index {
                                if prev != target {
                                    history.push(prev);
                                    if history.len() > 64 {
                                        history.remove(0);
                                    }
                                }
                            }
                            let path = PathBuf::from(&item.path);
                            pending_block = None;
                            queue_index = Some(target);
                            if let Ok(mut qi) = shared.queue_index.lock() {
                                *qi = Some(target);
                            }
                            dec = open_track_full(&shared, &path, queue_index, Some(&item), true);
                            if dec.is_some() {
                                playing = true;
                                seg_anchor = Some(0);
                                shared.status.store(STATUS_PLAYING, Ordering::SeqCst);
                            }
                        }
                    }
                    // 确认序号：engine 侧的 next() 在等这个信号再返回新曲目
                    shared.switch_seq.fetch_add(1, Ordering::SeqCst);
                }
                Cmd::Prev => {
                    // 播过 3s 先重头；否则按模式回退
                    let pos = shared.position_ms();
                    if pos > 3_000 {
                        if let Some(idx) = queue_index {
                            if let Some(item) = queue.get(idx).cloned() {
                                let path = PathBuf::from(&item.path);
                                pending_block = None;
                                dec = open_track_full(&shared, &path, Some(idx), Some(&item), true);
                                if dec.is_some() {
                                    playing = true;
                                    seg_anchor = Some(0);
                                    shared.status.store(STATUS_PLAYING, Ordering::SeqCst);
                                }
                            }
                        }
                    } else if !queue.is_empty() {
                        let target = if shared.shuffle() {
                            history.pop().or_else(|| shuffle_pick(queue.len(), queue_index))
                        } else {
                            Some(match queue_index {
                                Some(0) | None => queue.len() - 1,
                                Some(i) => i - 1,
                            })
                        };
                        if let Some(target) = target {
                            if let Some(item) = queue.get(target).cloned() {
                                let path = PathBuf::from(&item.path);
                                pending_block = None;
                                queue_index = Some(target);
                                if let Ok(mut qi) = shared.queue_index.lock() {
                                    *qi = Some(target);
                                }
                                dec = open_track_full(&shared, &path, queue_index, Some(&item), true);
                                if dec.is_some() {
                                    playing = true;
                                    seg_anchor = Some(0);
                                    shared.status.store(STATUS_PLAYING, Ordering::SeqCst);
                                }
                            }
                        }
                    }
                    shared.switch_seq.fetch_add(1, Ordering::SeqCst);
                }
                Cmd::Stop => {
                    dec = None;
                    playing = false;
                    pending_block = None;
                    seg_anchor = None;
                    shared.request_flush();
                    shared.status.store(STATUS_STOPPED, Ordering::SeqCst);
                    if let Ok(mut t) = shared.track.lock() {
                        *t = None;
                    }
                }
            }
        }

        if playing {
            if let Some(d) = dec.as_mut() {
                if pending_block.is_none() {
                    // 世代戳在解码前打：解码期间发生的 flush 会让本块过期被丢弃
                    let gen = shared.audio_gen.load(Ordering::SeqCst);
                    pending_block = fill_block(d, out_rate).map(|b| (gen, seg_anchor.take(), b));
                }
                match pending_block.take() {
                    Some(block) => {
                        // try_send：满则暂存，绝不阻塞，保证切歌指令能被立刻处理
                        match block_tx.try_send(block) {
                            Ok(()) => {}
                            Err(crossbeam_channel::TrySendError::Full(b)) => {
                                pending_block = Some(b);
                                std::thread::sleep(Duration::from_millis(2));
                            }
                            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                                playing = false;
                            }
                        }
                    }
                    None => {
                        // EOF：不 flush——已入通道的队尾继续播完（gapless），
                        // 新曲首块带锚点 0，播到它时进度归零；自动切歌只在这里发生（单一入口）
                        shared.track_ended.store(true, Ordering::SeqCst);
                        let mut advanced = false;
                        if !queue.is_empty() {
                            // 自动切歌：单曲循环重播；随机续抽；顺序看列表循环/播完停
                            let target = if shared.repeat() == RepeatMode::One {
                                queue_index.or(Some(0))
                            } else if shared.shuffle() {
                                shuffle_pick(queue.len(), queue_index)
                            } else {
                                queue_index.and_then(|i| {
                                    if i + 1 < queue.len() {
                                        Some(i + 1)
                                    } else if shared.repeat() == RepeatMode::All {
                                        Some(0)
                                    } else {
                                        None
                                    }
                                })
                            };
                            if let Some(target) = target {
                                if let Some(item) = queue.get(target).cloned() {
                                    let path = PathBuf::from(&item.path);
                                    pending_block = None;
                                    queue_index = Some(target);
                                    if let Ok(mut qi) = shared.queue_index.lock() {
                                        *qi = Some(target);
                                    }
                                    dec = open_track_full(&shared, &path, queue_index, Some(&item), false);
                                    advanced = dec.is_some();
                                    if advanced {
                                        seg_anchor = Some(0);
                                    }
                                }
                            }
                        }
                        if !advanced {
                            dec = None;
                            playing = false;
                            seg_anchor = None;
                            shared.status.store(STATUS_STOPPED, Ordering::SeqCst);
                        }
                    }
                }
            } else {
                std::thread::sleep(Duration::from_millis(20));
            }
        } else {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
