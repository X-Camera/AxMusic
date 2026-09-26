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

/// 主界面歌词区背景动效类型（可热切换）
/// 与前端 `SideVizKind` 一致使用 kebab-case（radial-bars / radial-line）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SideVizKind {
    /// 柔和光晕流动（默认，偏素）
    #[default]
    Aurora,
    /// 轻量频谱柱
    Spectrum,
    /// 漂浮粒子
    Particles,
    /// 环形频谱（柱式）
    #[serde(alias = "radial_bars")]
    RadialBars,
    /// 环形频谱（线条）
    #[serde(alias = "radial_line")]
    RadialLine,
    /// 封面流体渐变（AMLL 风）
    Fluid,
    /// 极光丝绸（WebGL 流光，前端自动回退 2D）
    Silk,
}

/// 同一效果的配色风格：素雅（单色白）/ 柔和 / 炫酷（多彩渐变）/ 封面（专辑取色）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SideVizPalette {
    /// 单色白/灰，最素
    Mono,
    /// 柔和单强调色
    #[default]
    Soft,
    /// 多彩渐变，最炫
    Vivid,
    /// 从当前专辑封面提取三色（无封面回退 soft）
    Cover,
}

/// 「封面流体」效果专属参数（0.0..=1.0，前端映射到物理量）
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FluidVizParams {
    /// 模糊度 → CSS blur 6..32px
    #[serde(default = "default_half")]
    pub blur: f32,
    /// 低音呼吸幅度（缩放/亮度脉动）
    #[serde(default = "default_half")]
    pub breathe: f32,
    /// 旋转/漂移速度系数
    #[serde(default = "default_half")]
    pub spin: f32,
}

impl Default for FluidVizParams {
    fn default() -> Self {
        Self {
            blur: 0.5,
            breathe: 0.5,
            spin: 0.5,
        }
    }
}

impl FluidVizParams {
    pub fn clamped(self) -> Self {
        Self {
            blur: self.blur.clamp(0.0, 1.0),
            breathe: self.breathe.clamp(0.0, 1.0),
            spin: self.spin.clamp(0.0, 1.0),
        }
    }
}

/// 「极光丝绸」效果专属参数
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SilkVizParams {
    /// 流速
    #[serde(default = "default_half")]
    pub flow: f32,
    /// 复杂度 → fbm octaves 3..6
    #[serde(default = "default_half")]
    pub complexity: f32,
    /// 亮度
    #[serde(default = "default_half")]
    pub brightness: f32,
}

impl Default for SilkVizParams {
    fn default() -> Self {
        Self {
            flow: 0.5,
            complexity: 0.5,
            brightness: 0.5,
        }
    }
}

impl SilkVizParams {
    pub fn clamped(self) -> Self {
        Self {
            flow: self.flow.clamp(0.0, 1.0),
            complexity: self.complexity.clamp(0.0, 1.0),
            brightness: self.brightness.clamp(0.0, 1.0),
        }
    }
}

/// 「频谱」效果专属参数
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SpectrumVizParams {
    /// 柱数 16..=128
    #[serde(default = "default_spectrum_bars")]
    pub bars: u32,
    /// 发光强度 0=关 .. 1
    #[serde(default = "default_half")]
    pub glow: f32,
    /// 峰值滞留点
    #[serde(default = "default_true")]
    pub peaks: bool,
    /// 上下镜像
    #[serde(default)]
    pub mirror: bool,
}

impl Default for SpectrumVizParams {
    fn default() -> Self {
        Self {
            bars: default_spectrum_bars(),
            glow: 0.5,
            peaks: true,
            mirror: false,
        }
    }
}

impl SpectrumVizParams {
    pub fn clamped(self) -> Self {
        Self {
            bars: self.bars.clamp(16, 128),
            glow: self.glow.clamp(0.0, 1.0),
            peaks: self.peaks,
            mirror: self.mirror,
        }
    }
}

/// 「粒子」效果专属参数
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ParticlesVizParams {
    /// 粒子数 8..=160
    #[serde(default = "default_particles_count")]
    pub count: u32,
    /// 近距连线（plexus）
    #[serde(default = "default_true")]
    pub links: bool,
    /// 连线距离系数 0..1
    #[serde(default = "default_half")]
    pub link_dist: f32,
    /// 粒子大小系数 0..1
    #[serde(default = "default_half")]
    pub size: f32,
}

impl Default for ParticlesVizParams {
    fn default() -> Self {
        Self {
            count: default_particles_count(),
            links: true,
            link_dist: 0.5,
            size: 0.5,
        }
    }
}

impl ParticlesVizParams {
    pub fn clamped(self) -> Self {
        Self {
            count: self.count.clamp(8, 160),
            links: self.links,
            link_dist: self.link_dist.clamp(0.0, 1.0),
            size: self.size.clamp(0.0, 1.0),
        }
    }
}

