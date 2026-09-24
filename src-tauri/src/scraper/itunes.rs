//! iTunes Search API 刮削源 — 官方免费免 key，CJK 覆盖好。
//! 端点：`/search`（album/song）、`/lookup?id=&entity=song`（整张曲目表）。

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use super::{ReleaseDetail, ReleaseTrack, ScrapeCandidate, TrackDetail, SRC_ITUNES};
use crate::lyrics::encode;
use crate::scraper::rate_limit_wait;

const ROOT: &str = "https://itunes.apple.com";

fn client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(crate::scraper::user_agent())
        .timeout(std::time::Duration::from_secs(20))
        .build()?)
}

fn get_json(url: &str) -> Result<serde_json::Value> {
    rate_limit_wait();
    let resp = client()?.get(url).send().context("iTunes 请求失败")?;
    if !resp.status().is_success() {
        return Err(anyhow!("iTunes HTTP {}", resp.status()));
    }
    Ok(resp.json().context("iTunes JSON 解析失败")?)
}

fn year_of(date: &str) -> String {
    // "2003-07-31T00:00:00Z" → "2003"
    date.get(..4).filter(|y| y.bytes().all(|b| b.is_ascii_digit())).unwrap_or("").to_string()
}

// ── 搜索 ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SearchResp<T> {
    results: Option<Vec<T>>,
}

