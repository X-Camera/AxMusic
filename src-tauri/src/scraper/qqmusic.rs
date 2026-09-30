//! QQ音乐刮削源 — 中文曲库第二大源。
//! 公开 web 端点无 key：client_search_cp（搜索）/ fcg_v8_album_info_cp（专辑曲目表）
//! / musicu.fcg get_song_detail_yqq（单曲详情）。

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use super::{ReleaseDetail, ReleaseTrack, ScrapeCandidate, TrackAlbum, TrackDetail, SRC_QQ};
use crate::lyrics::encode;
use crate::scraper::rate_limit_wait;

fn client() -> &'static reqwest::blocking::Client {
    crate::scraper::browser_client()
}

fn get_json(url: &str) -> Result<serde_json::Value> {
    rate_limit_wait();
    let resp = client()
        .get(url)
        .header("Referer", "https://y.qq.com")
        .header("Accept", "application/json")
        .send()
        .context("QQ音乐请求失败")?;
    if !resp.status().is_success() {
        return Err(anyhow!("QQ音乐 HTTP {}", resp.status()));
    }
    let v: serde_json::Value = resp.json().context("QQ音乐 JSON 解析失败")?;
    if let Some(code) = v.get("code").and_then(|c| c.as_i64()) {
        if code != 0 {
            return Err(anyhow!("QQ音乐返回错误码 {code}"));
        }
    }
    Ok(v)
}

fn year_of(date: &str) -> String {
    // "2003-07-31" → "2003"
    date.get(..4)
        .filter(|y| y.bytes().all(|b| b.is_ascii_digit()))
        .unwrap_or("")
        .to_string()
}

#[derive(Debug, Deserialize)]
struct Singer {
    name: Option<String>,
}

fn join_singers(list: Option<Vec<Singer>>) -> String {
    list.unwrap_or_default()
        .iter()
        .filter_map(|s| s.name.clone())
        .collect::<Vec<_>>()
        .join("/")
}

// ── 搜索 ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AlbumSearchItem {
    #[serde(rename = "albumMID")]
    album_mid: Option<String>,
    #[serde(rename = "albumName")]
    album_name: Option<String>,
    #[serde(rename = "singerName")]
    singer_name: Option<String>,
    singer_list: Option<Vec<Singer>>,
    #[serde(rename = "publicTime")]
    public_time: Option<String>,
    song_count: Option<i64>,
}

pub fn search_albums(album: &str, artist: &str) -> Result<Vec<ScrapeCandidate>> {
    let q = format!("{} {}", album.trim(), artist.trim()).trim().to_string();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    // t=8 专辑
    let url = format!(
        "https://c.y.qq.com/soso/fcgi-bin/client_search_cp?format=json&n=15&p=1&t=8&w={}",
        encode(&q)
    );
    let v = get_json(&url)?;
    let list = v
        .pointer("/data/album/list")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(list
        .into_iter()
        .filter_map(|item| {
            let a: AlbumSearchItem = serde_json::from_value(item).ok()?;
            let mid = a.album_mid.filter(|m| !m.is_empty())?;
            let artist = {
                let s = join_singers(a.singer_list);
                if !s.is_empty() { s } else { a.singer_name.unwrap_or_default() }
            };
            Some(ScrapeCandidate {
                id: mid.clone(),
                kind: "release".into(),
                source: SRC_QQ.into(),
                title: a.album_name.unwrap_or_default(),
                artist,
                year: year_of(a.public_time.as_deref().unwrap_or("")),
                track_count: a.song_count.unwrap_or(0),
                country: String::new(),
                disambiguation: String::new(),
                release_id: mid,
            })
        })
        .collect())
}

#[derive(Debug, Deserialize)]
struct SongSearchItem {
    songmid: Option<String>,
    songname: Option<String>,
    singer: Option<Vec<Singer>>,
    albumname: Option<String>,
}

pub fn search_tracks(title: &str, artist: &str) -> Result<Vec<ScrapeCandidate>> {
    let q = format!("{} {}", title.trim(), artist.trim()).trim().to_string();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    // t=0 单曲
    let url = format!(
        "https://c.y.qq.com/soso/fcgi-bin/client_search_cp?format=json&n=15&p=1&t=0&w={}",
        encode(&q)
    );
    let v = get_json(&url)?;
    let list = v
        .pointer("/data/song/list")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(list
        .into_iter()
        .filter_map(|item| {
            let s: SongSearchItem = serde_json::from_value(item).ok()?;
            let mid = s.songmid.filter(|m| !m.is_empty())?;
            Some(ScrapeCandidate {
                id: mid.clone(),
                kind: "recording".into(),
                source: SRC_QQ.into(),
                title: s.songname.unwrap_or_default(),
                artist: join_singers(s.singer),
                year: String::new(),
                track_count: 1,
                country: String::new(),
                disambiguation: s.albumname.unwrap_or_default(),
                release_id: mid,
            })
        })
        .collect())
}

