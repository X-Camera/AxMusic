//! ReplayGain 响度归一：读标签增益，播放时按模式补偿，峰值限幅防削波。
//!
//! 标签来源（lofty ItemKey，跨 FLAC/MP3/M4A/Opus）：
//! - 经典：`REPLAYGAIN_TRACK_GAIN` / `REPLAYGAIN_ALBUM_GAIN`（及 PEAK）
//! - Opus R128 兜底：`R128_TRACK_GAIN` / `R128_ALBUM_GAIN`（Q7.8 整数，1/256 dB）

use std::path::Path;

use anyhow::{Context, Result};
use lofty::file::TaggedFileExt;
use lofty::prelude::ItemKey;
use lofty::probe::Probe;
use serde::{Deserialize, Serialize};

/// ReplayGain 2.0 / 常见播放器目标响度（LUFS，约对应经典 89 dB SPL）
const TARGET_LUFS: f32 = -18.0;

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
    /// 无标签时运行时估算（不写文件）
    Estimated,
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

/// 增益落到安全区间（含非法值归 0）；写标签/缓存共用，保证两侧一致
pub fn clamp_gain_db_for_write(db: f32) -> f32 {
    clamp_gain_db(db)
}

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
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct GainTags {
    pub track_gain_db: Option<f32>,
    pub track_peak: Option<f32>,
    pub album_gain_db: Option<f32>,
    pub album_peak: Option<f32>,
}

fn get_string(tag: &lofty::tag::Tag, key: ItemKey) -> Option<String> {
    tag.get_string(key).map(|s| s.to_string())
}

/// 按命中键分流解析：经典 REPLAYGAIN_* 是 "x dB"，R128_* 是 Q7.8 整数。
/// 不能按字符串猜——纯数字串会被 parse_gain_db 误当成 dB。
fn parse_gain_by_keys(tag: &lofty::tag::Tag, classic: ItemKey, r128: ItemKey) -> Option<f32> {
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
            ItemKey::ReplayGainTrackGain,
            ItemKey::R128TrackGain,
        ),
        track_peak: get_string(tag, ItemKey::ReplayGainTrackPeak)
            .as_deref()
            .and_then(parse_peak),
        album_gain_db: parse_gain_by_keys(
            tag,
            ItemKey::ReplayGainAlbumGain,
            ItemKey::R128AlbumGain,
        ),
        album_peak: get_string(tag, ItemKey::ReplayGainAlbumPeak)
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

// ── 运行时估算缓存（无标签时播放中异步分析，不写文件） ──────────────

#[derive(Debug, Clone)]
struct CachedAnalysis {
    gain_db: f32,
    peak: Option<f32>,
    mtime: u64,
}

