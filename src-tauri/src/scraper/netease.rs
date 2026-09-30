//! 网易云音乐刮削源 — 中文曲库最全（简繁一体）。
//! 公开 web 端点无 key；有反爬限速（-462），所有请求过 rate_limit_wait，失败给中文提示。

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use super::{ReleaseDetail, ReleaseTrack, ScrapeCandidate, TrackAlbum, TrackDetail, SRC_NETEASE};
use crate::lyrics::encode;
use crate::scraper::rate_limit_wait;

fn client() -> &'static reqwest::blocking::Client {
    crate::scraper::browser_client()
}

fn get_json(url: &str) -> Result<serde_json::Value> {
    rate_limit_wait();
    let resp = client()
        .get(url)
        .header("Referer", "https://music.163.com")
        .header("Accept", "application/json")
        .send()
        .context("网易云请求失败")?;
    if !resp.status().is_success() {
        return Err(anyhow!("网易云 HTTP {}", resp.status()));
    }
    let v: serde_json::Value = resp.json().context("网易云 JSON 解析失败")?;
    // 反爬拦截：-462 需验证；-460 等一并显式报错
    if let Some(code) = v.get("code").and_then(|c| c.as_i64()) {
        if code != 200 {
            return Err(anyhow!("网易云返回错误码 {code}（可能被限速，稍后再试）"));
        }
    }
    Ok(v)
}

/// 毫秒 epoch → 年份（Howard Hinnant civil_from_days，只取年）
fn year_of_epoch_ms(ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    let days = ms.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    year.to_string()
}

#[derive(Debug, Deserialize)]
struct IdName {
    name: Option<String>,
}

fn join_artists(list: Option<Vec<IdName>>) -> String {
    list.unwrap_or_default()
        .iter()
        .filter_map(|a| a.name.clone())
        .collect::<Vec<_>>()
        .join("/")
}

// ── 搜索 ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AlbumSearchItem {
    id: Option<i64>,
    name: Option<String>,
    artists: Option<Vec<IdName>>,
    artist: Option<IdName>,
    /// 曲目数
    size: Option<i64>,
    /// 毫秒 epoch
    #[serde(rename = "publishTime")]
    publish_time: Option<i64>,
    company: Option<String>,
}

pub fn search_albums(album: &str, artist: &str) -> Result<Vec<ScrapeCandidate>> {
    let q = format!("{} {}", album.trim(), artist.trim()).trim().to_string();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    // type=10 专辑
    let url = format!("https://music.163.com/api/search/get/web?type=10&limit=15&s={}", encode(&q));
    let v = get_json(&url)?;
    let albums = v
        .pointer("/result/albums")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(albums
        .into_iter()
        .filter_map(|item| {
            let a: AlbumSearchItem = serde_json::from_value(item).ok()?;
            let id = a.id?;
            let artist = {
                let s = join_artists(a.artists);
                if !s.is_empty() { s } else { a.artist.and_then(|x| x.name).unwrap_or_default() }
            };
            Some(ScrapeCandidate {
                id: id.to_string(),
                kind: "release".into(),
                source: SRC_NETEASE.into(),
                title: a.name.unwrap_or_default(),
                artist,
                year: year_of_epoch_ms(a.publish_time.unwrap_or(0)),
                track_count: a.size.unwrap_or(0),
                country: String::new(),
                disambiguation: a.company.unwrap_or_default(),
                release_id: id.to_string(),
            })
        })
        .collect())
}

#[derive(Debug, Deserialize)]
struct SongSearchItem {
    id: Option<i64>,
    name: Option<String>,
    artists: Option<Vec<IdName>>,
    album: Option<IdName>,
}

pub fn search_tracks(title: &str, artist: &str) -> Result<Vec<ScrapeCandidate>> {
    let q = format!("{} {}", title.trim(), artist.trim()).trim().to_string();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    // type=1 单曲
    let url = format!("https://music.163.com/api/search/get/web?type=1&limit=15&s={}", encode(&q));
    let v = get_json(&url)?;
    let songs = v
        .pointer("/result/songs")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(songs
        .into_iter()
        .filter_map(|item| {
            let s: SongSearchItem = serde_json::from_value(item).ok()?;
            let id = s.id?;
            Some(ScrapeCandidate {
                id: id.to_string(),
                kind: "recording".into(),
                source: SRC_NETEASE.into(),
                title: s.name.unwrap_or_default(),
                artist: join_artists(s.artists),
                year: String::new(),
                track_count: 1,
                country: String::new(),
                disambiguation: s.album.and_then(|a| a.name).unwrap_or_default(),
                release_id: id.to_string(),
            })
        })
        .collect())
}

