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
use symphonia::core::audio::sample::{i24, u24};
use symphonia::core::audio::conv::ConvertibleSample;
use symphonia::core::audio::{Audio, AudioBuffer, GenericAudioBufferRef};
use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Time;

use super::replaygain::{self, ReplayGainInfo, ReplayGainMode};
use super::viz::{self, VizTap};
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
    /// 移出队列第 index 项（不打断其它项；当前曲被移时 engine 侧再切歌/停）
    RemoveAt { index: usize },
    /// 追加队列（不打断当前曲；与 shared 同步，自动切歌/循环按完整队列走）
    ExtendQueue { items: Vec<QueueItem> },
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
    /// 响度均衡线性倍数（与用户音量相乘；1.0 = 不补偿）
    rg_linear_bits: AtomicU32,
    /// 响度均衡模式：0 off / 1 track / 2 album
    rg_mode: AtomicU8,
    /// UI 展示用的本曲增益状态
    rg_info: Mutex<ReplayGainInfo>,
    track_ended: AtomicBool,
    /// gapless 过渡：新曲已装载、旧曲队尾仍在播。进度按新曲 0 报、回调不累加，
    /// 直到新曲首块锚点落地（否则快照会把上一曲末尾进度安到新曲上）
    gapless_tail: AtomicBool,
    /// 音频世代号：每次「应丢弃已缓冲音频」（seek/切歌/显式 flush）+1。
    /// 块带世代戳、回调只播当前世代——单布尔标志会把 flush 后新到的有效块一并冲掉
    audio_gen: Arc<AtomicU64>,
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
            rg_linear_bits: AtomicU32::new(1.0f32.to_bits()),
            rg_mode: AtomicU8::new(1), // 默认按曲目
            rg_info: Mutex::new(ReplayGainInfo::default()),
            track_ended: AtomicBool::new(false),
            gapless_tail: AtomicBool::new(false),
            audio_gen: Arc::new(AtomicU64::new(0)),
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

    fn rg_linear(&self) -> f32 {
        f32::from_bits(self.rg_linear_bits.load(Ordering::SeqCst))
    }

    fn rg_mode(&self) -> ReplayGainMode {
        match self.rg_mode.load(Ordering::SeqCst) {
            1 => ReplayGainMode::Track,
            2 => ReplayGainMode::Album,
            _ => ReplayGainMode::Off,
        }
    }

    fn set_rg_mode(&self, mode: ReplayGainMode) {
        let raw = match mode {
            ReplayGainMode::Off => 0,
            ReplayGainMode::Track => 1,
            ReplayGainMode::Album => 2,
        };
        self.rg_mode.store(raw, Ordering::SeqCst);
    }

    fn set_replaygain(&self, info: ReplayGainInfo) {
        let lin = info.linear();
        // 成对写入：命令线程与解码线程都可能写，交错会让 linear 与 info 来自不同版本
        match self.rg_info.lock() {
            Ok(mut g) => {
                *g = info;
                self.rg_linear_bits.store(lin.to_bits(), Ordering::SeqCst);
            }
            Err(p) => {
                *p.into_inner() = info;
                self.rg_linear_bits.store(lin.to_bits(), Ordering::SeqCst);
            }
        }
    }

    fn replaygain(&self) -> ReplayGainInfo {
        match self.rg_info.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
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
        // 显式 flush（seek/手动切歌）：退出 gapless 尾，并吞掉未消费的 track_ended
        self.gapless_tail.store(false, Ordering::SeqCst);
        self.track_ended.store(false, Ordering::SeqCst);
    }

    fn current_track_path(&self) -> Option<String> {
        self.track
            .lock()
            .ok()
            .and_then(|t| t.as_ref().map(|t| t.path.clone()))
    }
}

/// 为 path 解析并应用响度增益；无标签时后台异步估算（不写文件），完成后再平滑接入
fn apply_replaygain_for_path(shared: &Arc<Shared>, path: &Path) {
    let mode = shared.rg_mode();
    let (info, needs_analyze) = replaygain::resolve_for_playback(path, mode);
    shared.set_replaygain(info);
    if needs_analyze {
        spawn_rg_analyze(Arc::clone(shared), path.to_path_buf());
    }
}

fn spawn_rg_analyze(shared: Arc<Shared>, path: PathBuf) {
    if !replaygain::analysis_begin(&path) {
        return; // 该文件已在扫
    }
    let _ = std::thread::Builder::new()
        .name("axmusic-rg-scan".into())
        .spawn(move || {
            let result = replaygain::scan_track(&path);
            replaygain::analysis_end(&path);
            // scan_track 已在结果有效时写入估算缓存（带 mtime 门禁）
            let Ok(scan) = result else { return };
            let Some(gain) = scan.track_gain_db else { return };
            // 仍是当前曲才应用（用户可能已切歌）
            let same = shared
                .current_track_path()
                .map(|p| p == path.to_string_lossy())
                .unwrap_or(false);
            if same && shared.rg_mode() != ReplayGainMode::Off {
                shared.set_replaygain(replaygain::info_from_estimate(gain, scan.track_peak));
            }
        });
}