fn file_mtime(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 供写标签路径取「刚写完」的 mtime，再交给 cache_put 做强制覆盖
pub fn file_mtime_for_cache(path: &Path) -> u64 {
    file_mtime(path)
}

fn analysis_cache() -> std::sync::MutexGuard<'static, std::collections::HashMap<String, CachedAnalysis>>
{
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, CachedAnalysis>>> = OnceLock::new();
    CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

/// 查运行时估算缓存（mtime 变了则失效）
pub fn analysis_cache_get(path: &Path) -> Option<(f32, Option<f32>)> {
    let key = path.to_string_lossy().to_string();
    let mtime = file_mtime(path);
    let cache = analysis_cache();
    let hit = cache.get(&key)?;
    if hit.mtime != mtime {
        return None;
    }
    Some((hit.gain_db, hit.peak))
}

/// 写入估算缓存。`scanned_mtime` = 扫描开始时的文件 mtime；
/// 若文件已被替换（写标签/外部改动）则丢弃本次结果，避免旧测量顶着新 mtime 入库。
/// 写标签路径可传当前 mtime 作为强制覆盖。
pub fn analysis_cache_put(path: &Path, gain_db: f32, peak: Option<f32>, scanned_mtime: u64) {
    let key = path.to_string_lossy().to_string();
    let now = file_mtime(path);
    if now != scanned_mtime {
        return; // 扫描期间文件已变，结果作废
    }
    analysis_cache().insert(
        key,
        CachedAnalysis {
            gain_db,
            peak,
            mtime: now,
        },
    );
}

#[allow(dead_code)] // 外部改文件后主动失效；写标签路径走 cache_put 覆盖
pub fn analysis_cache_invalidate(path: &Path) {
    analysis_cache().remove(&path.to_string_lossy().to_string());
}

fn in_flight_set() -> std::sync::MutexGuard<'static, std::collections::HashSet<String>> {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    static IN_FLIGHT: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    IN_FLIGHT
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

/// 后台分析去重：true = 可以开扫；结束时必须 analysis_end
pub fn analysis_begin(path: &Path) -> bool {
    in_flight_set().insert(path.to_string_lossy().to_string())
}

pub fn analysis_end(path: &Path) {
    in_flight_set().remove(&path.to_string_lossy().to_string());
}

/// 由估算结果构造播放用增益（含峰值限幅）
pub fn info_from_estimate(gain_db: f32, peak: Option<f32>) -> ReplayGainInfo {
    let gain = clamp_gain_db(gain_db);
    let (applied, limited) = peak_limit(gain, peak);
    ReplayGainInfo {
        active: true,
        applied_gain_db: applied,
        source: ReplayGainSource::Estimated,
        track_gain_db: Some(gain),
        album_gain_db: None,
        track_peak: peak,
        album_peak: None,
        peak_limited: limited,
        requested_gain_db: gain,
    }
}

/// 播放解析增益：标签优先，否则查估算缓存。
/// 返回 (info, needs_analyze)：needs_analyze = 应开后台分析（无标签且缓存未命中）
pub fn resolve_for_playback(
    path: &Path,
    mode: ReplayGainMode,
) -> (ReplayGainInfo, bool) {
    if mode == ReplayGainMode::Off {
        return (ReplayGainInfo::default(), false);
    }
    let tags = read_gain_tags(path);
    let from_tags = compute(&tags, mode);
    if from_tags.active {
        return (from_tags, false);
    }
    if let Some((gain, peak)) = analysis_cache_get(path) {
        return (info_from_estimate(gain, peak), false);
    }
    (ReplayGainInfo::default(), true)
}

// ── 扫描估算（解码量响度 → 建议增益）与写回标签 ────────────────────

/// 单曲扫描结果（供右栏展示；写回前用户确认）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayGainScan {
    /// 门限积分响度（LUFS）；失败/静音为 None
    pub measured_lufs: Option<f32>,
    /// 建议写入的曲目增益 (dB) = 目标 − 测量
    pub track_gain_db: Option<f32>,
    /// 样本峰值（线性，0..=1+）
    pub track_peak: Option<f32>,
    /// 文件里已有的曲目增益/峰值
    pub existing_track_gain_db: Option<f32>,
    pub existing_track_peak: Option<f32>,
    /// 已有任一 REPLAYGAIN/R128 曲目字段
    pub has_track_tags: bool,
}

/// 解码整轨 PCM 并用 EBU R128 测积分响度与样本峰值
fn analyze_loudness(path: &Path) -> Result<(Option<f32>, Option<f32>)> {
    use symphonia::core::audio::sample::{i24, u24};
    use symphonia::core::audio::conv::ConvertibleSample;
    use symphonia::core::audio::{Audio, AudioBuffer, GenericAudioBufferRef};
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::errors::Error as SymError;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    let file = std::fs::File::open(path).with_context(|| format!("无法打开 {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut reader = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .context("无法解析音频容器")?;
    let track = reader
        .default_track(TrackType::Audio)
        .context("无可用音频轨")?
        .clone();
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio().cloned())
        .context("音轨缺少编解码参数")?;
    let sample_rate = params.sample_rate.unwrap_or(44_100).max(1);
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .context("不支持的编解码器")?;

    // 解码端统一立体声交错；ebur128 按 2ch 累计
    let mut meter = ebur128::EbuR128::new(
        2,
        sample_rate,
        ebur128::Mode::I | ebur128::Mode::SAMPLE_PEAK,
    )
    .context("初始化响度计失败")?;

    // 与播放引擎相同的交错立体声转换（直接内联，避免依赖 engine 私有函数）
    fn conv_planes<S: ConvertibleSample>(buf: &AudioBuffer<S>, f: impl Fn(S) -> f32) -> Vec<f32> {
        match buf.num_planes() {
            0 => Vec::new(),
            1 => {
                let mono = buf.plane(0).unwrap_or(&[]);
                let mut out = Vec::with_capacity(mono.len() * 2);
                for &s in mono {
                    let v = f(s);
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
                        out.push(f(l[i]));
                        out.push(f(r[i]));
                    }
                    out
                }
                None => Vec::new(),
            },
        }
    }
    fn pack_stereo(buf: &GenericAudioBufferRef<'_>) -> Vec<f32> {
        match buf {
            GenericAudioBufferRef::F32(b) => conv_planes(*b, |s: f32| s),
            GenericAudioBufferRef::U8(b) => {
                conv_planes(*b, |s: u8| (s as f32 - 128.0) / 128.0)
            }
            GenericAudioBufferRef::U16(b) => {
                conv_planes(*b, |s: u16| s as f32 / 32768.0 - 1.0)
            }
            GenericAudioBufferRef::U24(b) => {
                conv_planes(*b, |s: u24| (s.inner() as f32 / 8_388_608.0) - 1.0)
            }
            GenericAudioBufferRef::U32(b) => {
                conv_planes(*b, |s: u32| s as f32 / 2_147_483_648.0 - 1.0)
            }
            GenericAudioBufferRef::S8(b) => {
                conv_planes(*b, |s: i8| s as f32 / 128.0)
            }
            GenericAudioBufferRef::S16(b) => {
                conv_planes(*b, |s: i16| s as f32 / 32768.0)
            }
            GenericAudioBufferRef::S24(b) => {
                conv_planes(*b, |s: i24| s.inner() as f32 / 8_388_608.0)
            }
            GenericAudioBufferRef::S32(b) => {
                conv_planes(*b, |s: i32| s as f32 / 2_147_483_648.0)
            }
            GenericAudioBufferRef::F64(b) => conv_planes(*b, |s: f64| s as f32),
        }
    }

    loop {
        let packet = match reader.next_packet() {
            Ok(Some(p)) => p,
            // 0.6：EOF = Ok(None)；IoError(UnexpectedEof) 留作防御
            Ok(None) => break,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buf) => {
                let samples = pack_stereo(&buf);
                if !samples.is_empty() {
                    meter
                        .add_frames_f32(&samples)
                        .context("响度累计失败")?;
                }
            }
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        }
    }

    let lufs = meter
        .loudness_global()
        .ok()
        .filter(|v| v.is_finite())
        .map(|v| v as f32);
    let peak = (0..2)
        .filter_map(|ch| meter.sample_peak(ch).ok())
        .fold(None::<f64>, |acc, p| {
            Some(acc.map_or(p, |a: f64| a.max(p)))
        })
        .filter(|p| p.is_finite())
        .map(|p| p as f32);
    Ok((lufs, peak))
}

