//! Lyrics aggregation: LRCLIB + 网易云 + QQ音乐.
//! Each source searches independently; the command layer fans out and streams
//! results to the frontend as each source returns (lyrics://results events).
//!
//! Candidate ids are source-prefixed: "lrclib:123" / "netease:456" / "qq:xxx".
//! Default delivery stays sidecar `.lrc`.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths;

pub mod lrclib;
pub mod netease;
pub mod qqmusic;

pub const SOURCE_LRCLIB: &str = "lrclib";
pub const SOURCE_NETEASE: &str = "netease";
pub const SOURCE_QQ: &str = "qq";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricsCandidate {
    /// source-prefixed id, e.g. "lrclib:3394906"
    pub id: String,
    pub source: String,
    pub track_name: String,
    pub artist_name: String,
    pub album_name: String,
    /// seconds (0 = unknown)
    pub duration: f64,
    pub has_synced: bool,
    pub has_plain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricsContent {
    /// synced LRC text (with [mm:ss.xx] timestamps), if any
    pub synced: Option<String>,
    /// plain text fallback
    pub plain: Option<String>,
    /// 翻译歌词（LRC），如果源提供
    pub translation: Option<String>,
}

/// Shared blocking client builder (UA carries app identity per source policy).
pub(crate) fn http_client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(crate::scraper::user_agent())
        .timeout(std::time::Duration::from_secs(20))
        .build()?)
}

/// Minimal percent-encoding for query params.
pub(crate) fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Dispatch fetch by source-prefixed candidate id.
pub fn fetch(id: &str) -> Result<LyricsContent> {
    let (source, rest) = id
        .split_once(':')
        .ok_or_else(|| anyhow!("候选 id 缺少来源前缀: {id}"))?;
    match source {
        SOURCE_LRCLIB => lrclib::fetch(rest),
        SOURCE_NETEASE => netease::fetch(rest),
        SOURCE_QQ => qqmusic::fetch(rest),
        other => Err(anyhow!("未知歌词来源: {other}")),
    }
}

/// Pick the text to persist: synced LRC preferred, plain fallback.
pub fn best_text(c: &LyricsContent) -> Option<&str> {
    c.synced.as_deref().or(c.plain.as_deref())
}

// ── lrc 路径解析 ────────────────────────────────────────────────────
//
// 优先级：库内规范化命名 `{artist} - {title}.lrc` → 库内 stem 命名 `{stem}.lrc`
//       → 音频旁同名 .lrc
// 库外文件或未设置库目录时只走音频旁 sidecar。

/// 清洗文件名非法字符（Windows）
fn sanitize_filename(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect();
    cleaned.trim_end_matches(['.', ' ']).to_string()
}

/// 规范化文件名：`{artist} - {title}.lrc`
pub fn normalized_lrc_filename(artist: &str, title: &str) -> String {
    format!("{} - {}.lrc", sanitize_filename(artist), sanitize_filename(title))
}

/// 音频旁 sidecar 路径（同目录同主名、扩展名 .lrc）
pub fn sidecar_lrc_path(audio_path: &Path) -> PathBuf {
    audio_path.with_extension("lrc")
}

/// 库内 stem 命名 lrc 路径：`<库根>/lrc/{音频文件主名}.lrc`
fn library_lrc_stem_path(library_root: &Path, audio_path: &Path) -> PathBuf {
    let mut name = audio_path.file_stem().unwrap_or_default().to_os_string();
    name.push(".lrc");
    paths::library_lrc_dir(library_root).join(name)
}

/// 库内规范化命名 lrc 路径：`<库根>/lrc/{artist} - {title}.lrc`
fn library_lrc_normalized_path(library_root: &Path, artist: &str, title: &str) -> PathBuf {
    paths::library_lrc_dir(library_root).join(normalized_lrc_filename(artist, title))
}

/// 解析写入目标：有库目录 → 库内规范化命名（有 artist/title）或 stem 命名；否则音频旁 sidecar。
/// 库内写入前自动创建 `lrc/` 目录。
fn lrc_write_path(audio_path: &Path, artist: Option<&str>, title: Option<&str>) -> PathBuf {
    if let Some(root) = paths::library_root() {
        let dest = match (artist, title) {
            (Some(a), Some(t)) if !a.is_empty() && !t.is_empty() => {
                library_lrc_normalized_path(&root, a, t)
            }
            _ => library_lrc_stem_path(&root, audio_path),
        };
        if let Some(dir) = dest.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        return dest;
    }
    sidecar_lrc_path(audio_path)
}

/// 解析读取路径：规范化命名 → stem 命名 → 音频旁 sidecar；都不存在返回 None。
fn lrc_read_path(audio_path: &Path, artist: Option<&str>, title: Option<&str>) -> Option<PathBuf> {
    if let Some(root) = paths::library_root() {
        // 规范化命名优先（整理后）
        if let (Some(a), Some(t)) = (artist, title) {
            if !a.is_empty() && !t.is_empty() {
                let p = library_lrc_normalized_path(&root, a, t);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        // stem 命名
        let p = library_lrc_stem_path(&root, audio_path);
        if p.is_file() {
            return Some(p);
        }
    }
    // 音频旁 sidecar
    let sidecar = sidecar_lrc_path(audio_path);
    if sidecar.is_file() {
        return Some(sidecar);
    }
    None
}

/// 检查是否存在外挂歌词（任一位置即可）。
pub fn lrc_exists(audio_path: &Path, artist: Option<&str>, title: Option<&str>) -> bool {
    lrc_read_path(audio_path, artist, title).is_some()
}

/// Write sidecar .lrc（库内规范化/stem 命名优先）。Refuses to overwrite unless `overwrite`.
pub fn write_sidecar(
    audio_path: &Path,
    text: &str,
    overwrite: bool,
    artist: Option<&str>,
    title: Option<&str>,
) -> Result<PathBuf> {
    let dest = lrc_write_path(audio_path, artist, title);
    if dest.exists() && !overwrite {
        return Err(anyhow!("外挂歌词已存在: {}", dest.display()));
    }
    // 原子写：先写同目录临时文件再 rename，写入中途失败不会毁掉已有歌词
    paths::write_atomic(&dest, text.as_bytes())
        .with_context(|| format!("写入失败 {}", dest.display()))?;
    Ok(dest)
}

/// Read sidecar .lrc（规范化命名 → stem → 音频旁）。
pub fn read_sidecar(audio_path: &Path, artist: Option<&str>, title: Option<&str>) -> Result<String> {
    let p = lrc_read_path(audio_path, artist, title)
        .ok_or_else(|| anyhow!("外挂歌词不存在: {}", sidecar_lrc_path(audio_path).display()))?;
    // GBK/ANSI 的 .lrc 在中文 Windows 曲库里很常见：UTF-8 失败回退 GB18030，而不是报错
    paths::read_text_lossy(&p).with_context(|| format!("读取失败 {}", p.display()))
}

// ── embedded lyrics (via lofty) ────────────────────────────────────

/// Read embedded lyrics text (LYRICS / USLT / ©lyr) if present.
pub fn read_embedded(audio_path: &Path) -> Result<Option<String>> {
    use lofty::file::TaggedFileExt;
    use lofty::probe::Probe;
    use lofty::tag::ItemKey;

    let tagged = Probe::open(audio_path)?.read()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    let Some(tag) = tag else { return Ok(None) };
    Ok(tag
        .get_string(&ItemKey::Lyrics)
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty()))
}