// ── 详情 ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AlbumSong {
    name: Option<String>,
    artists: Option<Vec<IdName>>,
    /// 轨号（旧 API 字段；缺省用序号兜底）
    no: Option<i64>,
    /// 碟号（字符串如 "01"；多碟才有意义）
    cd: Option<String>,
}

pub fn fetch_release(album_id: &str) -> Result<ReleaseDetail> {
    if !crate::net_util::is_numeric_id(album_id) {
        return Err(anyhow!("专辑 id 不合法"));
    }
    let url = format!("https://music.163.com/api/album/{album_id}");
    let v = get_json(&url)?;
    let album = v.get("album").cloned().ok_or_else(|| anyhow!("网易云未找到该专辑"))?;
    let title = album.get("name").and_then(|x| x.as_str()).unwrap_or_default().to_string();
    let artist = album
        .get("artist")
        .and_then(|x| x.get("name"))
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();
    let year = year_of_epoch_ms(
        album.get("publishTime").and_then(|x| x.as_i64()).unwrap_or(0),
    );
    let songs: Vec<AlbumSong> = album
        .get("songs")
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| serde_json::from_value(s.clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    let tracks = songs
        .into_iter()
        .enumerate()
        .map(|(i, s)| ReleaseTrack {
            position: s.no.filter(|n| *n > 0).unwrap_or((i + 1) as i64),
            disc: s
                .cd
                .as_deref()
                .and_then(|c| c.trim().parse::<i64>().ok())
                .filter(|d| *d > 0)
                .unwrap_or(1),
            title: s.name.unwrap_or_default(),
            artist: join_artists(s.artists),
            recording_id: String::new(),
        })
        .collect();
    Ok(ReleaseDetail {
        release_id: album_id.to_string(),
        title,
        album_artist: artist.clone(),
        artist,
        year,
        release_type: String::new(),
        tracks,
    })
}

pub fn fetch_track(song_id: &str) -> Result<TrackDetail> {
    if !crate::net_util::is_numeric_id(song_id) {
        return Err(anyhow!("单曲 id 不合法"));
    }
    let url = format!("https://music.163.com/api/song/detail?ids=%5B{song_id}%5D");
    let v = get_json(&url)?;
    let first = v
        .get("songs")
        .and_then(|x| x.as_array())
        .and_then(|a| a.first())
        .cloned()
        .ok_or_else(|| anyhow!("网易云未找到该单曲"))?;
    let title = first.get("name").and_then(|x| x.as_str()).unwrap_or_default().to_string();
    let artist = join_artists(
        first
            .get("artists")
            .and_then(|x| x.as_array())
            .map(|arr| arr.iter().filter_map(|a| serde_json::from_value(a.clone()).ok()).collect()),
    );
    let album = first
        .get("album")
        .and_then(|x| x.get("name"))
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();
    let year = first
        .get("album")
        .and_then(|x| x.get("publishTime"))
        .and_then(|x| x.as_i64())
        .map(year_of_epoch_ms)
        .unwrap_or_default();
    Ok(TrackDetail {
        id: song_id.to_string(),
        title,
        artist,
        album,
        year,
    })
}

/// 单曲所属专辑（网易云一首歌通常只挂一个专辑）。
pub fn fetch_track_albums(song_id: &str) -> Result<Vec<TrackAlbum>> {
    if !crate::net_util::is_numeric_id(song_id) {
        return Err(anyhow!("单曲 id 不合法"));
    }
    let url = format!("https://music.163.com/api/song/detail?ids=%5B{song_id}%5D");
    let v = get_json(&url)?;
    let first = v
        .get("songs")
        .and_then(|x| x.as_array())
        .and_then(|a| a.first())
        .cloned()
        .ok_or_else(|| anyhow!("网易云未找到该单曲"))?;
    let album = first.get("album").cloned().unwrap_or_default();
    let Some(album_id) = album.get("id").and_then(|x| x.as_i64()) else {
        return Ok(Vec::new());
    };
    let title = album
        .get("name")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();
    let year = album
        .get("publishTime")
        .and_then(|x| x.as_i64())
        .map(year_of_epoch_ms)
        .unwrap_or_default();
    let artist = first
        .get("album")
        .and_then(|x| x.get("artist"))
        .and_then(|x| x.get("name"))
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();
    Ok(vec![TrackAlbum {
        source: SRC_NETEASE.into(),
        release_id: album_id.to_string(),
        title,
        artist,
        year,
        track_count: 0,
        country: String::new(),
        release_type: String::new(),
        disambiguation: String::new(),
    }])
}