/// 扫描单曲：量响度/峰值，对照目标给出建议增益，并带上文件里已有标签。
/// 成功后写入估算缓存（mtime 以扫描开始时为准），避免播放侧重复解码。
pub fn scan_track(path: &Path) -> Result<ReplayGainScan> {
    let mtime_at_start = file_mtime(path);
    let existing = read_gain_tags(path);
    let (measured_lufs, track_peak) = analyze_loudness(path)?;
    let track_gain_db = measured_lufs.map(|lufs| clamp_gain_db(TARGET_LUFS - lufs));
    if let Some(gain) = track_gain_db {
        analysis_cache_put(path, gain, track_peak, mtime_at_start);
    }
    Ok(ReplayGainScan {
        measured_lufs,
        track_gain_db,
        track_peak,
        existing_track_gain_db: existing.track_gain_db,
        existing_track_peak: existing.track_peak,
        has_track_tags: existing.track_gain_db.is_some() || existing.track_peak.is_some(),
    })
}

/// 把曲目增益/峰值写进标签（临时副本 + rename，与 tagger 同安全契约）
pub fn write_track_gain(path: &Path, gain_db: f32, peak: Option<f32>) -> Result<()> {
    use lofty::config::WriteOptions;
    use lofty::file::{AudioFile, FileType, TaggedFileExt as _};

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
    let ft = FileType::from_ext(ext).context("无法识别音频格式")?;
    let tmp = crate::paths::temp_path_for(path);
    let result = (|| -> Result<()> {
        std::fs::copy(path, &tmp).context("创建临时副本失败")?;
        let mut tagged = Probe::open(&tmp)
            .context("打开失败")?
            .set_file_type(ft)
            .read()
            .context("解析失败")?;
        if tagged.primary_tag().is_none() {
            let primary = tagged.primary_tag_type();
            tagged.insert_tag(lofty::tag::Tag::new(primary));
        }
        let tag = tagged.primary_tag_mut().context("无标签容器")?;
        let gain = clamp_gain_db(gain_db);
        tag.insert_text(
            ItemKey::ReplayGainTrackGain,
            format!("{gain:.2} dB"),
        );
        if let Some(p) = peak.filter(|p| p.is_finite() && *p > 0.0) {
            tag.insert_text(ItemKey::ReplayGainTrackPeak, format!("{p:.6}"));
        }
        tagged
            .save_to_path(&tmp, WriteOptions::default())
            .context("写回失败")?;
        std::fs::rename(&tmp, path).context("替换失败")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
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

    #[test]
    fn cache_put_rejects_stale_mtime() {
        let dir = std::env::temp_dir().join("axmusic-rg-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("stale.wav");
        std::fs::write(&path, b"RIFF").unwrap();
        let mtime = file_mtime(&path);
        analysis_cache_put(&path, -3.0, Some(0.5), mtime);
        assert!(analysis_cache_get(&path).is_some());
        // 伪造「扫描开始于更早 mtime」：文件已被替换后旧结果不得入库
        analysis_cache_invalidate(&path);
        analysis_cache_put(&path, -9.0, None, mtime.saturating_sub(10));
        assert!(analysis_cache_get(&path).is_none());
    }
}
