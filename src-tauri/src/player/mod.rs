//! PlayerEngine — thin playback pipeline (Symphonia decode + cpal output).
//! See docs/技术架构.md §2. Interface kept swappable for a future backend.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub mod engine;

pub use engine::SymphoniaPlayer;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackInfo {
    pub path: String,
    pub title: String,
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub path: String,
    pub title: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlayStatus {
    Stopped,
    Playing,
    Paused,
}

/// 循环模式（业界三态：关 → 列表循环 → 单曲循环；与随机正交）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    /// 顺序播完即停
    #[default]
    Off,
    /// 列表循环
    All,
    /// 单曲循环
    One,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSnapshot {
    pub status: PlayStatus,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: f32,
    pub track: Option<TrackInfo>,
    pub queue: Vec<QueueItem>,
    pub queue_index: Option<usize>,
    /// 随机播放（与循环独立）
    #[serde(default)]
    pub shuffle: bool,
    /// 循环模式
    #[serde(default)]
    pub repeat: RepeatMode,
}

/// Swappable playback engine surface (thin).
/// 契约样板（docs/技术架构.md §2）：现只有 Symphonia 一个实现、经 `Player` 直调，
/// 换后端（如 libmpv）时按此接口实现即可。
#[allow(dead_code)]
pub trait PlayerEngine {
    fn open(&mut self, path: &Path) -> Result<TrackInfo>;
    fn play(&mut self);
    fn pause(&mut self);
    fn seek(&mut self, ms: u64) -> Result<()>;
    fn set_queue(&mut self, items: Vec<QueueItem>);
    fn set_output_device(&mut self, id: &str) -> Result<()>;
    fn position(&self) -> u64;
    fn volume(&self) -> f32;
    fn set_volume(&mut self, v: f32);
}

/// Shared app-level player façade used by Tauri commands.
pub struct Player {
    pub engine: SymphoniaPlayer,
}

impl Player {
    pub fn new() -> Result<Self> {
        Ok(Self {
            engine: SymphoniaPlayer::new()?,
        })
    }

    pub fn snapshot(&self) -> PlayerSnapshot {
        self.engine.snapshot()
    }

    /// Play a single file (no queue change if empty).
    pub fn play_path(&mut self, path: &Path) -> Result<TrackInfo> {
        self.engine.play_path_at(path)
    }

    /// Play queue starting at index (items = full queue).
    /// 队列先同步写入 shared，再下发指令，避免首次点击时 play_index 读到空队列。
    pub fn play_queue(&mut self, items: Vec<QueueItem>, start: usize) -> Result<TrackInfo> {
        self.engine.play_queue_at(items, start)
    }

    /// 追加到当前队列末尾，排队等播放（不替换、不打断当前曲）。
    pub fn enqueue(&mut self, items: Vec<QueueItem>) {
        self.engine.enqueue(items);
    }

    /// 启动恢复上次播放列表（暂停在上次进度，不自动播）。
    pub fn restore_session(
        &mut self,
        items: Vec<QueueItem>,
        start: usize,
        position_ms: u64,
    ) -> Result<Option<TrackInfo>> {
        self.engine.restore_session(items, start, position_ms)
    }

    pub fn next(&mut self) -> Result<Option<TrackInfo>> {
        self.engine.next()
    }

    pub fn prev(&mut self) -> Result<Option<TrackInfo>> {
        self.engine.prev()
    }

    pub fn set_shuffle(&mut self, on: bool) {
        self.engine.set_shuffle(on);
    }

    pub fn set_repeat(&mut self, mode: RepeatMode) {
        self.engine.set_repeat(mode);
    }

    /// Called periodically: auto-advance when track finished.
    pub fn tick(&mut self) -> Option<TrackInfo> {
        self.engine.poll_track_ended()
    }
}
