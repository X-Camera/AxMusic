//! App settings persisted as JSON (`data_root/settings.json`).
//! Library working DB lives next to the music library, not here.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// 循环模式（关 / 列表循环 / 单曲循环；与随机正交）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

/// 旧版单一播放模式（仅迁移读取用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyPlayMode {
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

/// 满窗歌词字体（Windows 常见中文字体）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LyricsFont {
    /// 跟随应用显示字体
    #[default]
    Display,
    /// 微软雅黑
    Yahei,
    /// 等线
    Dengxian,
    /// 楷体
    Kaiti,
    /// 宋体
    Songti,
    /// 黑体
    Heiti,
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
    /// 随机播放（与循环独立）
    #[serde(default)]
    pub shuffle: bool,
    /// 循环：关 / 列表 / 单曲
    #[serde(default)]
    pub repeat: RepeatMode,
    /// 旧字段，仅迁移；序列化时丢弃
    #[serde(default, skip_serializing)]
    pub play_mode: Option<LegacyPlayMode>,
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
    /// 满窗歌词字号缩放（0.75..=1.5）
    #[serde(default = "default_lyrics_font_scale")]
    pub lyrics_font_scale: f32,
    /// 满窗歌词字体
    #[serde(default)]
    pub lyrics_font: LyricsFont,
    /// 满窗歌词行间距（主句 line-height，1.0..=2.0）
    #[serde(default = "default_lyrics_line_height")]
    pub lyrics_line_height: f32,
    /// 主界面歌词字号缩放（0.75..=1.5，与满窗分开）
    #[serde(default = "default_lyrics_font_scale")]
    pub side_lyrics_font_scale: f32,
    /// 主界面歌词字体
    #[serde(default)]
    pub side_lyrics_font: LyricsFont,
    /// 主界面歌词行间距（1.0..=2.0）
    #[serde(default = "default_lyrics_line_height")]
    pub side_lyrics_line_height: f32,
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

fn default_lyrics_font_scale() -> f32 {
    1.0
}

fn default_lyrics_line_height() -> f32 {
    1.25
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            library_root: None,
            volume: default_volume(),
            shuffle: false,
            repeat: RepeatMode::default(),
            play_mode: None,
            restore_volume: true,
            lyrics_save_mode: LyricsSaveMode::default(),
            lyrics_prefer: LyricsPrefer::default(),
            lyrics_sources: LyricsSources::default(),
            lyrics_font_scale: default_lyrics_font_scale(),
            lyrics_font: LyricsFont::default(),
            lyrics_line_height: default_lyrics_line_height(),
            side_lyrics_font_scale: default_lyrics_font_scale(),
            side_lyrics_font: LyricsFont::default(),
            side_lyrics_line_height: default_lyrics_line_height(),
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
    let Ok(text) = std::fs::read_to_string(&path) else {
        return AppSettings::default();
    };
    let mut s: AppSettings = serde_json::from_str(&text).unwrap_or_default();
    // 旧版 play_mode → shuffle + repeat
    if let Some(legacy) = s.play_mode.take() {
        match legacy {
            LegacyPlayMode::Sequential => {}
            LegacyPlayMode::Shuffle => {
                s.shuffle = true;
                s.repeat = RepeatMode::All;
            }
            LegacyPlayMode::RepeatOne => {
                s.repeat = RepeatMode::One;
            }
        }
    }
    s
}

pub fn save(settings: &AppSettings) -> Result<()> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(settings)?;
    crate::paths::write_atomic(&path, text.as_bytes())
        .with_context(|| format!("写设置失败 {}", path.display()))?;
    Ok(())
}