pub struct SymphoniaPlayer {
    shared: Arc<Shared>,
    cmd_tx: Sender<Cmd>,
    _worker: Option<std::thread::JoinHandle<()>>,
    output_sample_rate: u32,
    /// 频谱动效分接：回调侧生产者 + 前端订阅开关（viz 线程的消费者在此待取）
    viz_active: Arc<AtomicBool>,
    viz_consumer: Mutex<Option<ringbuf::HeapCons<f32>>>,
}

impl SymphoniaPlayer {
    pub fn new() -> Result<Self> {
        let (cmd_tx, cmd_rx) = unbounded::<Cmd>();
        let shared = Arc::new(Shared::new());
        let worker_shared = Arc::clone(&shared);
        let (viz_tap, viz_consumer) = viz::new_tap();
        let viz_active = Arc::clone(&viz_tap.active);

        let worker = std::thread::Builder::new()
            .name("axmusic-player".into())
            .spawn(move || worker_main(cmd_rx, worker_shared, viz_tap))
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
            viz_active,
            viz_consumer: Mutex::new(Some(viz_consumer)),
        })
    }

    /// 设备输出采样率（0 = 尚未就绪/设备异常）
    pub fn output_sample_rate(&self) -> u32 {
        self.output_sample_rate
    }

    /// 频谱动效订阅开关（前端开/关背景动效时调用）
    pub fn set_viz_active(&self, on: bool) {
        self.viz_active.store(on, Ordering::Relaxed);
    }

    pub fn viz_active_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.viz_active)
    }

    /// 音频世代句柄（viz 线程据此在切歌/seek 时清 ring，防残样闪跳）
    pub fn viz_gen_handle(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.shared.audio_gen)
    }

    /// 取出频谱 ring 的消费者（仅一次，给 viz 线程）
    pub fn take_viz_consumer(&self) -> Option<ringbuf::HeapCons<f32>> {
        self.viz_consumer.lock().ok()?.take()
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
            replaygain: self.shared.replaygain(),
            // 取出即清：前端每次轮询拿快照，同一条错误只弹一次
            error: self.shared.error.lock().ok().and_then(|mut e| e.take()),
        }
    }

    /// 响度均衡模式；改完立刻按当前曲重算增益（缺标签则后台估算）
    pub fn set_replaygain_mode(&mut self, mode: ReplayGainMode) {
        self.shared.set_rg_mode(mode);
        if let Some(track) = self.current_track() {
            apply_replaygain_for_path(&self.shared, Path::new(&track.path));
        } else {
            self.shared.set_replaygain(ReplayGainInfo::default());
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
        apply_replaygain_for_path(&self.shared, Path::new(&item.path));
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
    /// 同 path 已在队列里的跳过，避免同一首歌重复添加。
    /// **必须同步 worker 本地 queue**：EOF 自动切歌 / Next / 列表循环都读它，
    /// 只写 shared 会让后来加入的曲目进不了循环。
    pub fn enqueue(&mut self, items: Vec<QueueItem>) {
        if items.is_empty() {
            return;
        }
        let mut added: Vec<QueueItem> = Vec::new();
        if let Ok(mut q) = self.shared.queue.lock() {
            for item in items {
                if q.iter().any(|x| x.path == item.path) {
                    continue;
                }
                q.push(item.clone());
                added.push(item);
            }
        }
        if added.is_empty() {
            return;
        }
        // 不动音频缓冲：flush 会把当前曲目已解码的队尾冲掉，造成可闻断音
        let _ = self.cmd_tx.send(Cmd::ExtendQueue { items: added });
    }

    /// 移出队列第 `index` 首。移的是当前曲：有下一首则续播，否则停。
    pub fn remove_at(&mut self, index: usize) -> Result<()> {
        let mut items = match self.shared.queue.lock() {
            Ok(g) => g.clone(),
            Err(e) => e.into_inner().clone(),
        };
        if index >= items.len() {
            return Err(anyhow!("队列索引越界"));
        }
        items.remove(index);
        let old_index = self.shared.queue_index();
        let was_current = old_index == Some(index);
        let new_index = match old_index {
            Some(c) if c == index => {
                if items.is_empty() {
                    None
                } else {
                    Some(index.min(items.len() - 1))
                }
            }
            Some(c) if c > index => Some(c - 1),
            other => other,
        };
        if let Ok(mut g) = self.shared.queue.lock() {
            *g = items.clone();
        }
        if let Ok(mut g) = self.shared.queue_index.lock() {
            *g = new_index;
        }
        // worker 本地队列/历史下标对齐
        let _ = self.cmd_tx.send(Cmd::RemoveAt { index });

        if was_current {
            match new_index.and_then(|i| items.get(i).cloned()) {
                Some(item) => {
                    let info = TrackInfo {
                        path: item.path.clone(),
                        title: item.title.clone(),
                        duration_ms: item.duration_ms,
                        sample_rate: self.output_sample_rate,
                        channels: 2,
                    };
                    if let Ok(mut t) = self.shared.track.lock() {
                        *t = Some(info);
                    }
                    self.shared
                        .duration_ms
                        .store(item.duration_ms.max(1), Ordering::SeqCst);
                    self.shared.position_frames.store(0, Ordering::SeqCst);
                    self.shared.track_ended.store(false, Ordering::SeqCst);
                    self.shared.request_flush();
                    let _ = self.cmd_tx.send(Cmd::Open {
                        path: PathBuf::from(&item.path),
                        index: new_index,
                    });
                    let _ = self.cmd_tx.send(Cmd::Play);
                    self.shared.status.store(STATUS_PLAYING, Ordering::SeqCst);
                }
                None => {
                    if let Ok(mut t) = self.shared.track.lock() {
                        *t = None;
                    }
                    self.shared.status.store(STATUS_STOPPED, Ordering::SeqCst);
                    self.shared.request_flush();
                    let _ = self.cmd_tx.send(Cmd::Stop);
                }
            }
        }
        Ok(())
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
            apply_replaygain_for_path(&self.shared, Path::new(&info.path));
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
        apply_replaygain_for_path(&self.shared, path);
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
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    duration_ms: u64,
    src_sample_rate: u32,
    /// 源声道数（展示用；输出端统一转立体声再按设备映射）
    src_channels: u16,
    end: bool,
}

fn worker_main(cmd_rx: Receiver<Cmd>, shared: Arc<Shared>, viz_tap: VizTap) {
    let host = cpal::default_host();
    let Some(device) = host.default_output_device() else {
        set_error(&shared, Some("无默认音频输出设备".into()));
        return;
    };
    let Ok(default_config) = device.default_output_config() else {
        set_error(&shared, Some("读取输出配置失败".into()));
        return;
    };
    let out_rate = default_config.sample_rate();
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
        viz_prod: viz_tap.producer,
        viz_active: viz_tap.active,
        rg_smooth: 1.0,
    };

    let stream = match default_config.sample_format() {
        SampleFormat::F32 => device.build_output_stream(
            default_config.config(),
            move |data: &mut [f32], _| audio.fill(data),
            |e| eprintln!("audio stream error: {e}"),
            None,
        ),
        SampleFormat::I16 => {
            let mut tmp = Vec::new();
            device.build_output_stream(
                default_config.config(),
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
                default_config.config(),
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
    /// 频谱动效分接（mono f32，wait-free push；满则丢帧）
    viz_prod: ringbuf::HeapProd<f32>,
    viz_active: Arc<AtomicBool>,
    /// 响度增益平滑值（向 shared.rg_linear 靠拢，避免异步估算结果突然变响/变轻）
    rg_smooth: f32,
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
                        // 新曲首块真正开播：结束 gapless 尾，进度从 0 起累加
                        self.shared.gapless_tail.store(false, Ordering::SeqCst);
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
        use ringbuf::traits::Producer as _;
        let gen = self.shared.audio_gen.load(Ordering::SeqCst);
        if gen != self.cur_gen {
            self.cur_gen = gen;
            self.cur.clear();
            self.pos = 0;
            // 切歌/seek 的 ring 清理由 viz 线程监听 audio_gen 完成（producer 侧无 clear）
        }
        let user_vol = self.shared.volume();
        let rg_target = self.shared.rg_linear();
        let paused = self.shared.status.load(Ordering::SeqCst) != STATUS_PLAYING;
        let ch = self.out_channels;
        let frames = data.len() / ch;
        let mut frames_written = 0u64;
        // 增益平滑：约 80ms 时间常数（异步估算/切标签时不跳变）
        let sr = self.shared.sample_rate.load(Ordering::SeqCst).max(1) as f32;
        let rg_coeff = 1.0 - (-1.0 / (0.08 * sr)).exp();
        // 频谱分接缓冲（栈上，回调结束一次 push_slice；不分配不锁）
        let viz_on = !paused && self.viz_active.load(Ordering::Relaxed);
        let mut tap = [0.0f32; 4096];
        let mut tap_n = 0usize;

        for f in 0..frames {
            let o = f * ch;
            if paused {
                data[o..o + ch].fill(0.0);
                continue;
            }
            if self.pos >= self.cur.len() {
                // 换块前先结算已播帧：锚点会整段覆盖 position，不能把上一段尾巴算进新段
                if frames_written > 0 {
                    if !self.shared.gapless_tail.load(Ordering::SeqCst) {
                        self.shared
                            .position_frames
                            .fetch_add(frames_written, Ordering::SeqCst);
                    }
                    frames_written = 0;
                }
                self.pull_next();
            }
            if self.pos + 1 < self.cur.len() {
                // 源固定立体声交错；按设备声道数映射
                let ls = self.cur[self.pos];
                let rs = self.cur[self.pos + 1];
                self.pos += 2;
                if viz_on {
                    // 取音量前原始信号：动效幅度不随用户音量缩放
                    tap[tap_n] = (ls + rs) * 0.5;
                    tap_n += 1;
                    if tap_n == tap.len() {
                        let _ = self.viz_prod.push_slice(&tap);
                        tap_n = 0;
                    }
                }
                // 逐帧平滑增益，异步估算落地时不产生音量台阶
                self.rg_smooth += (rg_target - self.rg_smooth) * rg_coeff;
                let vol = user_vol * self.rg_smooth;
                let l = ls * vol;
                let r = rs * vol;
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
        if tap_n > 0 {
            let _ = self.viz_prod.push_slice(&tap[..tap_n]);
        }
        // 注：load(gapless_tail) 与 fetch_add 两步间，解码线程可能已置尾并归零 position，
        // 旧曲尾帧会短暂加到新曲进度上——有界良性毛刺（≤一个回调缓冲），新曲首块锚点 store(0) 必然校准
        if frames_written > 0 && !self.shared.gapless_tail.load(Ordering::SeqCst) {
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
    /// 源率≠设备率时的 sinc 重采样器（None = 同率直通）；fill_block 首调时按 out_rate 惰性建立
    resampler: Option<super::resampler::SincResampler>,
    /// 是否已按设备率判定过重采样需求（区分"未初始化"与"已判定直通"）
    resampler_decided: bool,
}

/// 预取的下一曲解码器：本曲播放中提前打开，EOF 切换时免开文件，队列衔接不空档
struct Prefetched {
    index: usize,
    path: PathBuf,
    dec: DecoderState,
}

/// 自动连播目标（与 EOF 路径同一套规则）：单曲循环重播；随机续抽；顺序看列表循环/播完停
fn auto_target(
    shared: &Shared,
    queue: &[QueueItem],
    queue_index: Option<usize>,
) -> Option<usize> {
    if queue.is_empty() {
        return None;
    }
    if shared.repeat() == RepeatMode::One {
        return queue_index.or(Some(0));
    }
    if shared.shuffle() {
        return shuffle_pick(queue.len(), queue_index);
    }
    queue_index.and_then(|i| {
        if i + 1 < queue.len() {
            Some(i + 1)
        } else if shared.repeat() == RepeatMode::All {
            Some(0)
        } else {
            None
        }
    })
}

/// 取本次连播应切到的下标：优先用开播时锁定的 planned（保证与预取一致），失效再重算。
/// 缓存若指向「当前曲」则视为过期（切歌后未及时作废会自己接自己）；单曲循环除外。
fn resolve_planned(
    shared: &Shared,
    queue: &[QueueItem],
    queue_index: Option<usize>,
    planned: &mut Option<(usize, u8)>,
) -> Option<usize> {
    let mode = shared.play_mode_raw();
    if let Some((i, m)) = *planned {
        let stale_self =
            Some(i) == queue_index && shared.repeat() != RepeatMode::One;
        if m == mode && queue.get(i).is_some() && !stale_self {
            return Some(i);
        }
    }
    let t = auto_target(shared, queue, queue_index);
    *planned = t.map(|i| (i, mode));
    t
}

/// 通道满（播放侧有存货）时预取下一曲解码器；失败不致命，EOF 时 open_track_full 兜底。
/// 仅在有余量时调用，避免开文件的 I/O 卡住解码线程。
fn try_prefetch(
    shared: &Shared,
    queue: &[QueueItem],
    queue_index: Option<usize>,
    planned: &mut Option<(usize, u8)>,
    prefetched: &mut Option<Prefetched>,
    prefetch_tried: &mut Option<usize>,
) {
    if prefetched.is_some() {
        return;
    }
    let Some(target) = resolve_planned(shared, queue, queue_index, planned) else {
        return;
    };
    // 同一目标只 open 一次（失败也不在 Full 循环里反复试），目标变了才再试
    if *prefetch_tried == Some(target) {
        return;
    }
    *prefetch_tried = Some(target);
    let Some(item) = queue.get(target) else {
        return;
    };
    let path = PathBuf::from(&item.path);
    match open_decoder(&path) {
        Ok(st) => {
            *prefetched = Some(Prefetched {
                index: target,
                path,
                dec: st,
            });
        }
        Err(_) => {
            // 预取失败不写 error（当前曲还在播）；EOF 走 open_track_full 时才报错
        }
    }
}

fn open_decoder(path: &Path) -> Result<DecoderState> {
    let file = File::open(path).with_context(|| format!("无法打开 {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let reader = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .context("无法解析音频容器")?;
    let track = reader
        .default_track(TrackType::Audio)
        .ok_or_else(|| anyhow!("无可用音频轨"))?
        .clone();
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio().cloned())
        .ok_or_else(|| anyhow!("音轨缺少编解码参数"))?;
    // Ogg 容器能识别 Opus 头但引擎没有解码器：明确告知，别让错误细节糊住重点
    if params.codec == CODEC_ID_OPUS {
        anyhow::bail!("Opus 编码暂不支持播放（待接入 FFmpeg 后端）；文件可正常入库管理");
    }
    // 0.6：时长优先取 Track.duration + time_base，回落 num_frames/采样率
    let duration_ms = match (track.time_base, track.duration) {
        (Some(tb), Some(dur)) => tb
            .calc_duration(dur)
            .map(|t| (t.as_secs_f64() * 1000.0) as u64)
            .unwrap_or(0),
        _ => match (track.num_frames, params.sample_rate) {
            (Some(frames), Some(rate)) if rate > 0 => frames * 1000 / rate as u64,
            _ => 0,
        },
    };
    let src_sample_rate = params.sample_rate.unwrap_or(44_100).max(1);
    let src_channels = params.channels.as_ref().map(|c| c.count() as u16).unwrap_or(2);
    let decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
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

/// 将已打开的解码器装入播放：写 shared 元数据、按需 flush。
/// `flush`=true 显式切歌/seek（丢弃已缓冲音频、进度归零）；
/// `flush`=false EOF 自动连播（保留队尾，进度展示按 0，首块锚点落地后开始累加）。
fn activate_decoder(
    shared: &Arc<Shared>,
    path: &Path,
    index: Option<usize>,
    queue_item: Option<&QueueItem>,
    st: DecoderState,
    flush: bool,
) -> DecoderState2 {
    if flush {
        shared.request_flush();
        shared.position_frames.store(0, Ordering::SeqCst);
        shared.track_ended.store(false, Ordering::SeqCst);
    } else {
        // gapless：已入通道的队尾继续播（不 flush）；进度按新曲 0 展示，
        // 由新曲首块锚点在队尾播完那一刻开始累加（见 AudioOut::fill）
        shared.gapless_tail.store(true, Ordering::SeqCst);
        shared.position_frames.store(0, Ordering::SeqCst);
    }

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
    // 读标签/缓存算响度增益；无标签时后台估算（含峰值限幅），UI 标识随 snapshot 带出
    apply_replaygain_for_path(shared, path);
    set_error(shared, None);
    DecoderState2 {
        inner: st,
        pending: Vec::new(),
        resampler: None,
        resampler_decided: false,
    }
}

/// `flush`：是否丢弃已缓冲音频。显式切歌/seek 要；EOF 自动连播不要（保留队尾，无缝衔接）。
fn open_track_full(
    shared: &Arc<Shared>,
    path: &Path,
    index: Option<usize>,
    queue_item: Option<&QueueItem>,
    flush: bool,
) -> Option<DecoderState2> {
    match open_decoder(path) {
        Ok(st) => Some(activate_decoder(shared, path, index, queue_item, st, flush)),
        Err(err) => {
            set_error(shared, Some(format!("{err:#}")));
            if let Ok(mut t) = shared.track.lock() {
                *t = None;
            }
            shared.set_replaygain(ReplayGainInfo::default());
            shared.status.store(STATUS_STOPPED, Ordering::SeqCst);
            None
        }
    }
}

fn seek_decoder(st: &mut DecoderState, ms: u64) -> Result<()> {
    let time = Time::from_nanos_u64(ms.saturating_mul(1_000_000));
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
            Ok(Some(p)) => p,
            // 0.6：EOF = Ok(None)；IoError(UnexpectedEof) 分支留作防御
            Ok(None) => {
                st.end = true;
                return Ok(None);
            }
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
        if packet.track_id != st.track_id {
            continue;
        }
        match st.decoder.decode(&packet) {
            Ok(buf) => return Ok(Some(to_interleaved_stereo(&buf))),
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(e),
        }
    }
}

fn pack_planes<S: ConvertibleSample>(buf: &AudioBuffer<S>, convert: impl Fn(S) -> f32) -> Vec<f32> {
    match buf.num_planes() {
        0 => Vec::new(),
        1 => {
            let mono = buf.plane(0).unwrap_or(&[]);
            let mut out = Vec::with_capacity(mono.len() * 2);
            for &s in mono {
                let v = convert(s);
                out.push(v);
                out.push(v);
            }
            out
        }
        _ => match buf.plane_pair(0, 1) {
            Some((l, r)) => {
                let n = l.len().min(r.len());
                let mut out = Vec::with_capacity(n * 2);
                for i in 0..n {
                    out.push(convert(l[i]));
                    out.push(convert(r[i]));
                }
                out
            }
            None => Vec::new(),
        },
    }
}

fn to_interleaved_stereo(buf: &GenericAudioBufferRef<'_>) -> Vec<f32> {
    match buf {
        GenericAudioBufferRef::F32(b) => pack_planes(*b, |s: f32| s),
        GenericAudioBufferRef::U8(b) => pack_planes(*b, |s: u8| (s as f32 - 128.0) / 128.0),
        GenericAudioBufferRef::U16(b) => {
            pack_planes(*b, |s: u16| (s as f32 / 32768.0) - 1.0)
        }
        GenericAudioBufferRef::U24(b) => pack_planes(*b, |s: u24| {
            (s.inner() as f32 / 8_388_608.0) - 1.0
        }),
        GenericAudioBufferRef::U32(b) => {
            pack_planes(*b, |s: u32| (s as f32 / 2_147_483_648.0) - 1.0)
        }
        GenericAudioBufferRef::S8(b) => pack_planes(*b, |s: i8| s as f32 / 128.0),
        GenericAudioBufferRef::S16(b) => pack_planes(*b, |s: i16| s as f32 / 32768.0),
        GenericAudioBufferRef::S24(b) => pack_planes(*b, |s: i24| {
            s.inner() as f32 / 8_388_608.0
        }),
        GenericAudioBufferRef::S32(b) => {
            pack_planes(*b, |s: i32| s as f32 / 2_147_483_648.0)
        }
        GenericAudioBufferRef::F64(b) => pack_planes(*b, |s: f64| s as f32),
    }
}

fn fill_block(dec: &mut DecoderState2, out_rate: u32) -> Option<Vec<f32>> {
    // 惰性判定：源率 == 设备率则直通（bit-perfect 前提，见 docs/播放引擎调研.md §4.4）
    if !dec.resampler_decided {
        dec.resampler_decided = true;
        if dec.inner.src_sample_rate != out_rate {
            dec.resampler = Some(super::resampler::SincResampler::new(
                dec.inner.src_sample_rate,
                out_rate,
            ));
        }
    }
    while dec.pending.len() < BLOCK_SAMPLES {
        match decode_chunk_native(&mut dec.inner) {
            Ok(Some(native)) => match dec.resampler.as_mut() {
                Some(r) => r.process(&native, &mut dec.pending),
                None => dec.pending.extend_from_slice(&native),
            },
            Ok(None) => {
                // EOF：补零挤出重采样器尾部（幂等），总长帧数守恒
                if let Some(r) = dec.resampler.as_mut() {
                    r.flush(&mut dec.pending);
                }
                break;
            }
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
    // 开播时锁定的连播下标 (index, play_mode)：EOF 与预取共用，避免随机抽两次对不上
    let mut planned: Option<(usize, u8)> = None;
    // 预取的下一曲解码器（本曲播放中提前 open，EOF 时直接换上，衔接不空档）
    let mut prefetched: Option<Prefetched> = None;
    // 已对哪个下标试过预取（含失败）：避免开文件失败后在 Full 循环里反复重试
    let mut prefetch_tried: Option<usize> = None;
    // 上次看到的 play_mode：中途改随机/循环要作废预取（目标可能变）
    let mut last_mode = shared.play_mode_raw();

    /// 队列/模式变化后作废旧预取，避免切到错曲
    fn drop_prefetch(
        prefetched: &mut Option<Prefetched>,
        planned: &mut Option<(usize, u8)>,
        prefetch_tried: &mut Option<usize>,
    ) {
        *prefetched = None;
        *planned = None;
        *prefetch_tried = None;
    }

    loop {
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Cmd::PlayQueue { items, start } => {
                    queue = items;
                    queue_index = Some(start);
                    history.clear();
                    drop_prefetch(&mut prefetched, &mut planned, &mut prefetch_tried);
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
                            // 锁定下一曲（随机也只抽这一次），供预取与 EOF 共用
                            let _ = resolve_planned(&shared, &queue, queue_index, &mut planned);
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
                    drop_prefetch(&mut prefetched, &mut planned, &mut prefetch_tried);
                    let item = index.and_then(|i| queue.get(i).cloned());
                    dec = open_track_full(&shared, &path, index, item.as_ref(), true);
                    seg_anchor = if dec.is_some() { Some(0) } else { None };
                    if dec.is_some() {
                        let _ = resolve_planned(&shared, &queue, queue_index, &mut planned);
                    }
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
                            drop_prefetch(&mut prefetched, &mut planned, &mut prefetch_tried);
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
                            if playing {
                                let _ =
                                    resolve_planned(&shared, &queue, queue_index, &mut planned);
                            }
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
                            // seek 后源从新位置吐帧：重采样历史必须清空，否则带出前一段残留
                            if let Some(r) = d.resampler.as_mut() {
                                r.reset();
                            }
                            shared.request_flush();
                            pending_block = None;
                            // seek 不换曲：预取仍有效，但 planned 要按新位置重估不必要——同曲下一首不变
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
                    drop_prefetch(&mut prefetched, &mut planned, &mut prefetch_tried);
                    if let Ok(mut qi) = shared.queue_index.lock() {
                        *qi = Some(start);
                    }
                    if let Ok(mut q) = shared.queue.lock() {
                        *q = queue.clone();
                    }
                }
                Cmd::RemoveAt { index } => {
                    if index < queue.len() {
                        queue.remove(index);
                    }
                    queue_index = match queue_index {
                        Some(c) if c == index => {
                            if queue.is_empty() {
                                None
                            } else {
                                Some(index.min(queue.len().saturating_sub(1)))
                            }
                        }
                        Some(c) if c > index => Some(c - 1),
                        other => other,
                    };
                    history.retain_mut(|h| {
                        if *h == index {
                            false
                        } else {
                            if *h > index {
                                *h -= 1;
                            }
                            true
                        }
                    });
                    drop_prefetch(&mut prefetched, &mut planned, &mut prefetch_tried);
                    if let Ok(mut q) = shared.queue.lock() {
                        *q = queue.clone();
                    }
                    if let Ok(mut qi) = shared.queue_index.lock() {
                        *qi = queue_index;
                    }
                    if dec.is_some() {
                        let _ = resolve_planned(&shared, &queue, queue_index, &mut planned);
                    }
                }
                Cmd::ExtendQueue { items } => {
                    for item in items {
                        if !queue.iter().any(|x| x.path == item.path) {
                            queue.push(item);
                        }
                    }
                    // 追加可能改变「下一曲」：顺序回绕目标、随机抽取池、播完停→有下一首。
                    // 仅按越界判失效不够（回绕目标 0 永不越界）；追加低频，统一作废重估
                    drop_prefetch(&mut prefetched, &mut planned, &mut prefetch_tried);
                    if dec.is_some() {
                        let _ = resolve_planned(&shared, &queue, queue_index, &mut planned);
                    }
                    // shared 已由 enqueue 写过完整队列；这里不回写，避免用旧本地覆盖
                }
                Cmd::RestoreSession {
                    items,
                    start,
                    position_ms,
                } => {
                    queue = items;
                    queue_index = if queue.is_empty() { None } else { Some(start) };
                    history.clear();
                    drop_prefetch(&mut prefetched, &mut planned, &mut prefetch_tried);
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
                                if let Some(r) = d.resampler.as_mut() {
                                    r.reset();
                                }
                                let rate = shared.sample_rate.load(Ordering::SeqCst).max(1);
                                let frames = position_ms.saturating_mul(rate) / 1000;
                                shared.position_frames.store(frames, Ordering::SeqCst);
                                seg_anchor = Some(frames);
                            }
                        }
                        playing = false;
                        shared.status.store(STATUS_PAUSED, Ordering::SeqCst);
                        if dec.is_some() {
                            let _ = resolve_planned(&shared, &queue, queue_index, &mut planned);
                        }
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
                            drop_prefetch(&mut prefetched, &mut planned, &mut prefetch_tried);
                            queue_index = Some(target);
                            if let Ok(mut qi) = shared.queue_index.lock() {
                                *qi = Some(target);
                            }
                            dec = open_track_full(&shared, &path, queue_index, Some(&item), true);
                            if dec.is_some() {
                                playing = true;
                                seg_anchor = Some(0);
                                shared.status.store(STATUS_PLAYING, Ordering::SeqCst);
                                let _ =
                                    resolve_planned(&shared, &queue, queue_index, &mut planned);
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
                                drop_prefetch(&mut prefetched, &mut planned, &mut prefetch_tried);
                                dec = open_track_full(&shared, &path, Some(idx), Some(&item), true);
                                if dec.is_some() {
                                    playing = true;
                                    seg_anchor = Some(0);
                                    shared.status.store(STATUS_PLAYING, Ordering::SeqCst);
                                    let _ =
                                        resolve_planned(&shared, &queue, queue_index, &mut planned);
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
                                drop_prefetch(&mut prefetched, &mut planned, &mut prefetch_tried);
                                queue_index = Some(target);
                                if let Ok(mut qi) = shared.queue_index.lock() {
                                    *qi = Some(target);
                                }
                                dec = open_track_full(&shared, &path, queue_index, Some(&item), true);
                                if dec.is_some() {
                                    playing = true;
                                    seg_anchor = Some(0);
                                    shared.status.store(STATUS_PLAYING, Ordering::SeqCst);
                                    let _ =
                                        resolve_planned(&shared, &queue, queue_index, &mut planned);
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
                    drop_prefetch(&mut prefetched, &mut planned, &mut prefetch_tried);
                    shared.request_flush();
                    shared.status.store(STATUS_STOPPED, Ordering::SeqCst);
                    if let Ok(mut t) = shared.track.lock() {
                        *t = None;
                    }
                }
            }
        }

        if playing {
            // 中途改随机/循环：旧预取目标可能变，作废后按新模式重估
            let mode = shared.play_mode_raw();
            if mode != last_mode {
                last_mode = mode;
                drop_prefetch(&mut prefetched, &mut planned, &mut prefetch_tried);
                if dec.is_some() {
                    let _ = resolve_planned(&shared, &queue, queue_index, &mut planned);
                }
            }
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
                                // 通道满 = 播放侧存货充足，正是预取的好时机（I/O 不卡出声）
                                try_prefetch(
                                    &shared,
                                    &queue,
                                    queue_index,
                                    &mut planned,
                                    &mut prefetched,
                                    &mut prefetch_tried,
                                );
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
                            // 自动切歌：单曲循环重播；随机续抽；顺序看列表循环/播完停。
                            // 用 resolve_planned（开播时锁定）保证与预取同一首，不重复抽随机
                            let target =
                                resolve_planned(&shared, &queue, queue_index, &mut planned);
                            if let Some(target) = target {
                                if let Some(item) = queue.get(target).cloned() {
                                    let path = PathBuf::from(&item.path);
                                    pending_block = None;
                                    queue_index = Some(target);
                                    if let Ok(mut qi) = shared.queue_index.lock() {
                                        *qi = Some(target);
                                    }
                                    // 优先吃预取：免 open_decoder，衔接零空档
                                    let pre = prefetched.take().filter(|p| {
                                        p.index == target && p.path == path
                                    });
                                    dec = match pre {
                                        Some(p) => {
                                            Some(activate_decoder(
                                                &shared,
                                                &path,
                                                queue_index,
                                                Some(&item),
                                                p.dec,
                                                false,
                                            ))
                                        }
                                        None => open_track_full(
                                            &shared, &path, queue_index, Some(&item), false,
                                        ),
                                    };
                                    advanced = dec.is_some();
                                    if advanced {
                                        seg_anchor = Some(0);
                                        // 旧 planned 指向刚切到的这首，先作废再锁再下一曲
                                        //（否则 resolve_planned 命中自己，下一首又播同一首）
                                        planned = None;
                                        prefetch_tried = None;
                                        let _ = resolve_planned(
                                            &shared,
                                            &queue,
                                            queue_index,
                                            &mut planned,
                                        );
                                    }
                                }
                            }
                        }
                        if !advanced {
                            dec = None;
                            playing = false;
                            seg_anchor = None;
                            drop_prefetch(&mut prefetched, &mut planned, &mut prefetch_tried);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 最小合法 AIFF（PCM s16be）：FORM/AIFF + COMM + SSND
    fn build_aiff_s16(frames: usize, rate: u32, channels: u16, freq: f64) -> Vec<u8> {
        // 80-bit IEEE754 extended：44100 → 指数 16383+15，尾数左对齐
        fn extended80(v: u32) -> [u8; 10] {
            assert!(v > 0);
            let msb = 31 - v.leading_zeros();
            let exp = (16383 + msb) as u16;
            let mantissa = (v as u64) << (63 - msb);
            let mut out = [0u8; 10];
            out[0] = (exp >> 8) as u8;
            out[1] = exp as u8;
            out[2..10].copy_from_slice(&mantissa.to_be_bytes());
            out
        }
        let data_len = frames * channels as usize * 2;
        let mut v = Vec::new();
        v.extend_from_slice(b"FORM");
        v.extend_from_slice(&(4 + (8 + 18) + (8 + 8 + data_len) as u32).to_be_bytes());
        v.extend_from_slice(b"AIFF");
        v.extend_from_slice(b"COMM");
        v.extend_from_slice(&18u32.to_be_bytes());
        v.extend_from_slice(&channels.to_be_bytes());
        v.extend_from_slice(&(frames as u32).to_be_bytes());
        v.extend_from_slice(&16u16.to_be_bytes());
        v.extend_from_slice(&extended80(rate));
        v.extend_from_slice(b"SSND");
        v.extend_from_slice(&(8 + data_len as u32).to_be_bytes());
        v.extend_from_slice(&0u32.to_be_bytes()); // offset
        v.extend_from_slice(&0u32.to_be_bytes()); // blocksize
        for i in 0..frames {
            let s = (2.0 * std::f64::consts::PI * freq * i as f64 / rate as f64).sin();
            let q = (s * 32767.0) as i16;
            for _ in 0..channels {
                v.extend_from_slice(&q.to_be_bytes());
            }
        }
        v
    }

    /// AIFF 假支持回归：能 probe、能解码、帧数与能量对得上（0.6 起 aiff feature 生效）
    #[test]
    fn aiff_decodes_end_to_end() {
        let frames = 4410usize; // 0.1s @44.1k
        let bytes = build_aiff_s16(frames, 44_100, 2, 1000.0);
        let dir = std::env::temp_dir().join(format!("axmusic-aiff-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.aiff");
        std::fs::write(&path, &bytes).unwrap();

        let mut st = open_decoder(&path).expect("AIFF 应能打开");
        assert_eq!(st.src_sample_rate, 44_100);
        assert_eq!(st.src_channels, 2);
        assert_eq!(st.duration_ms, 100);

        let mut total_frames = 0usize;
        let mut energy = 0f64;
        while let Ok(Some(block)) = decode_chunk_native(&mut st) {
            total_frames += block.len() / 2;
            energy += block.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>();
        }
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(total_frames, frames, "解码帧数应精确等于源帧数");
        let rms = (energy / (total_frames * 2) as f64).sqrt();
        assert!(
            (rms - std::f64::consts::FRAC_1_SQRT_2).abs() < 0.05,
            "满幅 1kHz 正弦 RMS 应≈0.707，实得 {rms:.3}（说明解出了真实波形而非静音）"
        );
    }
}
