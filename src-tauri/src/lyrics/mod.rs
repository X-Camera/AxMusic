//! Lyrics aggregation: LRCLIB + 网易云 + QQ音乐.
//! Each source searches independently; the command layer fans out and streams
//! results to the frontend as each source returns (lyrics://results events).
//!
//! Candidate ids are source-prefixed: "lrclib:123" / "netease:456" / "qq:xxx".
//! Default delivery stays sidecar `.lrc`.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::scanner::sidecar_lrc_path;

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

// ── sidecar .lrc ───────────────────────────────────────────────────

/// Write sidecar .lrc next to the audio file. Refuses to overwrite unless `overwrite`.
pub fn write_sidecar(audio_path: &Path, text: &str, overwrite: bool) -> Result<PathBuf> {
    let dest = sidecar_lrc_path(audio_path);
    if dest.exists() && !overwrite {
        return Err(anyhow!("外挂歌词已存在: {}", dest.display()));
    }
    // 原子写：先写同目录临时文件再 rename，写入中途失败不会毁掉已有歌词
    crate::paths::write_atomic(&dest, text.as_bytes())
        .with_context(|| format!("写入失败 {}", dest.display()))?;
    Ok(dest)
}

pub fn read_sidecar(audio_path: &Path) -> Result<String> {
    let p = sidecar_lrc_path(audio_path);
    std::fs::read_to_string(&p).with_context(|| format!("读取失败 {}", p.display()))
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
