//! App settings persisted as JSON (`data_root/settings.json`).
//! Library working DB lives next to the music library, not here.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// 播放模式（顺序 / 随机 / 单曲）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlayMode {
    #[default]
    Sequential,
    Shuffle,
    RepeatOne,
}

/// 歌词默认保存方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LyricsSaveMode {
    #[default]
    Sidecar,
    Embed,
}

/// 满窗/播放页读取歌词时的优先来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LyricsPrefer {
    #[default]
    Sidecar,
    Embed,
}

/// 歌曲页默认视图
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SongsView {
    #[default]
    List,
    Grid,
}

/// 关闭主窗口行为：每次询问 / 缩到托盘 / 直接退出
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CloseBehavior {
    #[default]
    Ask,
    Tray,
    Exit,
}

/// 主题色配色（暗色底上的强调色方案）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ColorScheme {
    #[default]
    Nebula,
    Sky,
    Jade,
    Rose,
    Amber,
    Coral,
    /// 中性石墨（灰阶强调，无彩色偏）
    Graphite,
}

/// 外观：暗色 / 浅色
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

/// 歌词在线源开关
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricsSources {
    pub lrclib: bool,
    pub netease: bool,
    pub qq: bool,
}

impl Default for LyricsSources {
    fn default() -> Self {
        Self {
            lrclib: true,
            netease: true,
            qq: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// Absolute path of the library root (music + axmusic.db live here).
    pub library_root: Option<String>,
    /// 0.0 ..= 1.0
    #[serde(default = "default_volume")]
    pub volume: f32,
    /// 播放模式（P0 先落盘，P1 接引擎）
    #[serde(default)]
    pub play_mode: PlayMode,
    /// 启动时恢复上次音量
    #[serde(default = "default_true")]
    pub restore_volume: bool,
    /// 歌词默认保存：外挂 .lrc / 内嵌
    #[serde(default)]
    pub lyrics_save_mode: LyricsSaveMode,
    /// 播放页读取优先：外挂 / 内嵌
    #[serde(default)]
    pub lyrics_prefer: LyricsPrefer,
    /// 在线歌词源开关
    #[serde(default)]
    pub lyrics_sources: LyricsSources,
    /// 歌曲页默认视图
    #[serde(default)]
    pub songs_view: SongsView,
    /// 关闭主窗口：询问 / 缩到托盘 / 退出
    #[serde(default)]
    pub close_behavior: CloseBehavior,
    /// 外观：暗色 / 浅色
    #[serde(default)]
    pub theme_mode: ThemeMode,
    /// 皮肤（表面 + 强调色家族）
    #[serde(default)]
    pub color_scheme: ColorScheme,
}

fn default_volume() -> f32 {
    0.8
}

fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            library_root: None,
            volume: default_volume(),
            play_mode: PlayMode::default(),
            restore_volume: true,
            lyrics_save_mode: LyricsSaveMode::default(),
            lyrics_prefer: LyricsPrefer::default(),
            lyrics_sources: LyricsSources::default(),
            songs_view: SongsView::default(),
            close_behavior: CloseBehavior::default(),
            theme_mode: ThemeMode::default(),
            color_scheme: ColorScheme::default(),
        }
    }
}

pub fn settings_path() -> PathBuf {
    crate::paths::data_root().join("settings.json")
}

pub fn load() -> AppSettings {
    let path = settings_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

pub fn save(settings: &AppSettings) -> Result<()> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(settings)?;
    std::fs::write(&path, text).with_context(|| format!("写设置失败 {}", path.display()))?;
    Ok(())
}
