//! ReplayGain 响度归一：读标签增益，播放时按模式补偿，峰值限幅防削波。
//!
//! 标签来源（lofty ItemKey，跨 FLAC/MP3/M4A/Opus）：
//! - 经典：`REPLAYGAIN_TRACK_GAIN` / `REPLAYGAIN_ALBUM_GAIN`（及 PEAK）
//! - Opus R128 兜底：`R128_TRACK_GAIN` / `R128_ALBUM_GAIN`（Q7.8 整数，1/256 dB）

use std::path::Path;

use lofty::file::TaggedFileExt;
use lofty::prelude::ItemKey;
use lofty::probe::Probe;
use serde::{Deserialize, Serialize};

/// 响度均衡模式（设置页「播放」组）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReplayGainMode {
    /// 不补偿
    Off,
    /// 按曲目（随机/单曲听，响度拉平）
    #[default]
    Track,
    /// 按专辑（保留专辑内动态）
    Album,
}

/// 增益实际取自哪套标签
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReplayGainSource {
    #[default]
    None,
    Track,
    Album,
}

/// 供 UI 展示的本曲响度均衡状态
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ReplayGainInfo {
    /// 是否启用了补偿（模式非关且读到标签）
    pub active: bool,
    /// 实际应用的增益 (dB)；未启用为 0
    pub applied_gain_db: f32,
    /// 增益来源（track / album / none）
    pub source: ReplayGainSource,
    /// 标签原始曲目/专辑增益 (dB)，tooltip 用
    pub track_gain_db: Option<f32>,
    pub album_gain_db: Option<f32>,
    pub track_peak: Option<f32>,
    pub album_peak: Option<f32>,
    /// 峰值限幅把增益收紧过（防削波）
    pub peak_limited: bool,
    /// 限幅前的原始增益 (dB)；仅 peak_limited 时有意义
    pub requested_gain_db: f32,
}

impl ReplayGainInfo {
    /// 线性倍数（与用户音量相乘）
    pub fn linear(&self) -> f32 {
        if !self.active {
            return 1.0;
        }
        db_to_linear(self.applied_gain_db)
    }
}

/// 增益安全区间（dB）：防异常标签把线性倍数算成 inf / 0
const GAIN_DB_MIN: f32 = -60.0;
const GAIN_DB_MAX: f32 = 24.0;

/// dB → 线性振幅倍数（钳制到安全区间）
pub fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db.clamp(GAIN_DB_MIN, GAIN_DB_MAX) * 0.05)
}

/// 增益落到安全区间（含非法值归 0）
fn clamp_gain_db(db: f32) -> f32 {
    if !db.is_finite() {
        return 0.0;
    }
    db.clamp(GAIN_DB_MIN, GAIN_DB_MAX)
}

/// 解析增益字段：`"-6.54 dB"` / `"-6.54dB"` / `"-6.54"`
pub fn parse_gain_db(raw: &str) -> Option<f32> {
    let s = raw.trim();
    let s = s
        .strip_suffix("dB")
        .or_else(|| s.strip_suffix("db"))
        .or_else(|| s.strip_suffix("DB"))
        .map(str::trim)
        .unwrap_or(s);
    s.parse::<f32>().ok().filter(|v| v.is_finite())
}

/// 解析峰值：`"0.988235"`；0/负数视为无效
pub fn parse_peak(raw: &str) -> Option<f32> {
    raw.trim()
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
}

/// R128 标签：Q7.8 整数（1/256 dB），相对 -23 LUFS；直接当作待应用增益
fn parse_r128_gain(raw: &str) -> Option<f32> {
    let n: i32 = raw.trim().parse().ok()?;
    Some(n as f32 / 256.0)
}

/// 峰值限幅：应用增益后若峰值将超过 0 dBFS，把增益收到刚好不削波
fn peak_limit(gain_db: f32, peak: Option<f32>) -> (f32, bool) {
    let Some(peak) = peak.filter(|p| *p > 0.0 && p.is_finite()) else {
        return (gain_db, false);
    };
    let limited_db = -20.0 * peak.log10();
    if gain_db > limited_db {
        (limited_db, true)
    } else {
        (gain_db, false)
    }
}

/// 标签里的四组字段
#[derive(Debug, Clone, Copy, Default)]
pub struct GainTags {
    pub track_gain_db: Option<f32>,
    pub track_peak: Option<f32>,
    pub album_gain_db: Option<f32>,
    pub album_peak: Option<f32>,
}

fn get_string(tag: &lofty::tag::Tag, key: &ItemKey) -> Option<String> {
    tag.get_string(key).map(|s| s.to_string())
}

/// 按命中键分流解析：经典 REPLAYGAIN_* 是 "x dB"，R128_* 是 Q7.8 整数。
/// 不能按字符串猜——纯数字串会被 parse_gain_db 误当成 dB。
fn parse_gain_by_keys(tag: &lofty::tag::Tag, classic: &ItemKey, r128: &ItemKey) -> Option<f32> {
    if let Some(raw) = get_string(tag, classic) {
        return parse_gain_db(&raw).map(clamp_gain_db);
    }
    if let Some(raw) = get_string(tag, r128) {
        return parse_r128_gain(&raw).map(clamp_gain_db);
    }
    None
}