#[derive(Debug, Deserialize)]
struct AlbumItem {
    #[serde(rename = "collectionId")]
    collection_id: Option<i64>,
    #[serde(rename = "collectionName")]
    collection_name: Option<String>,
    #[serde(rename = "artistName")]
    artist_name: Option<String>,
    #[serde(rename = "releaseDate")]
    release_date: Option<String>,
    #[serde(rename = "trackCount")]
    track_count: Option<i64>,
    #[serde(rename = "primaryGenreName")]
    genre: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SongItem {
    #[serde(rename = "trackId")]
    track_id: Option<i64>,
    #[serde(rename = "trackName")]
    track_name: Option<String>,
    #[serde(rename = "artistName")]
    artist_name: Option<String>,
    #[serde(rename = "collectionName")]
    collection_name: Option<String>,
    #[serde(rename = "releaseDate")]
    release_date: Option<String>,
    #[serde(rename = "trackNumber")]
    track_number: Option<i64>,
    #[serde(rename = "discNumber")]
    disc_number: Option<i64>,
}

/// 搜索 storefront 策略：CJK 内容在默认（US）店几乎搜不到，先试 TW（中文曲库全），
/// 空手再回落默认店（欧美/古典更全）。iTunes 官方免费接口，两次请求代价可忽略。
fn search_tw_then_default<T>(entity: &str, q: &str, limit: usize) -> Result<Vec<T>>
where
    T: for<'de> Deserialize<'de>,
{
    for country in ["&country=TW", ""] {
        let url = format!(
            "{ROOT}/search?media=music&entity={entity}&limit={limit}{country}&term={}",
            encode(q)
        );
        let v = get_json(&url)?;
        let data: SearchResp<T> = serde_json::from_value(v).context("iTunes 搜索解析失败")?;
        let results = data.results.unwrap_or_default();
        if !results.is_empty() {
            return Ok(results);
        }
    }
    Ok(Vec::new())
}

pub fn search_albums(album: &str, artist: &str) -> Result<Vec<ScrapeCandidate>> {
    let q = format!("{} {}", album.trim(), artist.trim()).trim().to_string();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let results = search_tw_then_default::<AlbumItem>("album", &q, 15)?;
    Ok(results
        .into_iter()
        .filter_map(|a| {
            let id = a.collection_id?;
            Some(ScrapeCandidate {
                id: id.to_string(),
                kind: "release".into(),
                source: SRC_ITUNES.into(),
                title: a.collection_name.unwrap_or_default(),
                artist: a.artist_name.unwrap_or_default(),
                year: year_of(a.release_date.as_deref().unwrap_or("")),
                track_count: a.track_count.unwrap_or(0),
                country: String::new(),
                disambiguation: a.genre.unwrap_or_default(),
                release_id: id.to_string(),
            })
        })
        .collect())
}

pub fn search_tracks(title: &str, artist: &str) -> Result<Vec<ScrapeCandidate>> {
    let q = format!("{} {}", title.trim(), artist.trim()).trim().to_string();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let results = search_tw_then_default::<SongItem>("song", &q, 15)?;
    Ok(results
        .into_iter()
        .filter_map(|s| {
            let id = s.track_id?;
            Some(ScrapeCandidate {
                id: id.to_string(),
                kind: "recording".into(),
                source: SRC_ITUNES.into(),
                title: s.track_name.unwrap_or_default(),
                artist: s.artist_name.unwrap_or_default(),
                year: year_of(s.release_date.as_deref().unwrap_or("")),
                track_count: 1,
                country: String::new(),
                // 单曲候选的消歧信息放所属专辑
                disambiguation: s.collection_name.unwrap_or_default(),
                release_id: id.to_string(),
            })
        })
        .collect())
}

// ── 详情 ───────────────────────────────────────────────────────────

/// lookup 同样吃 storefront：先试 TW 再回落默认店
fn lookup_tw_then_default(id: &str, entity: Option<&str>) -> Result<Vec<serde_json::Value>> {
    for country in ["&country=TW", ""] {
        let entity_qs = entity.map(|e| format!("&entity={e}")).unwrap_or_default();
        let url = format!("{ROOT}/lookup?id={id}{entity_qs}&limit=200{country}");
        let v = get_json(&url)?;
        let results = v
            .get("results")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        if !results.is_empty() {
            return Ok(results);
        }
    }
    Ok(Vec::new())
}

/// lookup：第一项是 collection 本体，其余为曲目
pub fn fetch_release(collection_id: &str) -> Result<ReleaseDetail> {
    let results = lookup_tw_then_default(collection_id, Some("song"))?;
    if results.is_empty() {
        return Err(anyhow!("iTunes 未找到该专辑"));
    }
    let coll: AlbumItem = serde_json::from_value(results[0].clone()).context("iTunes 专辑解析失败")?;
    let mut tracks = Vec::new();
    for item in results.iter().skip(1) {
        let Ok(s) = serde_json::from_value::<SongItem>(item.clone()) else {
            continue;
        };
        tracks.push(ReleaseTrack {
            position: s.track_number.unwrap_or((tracks.len() + 1) as i64),
            disc: s.disc_number.filter(|d| *d > 0).unwrap_or(1),
            title: s.track_name.unwrap_or_default(),
            artist: s.artist_name.clone().unwrap_or_default(),
            // 非 MB 源无录音 MBID：留空，避免污染 catalog.mbid 的唯一匹配语义
            recording_id: String::new(),
        });
    }
    let artist = coll.artist_name.clone().unwrap_or_default();
    Ok(ReleaseDetail {
        release_id: collection_id.to_string(),
        title: coll.collection_name.unwrap_or_default(),
        album_artist: artist.clone(),
        artist,
        year: year_of(coll.release_date.as_deref().unwrap_or("")),
        release_type: String::new(),
        tracks,
    })
}

pub fn fetch_track(track_id: &str) -> Result<TrackDetail> {
    let results = lookup_tw_then_default(track_id, None)?;
    let first = results
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("iTunes 未找到该单曲"))?;
    let s: SongItem = serde_json::from_value(first).context("iTunes 单曲解析失败")?;
    Ok(TrackDetail {
        id: track_id.to_string(),
        title: s.track_name.unwrap_or_default(),
        artist: s.artist_name.unwrap_or_default(),
        album: s.collection_name.unwrap_or_default(),
        year: year_of(s.release_date.as_deref().unwrap_or("")),
    })
}
