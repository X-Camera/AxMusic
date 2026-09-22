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

use super::{PlayerEngine, PlayStatus, PlayerSnapshot, QueueItem, TrackInfo};

const STATUS_STOPPED: u8 = 0;
const STATUS_PLAYING: u8 = 1;
const STATUS_PAUSED: u8 = 2;
/// Stereo interleaved samples per block.
const BLOCK_SAMPLES: usize = 2048;
/// Blocks kept in flight (~0.15s at 1024 frames).
const BLOCK_QUEUE: usize = 8;

enum Cmd {
    Open { path: PathBuf, index: Option<usize> },
    Play,
    Pause,
    Seek { ms: u64 },
    SetVolume { v: f32 },
    SetQueue { items: Vec<QueueItem>, start: usize },
    Next,
    Prev,
    Stop,
}

struct Shared {
    status: AtomicU8,
    position_frames: AtomicU64,
    sample_rate: AtomicU64,
    duration_ms: AtomicU64,
    volume_bits: AtomicU32,
    track_ended: AtomicBool,
    flush_audio: AtomicBool,
    track: Mutex<Option<TrackInfo>>,
    queue: Mutex<Vec<QueueItem>>,
    queue_index: Mutex<Option<usize>>,
    error: Mutex<Option<String>>,
}

impl Shared {
    fn new() -> Self {
        Self {
            status: AtomicU8::new(STATUS_STOPPED),
            position_frames: AtomicU64::new(0),
            sample_rate: AtomicU64::new(48_000),
            duration_ms: AtomicU64::new(0),
            volume_bits: AtomicU32::new(0.8f32.to_bits()),
            track_ended: AtomicBool::new(false),
            flush_audio: AtomicBool::new(false),
            track: Mutex::new(None),
            queue: Mutex::new(Vec::new()),
            queue_index: Mutex::new(None),
            error: Mutex::new(None),
        }
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
        self.flush_audio.store(true, Ordering::SeqCst);
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

        for _ in 0..80 {
            if shared.error.lock().map(|e| e.is_some()).unwrap_or(false) {
                break;
            }
            if shared.sample_rate.load(Ordering::SeqCst) != 48_000
                || shared.sample_rate.load(Ordering::SeqCst) > 0
            {
                // wait until worker publishes device rate (starts at 48000; real value overwrites)
                if shared
                    .error
                    .lock()
                    .map(|e| e.is_some())
                    .unwrap_or(false)
                {
                    break;
                }
                // break once worker has run a moment — sample_rate always > 0
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
        }
    }

    pub fn play_index(&mut self, index: usize) -> Result<TrackInfo> {
        let item = {
            let q = self.shared.queue.lock().unwrap();
            q.get(index).cloned()
        };
        let item = item.ok_or_else(|| anyhow!("队列中无此曲目"))?;
        let path = PathBuf::from(&item.path);
        let _ = self.cmd_tx.send(Cmd::Open {
            path,
            index: Some(index),
        });
        let _ = self.cmd_tx.send(Cmd::Play);
        Ok(TrackInfo {
            path: item.path,
            title: item.title,
            duration_ms: item.duration_ms,
            sample_rate: self.output_sample_rate,
            channels: 2,
        })
    }

    pub fn next(&mut self) -> Result<Option<TrackInfo>> {
        let _ = self.cmd_tx.send(Cmd::Next);
        Ok(self.current_track())
    }

    pub fn prev(&mut self) -> Result<Option<TrackInfo>> {
        let _ = self.cmd_tx.send(Cmd::Prev);
        Ok(self.current_track())
    }

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
        let _ = self.cmd_tx.send(Cmd::Open {
            path: path.to_path_buf(),
            index: None,
        });
        Ok(quick_track_info(path))
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
    end: bool,
}

fn worker_main(cmd_rx: Receiver<Cmd>, shared: Arc<Shared>) {
    let host = cpal::default_host();
    let Some(device) = host.default_output_device() else {
        *shared.error.lock().unwrap() = Some("无默认音频输出设备".into());
        return;
    };
    let Ok(default_config) = device.default_output_config() else {
        *shared.error.lock().unwrap() = Some("读取输出配置失败".into());
        return;
    };
    let out_rate = default_config.sample_rate().0;
    shared.sample_rate.store(out_rate as u64, Ordering::SeqCst);

    let (block_tx, block_rx) = bounded::<Vec<f32>>(BLOCK_QUEUE);
    let mut audio = AudioOut {
        cur: Vec::new(),
        pos: 0,
        block_rx,
        shared: Arc::clone(&shared),
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
            *shared.error.lock().unwrap() = Some("不支持的采样格式".into());
            return;
        }
    };

    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            *shared.error.lock().unwrap() = Some(format!("打开音频输出失败: {e}"));
            return;
        }
    };
    if let Err(e) = stream.play() {
        *shared.error.lock().unwrap() = Some(format!("启动音频输出失败: {e}"));
        return;
    }

    // cpal::Stream is !Send — keep on this thread.
    let _stream: Stream = stream;
    decode_loop(cmd_rx, shared, block_tx, out_rate);
}

struct AudioOut {
    cur: Vec<f32>,
    pos: usize,
    block_rx: Receiver<Vec<f32>>,
    shared: Arc<Shared>,
}