/// 从音频文件读 ReplayGain / R128 标签
pub fn read_gain_tags(path: &Path) -> GainTags {
    let Ok(tagged) = Probe::open(path).and_then(|p| p.read()) else {
        return GainTags::default();
    };
    let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else {
        return GainTags::default();
    };

    GainTags {
        track_gain_db: parse_gain_by_keys(
            tag,
            &ItemKey::ReplayGainTrackGain,
            &ItemKey::Unknown("R128_TRACK_GAIN".into()),
        ),
        track_peak: get_string(tag, &ItemKey::ReplayGainTrackPeak)
            .as_deref()
            .and_then(parse_peak),
        album_gain_db: parse_gain_by_keys(
            tag,
            &ItemKey::ReplayGainAlbumGain,
            &ItemKey::Unknown("R128_ALBUM_GAIN".into()),
        ),
        album_peak: get_string(tag, &ItemKey::ReplayGainAlbumPeak)
            .as_deref()
            .and_then(parse_peak),
    }
}

/// 按模式从标签算出实际应用增益（含峰值限幅）
pub fn compute(tags: &GainTags, mode: ReplayGainMode) -> ReplayGainInfo {
    let mut info = ReplayGainInfo {
        track_gain_db: tags.track_gain_db,
        album_gain_db: tags.album_gain_db,
        track_peak: tags.track_peak,
        album_peak: tags.album_peak,
        ..Default::default()
    };
    if mode == ReplayGainMode::Off {
        return info;
    }

    // 选源：模式优先，缺则回落另一套；两套都没有则不补偿
    let (gain, peak, source) = match mode {
        ReplayGainMode::Track => match (tags.track_gain_db, tags.album_gain_db) {
            (Some(g), _) => (g, tags.track_peak, ReplayGainSource::Track),
            (None, Some(g)) => (g, tags.album_peak, ReplayGainSource::Album),
            (None, None) => (0.0, None, ReplayGainSource::None),
        },
        ReplayGainMode::Album => match (tags.album_gain_db, tags.track_gain_db) {
            (Some(g), _) => (g, tags.album_peak, ReplayGainSource::Album),
            (None, Some(g)) => (g, tags.track_peak, ReplayGainSource::Track),
            (None, None) => (0.0, None, ReplayGainSource::None),
        },
        ReplayGainMode::Off => unreachable!(),
    };
    if source == ReplayGainSource::None {
        return info;
    }

    let gain = clamp_gain_db(gain);
    let (applied, limited) = peak_limit(gain, peak);
    info.active = true;
    info.source = source;
    info.requested_gain_db = gain;
    info.applied_gain_db = applied;
    info.peak_limited = limited;
    info
}

/// 读标签 + 按模式计算（播放打开曲目时调用）
pub fn compute_for_path(path: &Path, mode: ReplayGainMode) -> ReplayGainInfo {
    if mode == ReplayGainMode::Off {
        return ReplayGainInfo::default();
    }
    compute(&read_gain_tags(path), mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_gain_variants() {
        assert_eq!(parse_gain_db("-6.54 dB"), Some(-6.54));
        assert_eq!(parse_gain_db(" +2.3dB "), Some(2.3));
        assert_eq!(parse_gain_db("0"), Some(0.0));
        assert_eq!(parse_gain_db("abc"), None);
    }

    #[test]
    fn parse_r128_q7_8_integer() {
        // RFC 7845：Q7.8，-1234 → ≈ -4.82 dB；不得被当成 -1234 dB
        assert_eq!(parse_r128_gain("-1234"), Some(-1234.0 / 256.0));
        assert_eq!(parse_r128_gain("256"), Some(1.0));
    }

    #[test]
    fn clamp_rejects_absurd_gain() {
        assert_eq!(clamp_gain_db(9999.0), GAIN_DB_MAX);
        assert_eq!(clamp_gain_db(-9999.0), GAIN_DB_MIN);
        assert_eq!(clamp_gain_db(f32::NAN), 0.0);
        assert!((db_to_linear(9999.0) - db_to_linear(GAIN_DB_MAX)).abs() < 1e-6);
        assert!(db_to_linear(9999.0).is_finite());
        assert!(db_to_linear(-9999.0) > 0.0);
    }

    #[test]
    fn parse_peak_rejects_nonpositive() {
        assert_eq!(parse_peak("0.988235"), Some(0.988235));
        assert_eq!(parse_peak("0"), None);
        assert_eq!(parse_peak("-1"), None);
    }

    #[test]
    fn peak_limit_clamps_to_unity() {
        // +12 dB × peak 0.5 → 线性 3.98 × 0.5 > 1，应收在 -20*log10(0.5)≈+6.02
        let (g, limited) = peak_limit(12.0, Some(0.5));
        assert!(limited);
        assert!((g - 6.0206).abs() < 0.01);
        // 负增益不限幅
        let (g, limited) = peak_limit(-3.0, Some(0.9));
        assert!(!limited);
        assert_eq!(g, -3.0);
    }

    #[test]
    fn compute_track_fallback_to_album() {
        let tags = GainTags {
            track_gain_db: None,
            track_peak: None,
            album_gain_db: Some(-2.0),
            album_peak: Some(0.8),
        };
        let info = compute(&tags, ReplayGainMode::Track);
        assert!(info.active);
        assert_eq!(info.source, ReplayGainSource::Album);
        assert!((info.applied_gain_db + 2.0).abs() < 1e-5);
    }

    #[test]
    fn compute_off_is_inactive() {
        let tags = GainTags {
            track_gain_db: Some(3.0),
            ..Default::default()
        };
        let info = compute(&tags, ReplayGainMode::Off);
        assert!(!info.active);
        assert_eq!(info.linear(), 1.0);
    }
}
