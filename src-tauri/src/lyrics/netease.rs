//! 网易云音乐 lyrics source — strongest Chinese catalog.
//! Public web endpoints; no key. Keep requests polite (rate-limited).

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use super::{encode, LyricsCandidate, LyricsContent, SOURCE_NETEASE};
use crate::scraper::rate_limit_wait;

const UA_BROWSER: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

#[derive(Debug, Deserialize)]
struct SearchResp {
    result: Option<SearchResult>,
}

#[derive(Debug, Deserialize)]
struct SearchResult {
    songs: Option<Vec<Song>>,
}

#[derive(Debug, Deserialize)]
struct Song {
    id: Option<i64>,
    name: Option<String>,
    artists: Option<Vec<Artist>>,
    album: Option<Album>,
    /// milliseconds
    duration: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Artist {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Album {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LyricResp {
    lrc: Option<LyricBlock>,
    tlyric: Option<LyricBlock>,
}

#[derive(Debug, Deserialize)]
struct LyricBlock {
    lyric: Option<String>,
}

fn netease_client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(UA_BROWSER)
        .timeout(std::time::Duration::from_secs(20))
        .build()?)
}

pub fn search(title: &str, artist: &str, _album: &str) -> Result<Vec<LyricsCandidate>> {
    rate_limit_wait();
    let query = if artist.trim().is_empty() {
        title.trim().to_string()
    } else {
        format!("{} {}", title.trim(), artist.trim())
    };
    let url = format!(
        "https://music.163.com/api/search/get/web?type=1&limit=15&s={}",
        encode(&query)
    );
    let resp = netease_client()?
        .get(&url)
        .header("Referer", "https://music.163.com")
        .header("Accept", "application/json")
        .send()
        .context("网易云请求失败")?;
    if !resp.status().is_success() {
        return Err(anyhow!("网易云 HTTP {}", resp.status()));
    }
    let data: SearchResp = resp.json().context("网易云 JSON 解析失败")?;
    let songs = data
        .result
        .and_then(|r| r.songs)
        .unwrap_or_default();
    Ok(songs
        .into_iter()
        .filter_map(|s| {
            let id = s.id?;
            let artist = s
                .artists
                .unwrap_or_default()
                .iter()
                .filter_map(|a| a.name.clone())
                .collect::<Vec<_>>()
                .join("/");
            Some(LyricsCandidate {
                id: format!("{}:{}", SOURCE_NETEASE, id),
                source: SOURCE_NETEASE.into(),
                track_name: s.name.unwrap_or_default(),
                artist_name: artist,
                album_name: s.album.and_then(|a| a.name).unwrap_or_default(),
                duration: s.duration.unwrap_or(0.0) / 1000.0,
                // 网易云歌词接口单独拉取；此处乐观标记，fetch 时落空则报无词
                has_synced: true,
                has_plain: true,
            })
        })
        .collect())
}

pub fn fetch(id: &str) -> Result<LyricsContent> {
    rate_limit_wait();
    let url = format!(
        "https://music.163.com/api/song/lyric?id={id}&lv=1&kv=1&tv=-1"
    );
    let resp = netease_client()?
        .get(&url)
        .header("Referer", "https://music.163.com")
        .header("Accept", "application/json")
        .send()
        .context("网易云请求失败")?;
    if !resp.status().is_success() {
        return Err(anyhow!("网易云 HTTP {}", resp.status()));
    }
    let data: LyricResp = resp.json().context("网易云歌词解析失败")?;
    let lrc = data.lrc.and_then(|b| b.lyric).filter(|s| !s.trim().is_empty());
    let tlyric = data
        .tlyric
        .and_then(|b| b.lyric)
        .filter(|s| !s.trim().is_empty());
    // 网易云 lrc 基本都带时间戳，按 synced 处理
    Ok(LyricsContent {
        synced: lrc,
        plain: None,
        translation: tlyric,
    })
}