impl AudioOut {
    fn fill(&mut self, data: &mut [f32]) {
        if self.shared.flush_audio.swap(false, Ordering::SeqCst) {
            self.cur.clear();
            self.pos = 0;
            while self.block_rx.try_recv().is_ok() {}
        }
        let vol = self.shared.volume();
        let paused = self.shared.status.load(Ordering::SeqCst) != STATUS_PLAYING;
        let n = data.len();
        let mut frames_written = 0u64;

        for i in 0..n {
            if paused {
                data[i] = 0.0;
                continue;
            }
            if self.pos >= self.cur.len() {
                self.cur = self.block_rx.try_recv().unwrap_or_default();
                self.pos = 0;
            }
            if self.pos < self.cur.len() {
                data[i] = self.cur[self.pos] * vol;
                self.pos += 1;
                if i % 2 == 0 {
                    frames_written += 1;
                }
            } else {
                data[i] = 0.0;
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
    let mut reader = probed.format;
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
    let decoder = symphonia::default::get_codecs()
        .make(&params, &DecoderOptions::default())
        .context("不支持的编解码器")?;
    Ok(DecoderState {
        reader,
        decoder,
        track_id,
        duration_ms,
        src_sample_rate,
        end: false,
    })
}

fn open_track_full(
    shared: &Shared,
    path: &Path,
    index: Option<usize>,
    queue_item: Option<&QueueItem>,
) -> Option<DecoderState2> {
    shared.request_flush();
    shared.position_frames.store(0, Ordering::SeqCst);
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
                channels: 2,
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
            *shared.error.lock().unwrap() = None;
            Some(DecoderState2 {
                inner: st,
                pending: Vec::new(),
                resample_pos: 0.0,
            })
        }
        Err(err) => {
            *shared.error.lock().unwrap() = Some(format!("{err:#}"));
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

fn decode_loop(
    cmd_rx: Receiver<Cmd>,
    shared: Arc<Shared>,
    block_tx: Sender<Vec<f32>>,
    out_rate: u32,
) {
    let mut dec: Option<DecoderState2> = None;
    let mut playing = false;
    let mut queue: Vec<QueueItem> = Vec::new();
    let mut queue_index: Option<usize> = None;

    // drop leftover blocks helper
    let flush = || {
        shared.request_flush();
    };

    loop {
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Cmd::Open { path, index } => {
                    flush();
                    let item = index.and_then(|i| queue.get(i).cloned());
                    dec = open_track_full(&shared, &path, index, item.as_ref());
                }
                Cmd::Play => {
                    playing = true;
                    shared.status.store(STATUS_PLAYING, Ordering::SeqCst);
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
                            flush();
                            let rate = shared.sample_rate.load(Ordering::SeqCst).max(1);
                            shared
                                .position_frames
                                .store(ms * rate / 1000, Ordering::SeqCst);
                        }
                    }
                }
                Cmd::SetVolume { v } => {
                    shared.volume_bits.store(v.to_bits(), Ordering::SeqCst);
                }
                Cmd::SetQueue { items, start } => {
                    queue = items;
                    queue_index = Some(start);
                    if let Ok(mut qi) = shared.queue_index.lock() {
                        *qi = Some(start);
                    }
                    if let Ok(mut q) = shared.queue.lock() {
                        *q = queue.clone();
                    }
                }
                Cmd::Next => {
                    if let Some(idx) = queue_index {
                        if idx + 1 < queue.len() {
                            let item = queue[idx + 1].clone();
                            let path = PathBuf::from(&item.path);
                            flush();
                            queue_index = Some(idx + 1);
                            dec = open_track_full(&shared, &path, queue_index, Some(&item));
                            if dec.is_some() {
                                playing = true;
                                shared.status.store(STATUS_PLAYING, Ordering::SeqCst);
                            }
                        }
                    }
                }
                Cmd::Prev => {
                    if let Some(idx) = queue_index {
                        let target = idx.saturating_sub(1);
                        if let Some(item) = queue.get(target).cloned() {
                            let path = PathBuf::from(&item.path);
                            flush();
                            queue_index = Some(target);
                            dec = open_track_full(&shared, &path, queue_index, Some(&item));
                            if dec.is_some() {
                                playing = true;
                                shared.status.store(STATUS_PLAYING, Ordering::SeqCst);
                            }
                        }
                    }
                }
                Cmd::Stop => {
                    dec = None;
                    playing = false;
                    flush();
                    shared.status.store(STATUS_STOPPED, Ordering::SeqCst);
                    if let Ok(mut t) = shared.track.lock() {
                        *t = None;
                    }
                }
            }
        }

        if playing {
            if let Some(d) = dec.as_mut() {
                match fill_block(d, out_rate) {
                    Some(block) => {
                        // block if queue full (backpressure) so we don't spin
                        if block_tx.send(block).is_err() {
                            // audio side gone
                            playing = false;
                        }
                    }
                    None => {
                        // EOF — wait until output drains (blocks already in queue)
                        shared.track_ended.store(true, Ordering::SeqCst);
                        let advanced = if let Some(idx) = queue_index {
                            if idx + 1 < queue.len() {
                                let item = queue[idx + 1].clone();
                                let path = PathBuf::from(&item.path);
                                flush();
                                queue_index = Some(idx + 1);
                                let d2 = open_track_full(&shared, &path, queue_index, Some(&item));
                                dec = d2;
                                dec.is_some()
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        if !advanced {
                            dec = None;
                            playing = false;
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