/// 「环形」效果（环柱/环线共用）专属参数
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RadialVizParams {
    /// 主圆环大小 0..1 → min(w,h)×0.08..0.30
    #[serde(default = "default_half")]
    pub radius: f32,
    /// 外伸长度（环柱）/ 波形幅度（环线）系数
    #[serde(default = "default_half")]
    pub out_len: f32,
    /// 内伸长度系数（仅环柱）
    #[serde(default = "default_half")]
    pub in_len: f32,
    /// 内环粒子发射量 0..=200（仅环线，0=关）
    #[serde(default = "default_radial_emit")]
    pub emit: u32,
    /// 粒子触发灵敏度 0..1（越高越容易触发；仅环线）
    #[serde(default = "default_half")]
    pub sensitivity: f32,
}

impl Default for RadialVizParams {
    fn default() -> Self {
        Self {
            radius: 0.5,
            out_len: 0.5,
            in_len: 0.5,
            emit: default_radial_emit(),
            sensitivity: 0.5,
        }
    }
}

impl RadialVizParams {
    pub fn clamped(self) -> Self {
        Self {
            radius: self.radius.clamp(0.0, 1.0),
            out_len: self.out_len.clamp(0.0, 1.0),
            in_len: self.in_len.clamp(0.0, 1.0),
            emit: self.emit.clamp(0, 200),
            sensitivity: self.sensitivity.clamp(0.0, 1.0),
        }
    }
}

fn default_radial_emit() -> u32 {
    40
}

fn default_half() -> f32 {
    0.5
}

fn default_spectrum_bars() -> u32 {
    64
}

fn default_particles_count() -> u32 {
    56
}

/// 主界面歌词区背景动效设置（默认关闭，保持界面素净）
/// 字段全部 `default`，旧 settings.json 缺字段也不致整份配置解析失败。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideVizSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub kind: SideVizKind,
    #[serde(default)]
    pub palette: SideVizPalette,
    /// 主色 hex（如 #82aaff）
    #[serde(default = "default_viz_color")]
    pub color: String,
    /// 强度 0.0..=1.0
    #[serde(default = "default_viz_intensity")]
    pub intensity: f32,
    /// 图层不透明度 0.0..=1.0
    #[serde(default = "default_viz_opacity")]
    pub opacity: f32,
    /// 动画速度 0.2..=2.0
    #[serde(default = "default_viz_speed")]
    pub speed: f32,
    /// 渲染缩放 0.5 / 0.75 / 1.0（省 GPU）
    #[serde(default = "default_viz_render_scale")]
    pub render_scale: f32,
    /// 帧率上限 30 / 60
    #[serde(default = "default_viz_fps_cap")]
    pub fps_cap: u32,
    /// 「封面流体」专属参数
    #[serde(default)]
    pub fluid: FluidVizParams,
    /// 「极光丝绸」专属参数
    #[serde(default)]
    pub silk: SilkVizParams,
    /// 「频谱」专属参数
    #[serde(default)]
    pub spectrum_ex: SpectrumVizParams,
    /// 「粒子」专属参数
    #[serde(default)]
    pub particles_ex: ParticlesVizParams,
    /// 「环形」（环柱/环线）专属参数
    #[serde(default)]
    pub radial_ex: RadialVizParams,
}

fn default_viz_color() -> String {
    "#82aaff".into()
}

fn default_viz_intensity() -> f32 {
    0.45
}

fn default_viz_opacity() -> f32 {
    0.42
}

fn default_viz_speed() -> f32 {
    1.0
}

fn default_viz_render_scale() -> f32 {
    1.0
}

fn default_viz_fps_cap() -> u32 {
    60
}

/// `#rgb` / `#rrggbb`（大小写均可）
pub fn is_hex_color(s: &str) -> bool {
    let s = s.strip_prefix('#').unwrap_or(s);
    if !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }
    matches!(s.len(), 3 | 6)
}

impl Default for SideVizSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            kind: SideVizKind::default(),
            palette: SideVizPalette::default(),
            color: default_viz_color(),
            intensity: default_viz_intensity(),
            opacity: default_viz_opacity(),
            speed: default_viz_speed(),
            render_scale: default_viz_render_scale(),
            fps_cap: default_viz_fps_cap(),
            fluid: FluidVizParams::default(),
            silk: SilkVizParams::default(),
            spectrum_ex: SpectrumVizParams::default(),
            particles_ex: ParticlesVizParams::default(),
            radial_ex: RadialVizParams::default(),
        }
    }
}

/// side_viz 宽松反序列化：整组解析失败（如降级遇到新枚举值）时回退默认，
/// 不拖垮整份 settings.json（load() 的整份解析是最后防线，这里兜住字段级）。
pub fn side_viz_lenient<'de, D>(deserializer: D) -> Result<SideVizSettings, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(v).unwrap_or_default())
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
    /// 主界面歌词区背景动效（默认关）
    #[serde(default, deserialize_with = "side_viz_lenient")]
    pub side_viz: SideVizSettings,
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
            side_viz: SideVizSettings::default(),
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
    // 解析失败时绝不整份丢弃：至少保留 library_root，避免下次 save 把库指针冲成 null
    let mut s: AppSettings = match serde_json::from_str(&text) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("[settings] 解析失败，已尽量保留关键字段: {err}");
            let mut fallback = AppSettings::default();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(lr) = v.get("library_root").and_then(|x| x.as_str()) {
                    if !lr.is_empty() {
                        fallback.library_root = Some(lr.to_string());
                    }
                }
            }
            fallback
        }
    };
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