// ── 详情 ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AlbumSong {
    songname: Option<String>,
    singer: Option<Vec<Singer>>,
    /// 碟号（多碟时 >1）
    #[serde(rename = "belongCD")]
    belong_cd: Option<i64>,
    /// 碟内轨号（多碟时与 belongCD 配对；缺省用列表序号兜底）
    #[serde(rename = "indexCD")]
    index_cd: Option<i64>,
}

pub fn fetch_release(album_mid: &str) -> Result<ReleaseDetail> {
    if !crate::net_util::is_token_id(album_mid) {
        return Err(anyhow!("专辑 mid 不合法"));
    }
    let url = format!(
        "https://c.y.qq.com/v8/fcg-bin/fcg_v8_album_info_cp.fcg?albummid={album_mid}&format=json"
    );
    let v = get_json(&url)?;
    let data = v.get("data").cloned().ok_or_else(|| anyhow!("QQ音乐未找到该专辑"))?;
    let name = data.get("name").and_then(|x| x.as_str()).unwrap_or_default().to_string();
    if name.is_empty() {
        return Err(anyhow!("QQ音乐未找到该专辑"));
    }
    let singer = data.get("singername").and_then(|x| x.as_str()).unwrap_or_default().to_string();
    let year = year_of(data.get("aDate").and_then(|x| x.as_str()).unwrap_or(""));
    let songs: Vec<AlbumSong> = data
        .get("list")
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
            position: s.index_cd.filter(|n| *n > 0).unwrap_or((i + 1) as i64),
            disc: s.belong_cd.filter(|d| *d > 0).unwrap_or(1),
            title: s.songname.unwrap_or_default(),
            artist: join_singers(s.singer),
            recording_id: String::new(),
        })
        .collect();
    Ok(ReleaseDetail {
        release_id: album_mid.to_string(),
        title: name,
        album_artist: singer.clone(),
        artist: singer,
        year,
        release_type: String::new(),
        tracks,
    })
}

pub fn fetch_track(song_mid: &str) -> Result<TrackDetail> {
    if !crate::net_util::is_token_id(song_mid) {
        return Err(anyhow!("单曲 mid 不合法"));
    }
    let ti = fetch_track_info(song_mid)?;
    let title = ti.get("name").and_then(|x| x.as_str()).unwrap_or_default().to_string();
    if title.is_empty() {
        return Err(anyhow!("QQ音乐未找到该单曲"));
    }
    let artist = join_singers(
        ti.get("singer")
            .and_then(|x| x.as_array())
            .map(|arr| arr.iter().filter_map(|s| serde_json::from_value(s.clone()).ok()).collect()),
    );
    let album = ti
        .get("album")
        .and_then(|x| x.get("name"))
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();
    let year = year_of(ti.get("time_public").and_then(|x| x.as_str()).unwrap_or(""));
    Ok(TrackDetail {
        id: song_mid.to_string(),
        title,
        artist,
        album,
        year,
    })
}

/// 单曲所属专辑（QQ 一首歌通常只挂一个专辑）。
pub fn fetch_track_albums(song_mid: &str) -> Result<Vec<TrackAlbum>> {
    if !crate::net_util::is_token_id(song_mid) {
        return Err(anyhow!("单曲 mid 不合法"));
    }
    let ti = fetch_track_info(song_mid)?;
    let album = ti.get("album").cloned().unwrap_or_default();
    let Some(album_mid) = album
        .get("mid")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
    else {
        return Ok(Vec::new());
    };
    Ok(vec![TrackAlbum {
        source: SRC_QQ.into(),
        release_id: album_mid,
        title: album
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        artist: {
            // 专辑详情才有 singername；单曲详情常无 → 回落曲目歌手
            let from_album = album
                .get("singername")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string();
            if !from_album.is_empty() {
                from_album
            } else {
                join_singers(
                    ti.get("singer")
                        .and_then(|x| x.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|s| serde_json::from_value(s.clone()).ok())
                                .collect()
                        }),
                )
            }
        },
        year: year_of(ti.get("time_public").and_then(|x| x.as_str()).unwrap_or("")),
        track_count: 0,
        country: String::new(),
        release_type: String::new(),
        disambiguation: String::new(),
    }])
}

fn fetch_track_info(song_mid: &str) -> Result<serde_json::Value> {
    // musicu.fcg：data 参数为 JSON（get_song_detail_yqq）
    let data = format!(
        r#"{{"comm":{{"ct":24,"cv":0}},"songinfo":{{"module":"music.pf_song_detail_svr","method":"get_song_detail_yqq","param":{{"song_mid":"{song_mid}"}}}}}}"#
    );
    let url = format!("https://u.y.qq.com/cgi-bin/musicu.fcg?format=json&data={}", encode(&data));
    let v = get_json(&url)?;
    v.pointer("/songinfo/data/track_info")
        .cloned()
        .ok_or_else(|| anyhow!("QQ音乐未找到该单曲"))
}
