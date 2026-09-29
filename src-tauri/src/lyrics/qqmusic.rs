//! QQ音乐 lyrics source — second large Chinese catalog.
//! Public web endpoints; lyric payload is base64-encoded.

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::Deserialize;

use super::{encode, LyricsCandidate, LyricsContent, SOURCE_QQ};
use crate::scraper::rate_limit_wait;

#[derive(Debug, Deserialize)]
struct SearchResp {
    data: Option<SearchData>,
}

#[derive(Debug, Deserialize)]
struct SearchData {
    song: Option<SongList>,
}

#[derive(Debug, Deserialize)]
struct SongList {
    list: Option<Vec<SongItem>>,
}

#[derive(Debug, Deserialize)]
struct SongItem {
    /// alphanumeric mid (used by lyric endpoint)
    songmid: Option<String>,
    songname: Option<String>,
    singer: Option<Vec<Singer>>,
    albumname: Option<String>,
    /// seconds
    interval: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Singer {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LyricResp {
    lyric: Option<String>,
    trans: Option<String>,
}

fn qq_client() -> &'static reqwest::blocking::Client {
    crate::scraper::browser_client()
}

pub fn search(title: &str, artist: &str, _album: &str) -> Result<Vec<LyricsCandidate>> {
    rate_limit_wait();
    let query = if artist.trim().is_empty() {
        title.trim().to_string()
    } else {
        format!("{} {}", title.trim(), artist.trim())
    };
    let url = format!(
        "https://c.y.qq.com/soso/fcgi-bin/client_search_cp?format=json&n=15&p=1&t=0&w={}",
        encode(&query)
    );
    let resp = qq_client()
        .get(&url)
        .header("Referer", "https://y.qq.com")
        .header("Accept", "application/json")
        .send()
        .context("QQ音乐请求失败")?;
    if !resp.status().is_success() {
        return Err(anyhow!("QQ音乐 HTTP {}", resp.status()));
    }
    let data: SearchResp = resp.json().context("QQ音乐 JSON 解析失败")?;
    let list = data
        .data
        .and_then(|d| d.song)
        .and_then(|s| s.list)
        .unwrap_or_default();
    Ok(list
        .into_iter()
        .filter_map(|s| {
            let mid = s.songmid?;
            let artist = s
                .singer
                .unwrap_or_default()
                .iter()
                .filter_map(|x| x.name.clone())
                .collect::<Vec<_>>()
                .join("/");
            Some(LyricsCandidate {
                id: format!("{}:{}", SOURCE_QQ, mid),
                source: SOURCE_QQ.into(),
                track_name: s.songname.unwrap_or_default(),
                artist_name: artist,
                album_name: s.albumname.unwrap_or_default(),
                duration: s.interval.unwrap_or(0.0),
                has_synced: true,
                has_plain: true,
            })
        })
        .collect())
}

pub fn fetch(mid: &str) -> Result<LyricsContent> {
    if !crate::net_util::is_token_id(mid) {
        return Err(anyhow!("候选 id 不合法"));
    }
    rate_limit_wait();
    let url = format!(
        "https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg?songmid={mid}&format=json&nobase64=1&g_tk=5381"
    );
    let resp = qq_client()
        .get(&url)
        .header("Referer", "https://y.qq.com/n/ryqq/player")
        .header("Origin", "https://y.qq.com")
        .header("Accept", "application/json")
        .send()
        .context("QQ音乐请求失败")?;
    if !resp.status().is_success() {
        return Err(anyhow!("QQ音乐 HTTP {}", resp.status()));
    }
    let data: LyricResp = resp.json().context("QQ音乐歌词解析失败")?;

    // nobase64=1 时已是明文；个别服务端配置仍回 base64，做兜底解码
    fn maybe_decode(s: Option<String>) -> Option<String> {
        let s = s.filter(|x| !x.trim().is_empty())?;
        if s.contains('\n') || s.contains("[ti:") || s.contains("[00:") {
            return Some(s);
        }
        B64.decode(s.trim())
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .or(Some(s))
    }

    Ok(LyricsContent {
        synced: maybe_decode(data.lyric),
        plain: None,
        translation: maybe_decode(data.trans),
    })
}
