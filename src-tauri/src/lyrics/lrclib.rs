//! LRCLIB source — free, no key, strong on English/Japanese; weaker on Chinese.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use super::{encode, http_client, LyricsCandidate, LyricsContent, SOURCE_LRCLIB};
use crate::scraper::rate_limit_wait;

const LRCLIB: &str = "https://lrclib.net/api";

#[derive(Debug, Deserialize)]
struct LrcEntry {
    id: Option<i64>,
    #[serde(rename = "trackName")]
    track_name: Option<String>,
    #[serde(rename = "artistName")]
    artist_name: Option<String>,
    #[serde(rename = "albumName")]
    album_name: Option<String>,
    duration: Option<f64>,
    #[serde(rename = "plainLyrics")]
    plain_lyrics: Option<String>,
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
}

pub fn search(title: &str, artist: &str, album: &str) -> Result<Vec<LyricsCandidate>> {
    rate_limit_wait();
    let mut url = format!("{LRCLIB}/search?track_name={}", encode(title.trim()));
    if !artist.trim().is_empty() {
        url.push_str(&format!("&artist_name={}", encode(artist.trim())));
    }
    if !album.trim().is_empty() {
        url.push_str(&format!("&album_name={}", encode(album.trim())));
    }
    let resp = http_client()
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .context("LRCLIB 请求失败")?;
    if !resp.status().is_success() {
        return Err(anyhow!("LRCLIB HTTP {}", resp.status()));
    }
    let list: Vec<LrcEntry> = resp.json().context("LRCLIB JSON 解析失败")?;
    Ok(list
        .into_iter()
        .filter_map(|e| {
            let has_synced = e
                .synced_lyrics
                .as_ref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            let has_plain = e
                .plain_lyrics
                .as_ref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            Some(LyricsCandidate {
                id: format!("{}:{}", SOURCE_LRCLIB, e.id?),
                source: SOURCE_LRCLIB.into(),
                track_name: e.track_name.unwrap_or_default(),
                artist_name: e.artist_name.unwrap_or_default(),
                album_name: e.album_name.unwrap_or_default(),
                duration: e.duration.unwrap_or(0.0),
                has_synced,
                has_plain,
            })
        })
        .filter(|c| c.has_synced || c.has_plain)
        .take(15)
        .collect())
}

pub fn fetch(id: &str) -> Result<LyricsContent> {
    // id 来自前端候选，拼进 URL 路径前只放行纯数字
    if !crate::net_util::is_numeric_id(id) {
        return Err(anyhow!("候选 id 不合法"));
    }
    rate_limit_wait();
    let url = format!("{LRCLIB}/get/{id}");
    let resp = http_client()
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .context("LRCLIB 请求失败")?;
    if !resp.status().is_success() {
        return Err(anyhow!("LRCLIB HTTP {}", resp.status()));
    }
    let e: LrcEntry = resp.json().context("LRCLIB JSON 解析失败")?;
    Ok(LyricsContent {
        synced: e.synced_lyrics.filter(|s| !s.trim().is_empty()),
        plain: e.plain_lyrics.filter(|s| !s.trim().is_empty()),
        translation: None,
    })
}
