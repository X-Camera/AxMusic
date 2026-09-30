//! MusicBrainz WS/2 JSON client (search + release tracklist).

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::Value;

use super::{rate_limit_wait, ScrapeCandidate, SRC_MB};

/// 通用结构在 scraper/mod.rs 定义，此处 re-export 保持旧路径可用
pub use super::{ReleaseDetail, ReleaseTrack};

const MB_ROOT: &str = "https://musicbrainz.org/ws/2";

#[derive(Debug, Deserialize)]
struct MbNameCredit {
    name: Option<String>,
    artist: Option<MbArtist>,
}

#[derive(Debug, Deserialize)]
struct MbArtist {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // 完整映射 API 响应，部分字段暂未读取
struct MbReleaseGroup {
    id: Option<String>,
    title: Option<String>,
    #[serde(rename = "first-release-date")]
    first_release_date: Option<String>,
    #[serde(rename = "primary-type")]
    primary_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // 完整映射 API 响应，部分字段暂未读取
struct MbRelease {
    id: Option<String>,
    title: Option<String>,
    date: Option<String>,
    country: Option<String>,
    status: Option<String>,
    disambiguation: Option<String>,
    #[serde(rename = "track-count")]
    track_count: Option<i64>,
    #[serde(rename = "artist-credit")]
    artist_credit: Option<Vec<MbNameCredit>>,
    #[serde(rename = "release-group")]
    release_group: Option<MbReleaseGroup>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // 完整映射 API 响应，部分字段暂未读取
struct MbRecording {
    id: Option<String>,
    title: Option<String>,
    length: Option<i64>,
    #[serde(rename = "artist-credit")]
    artist_credit: Option<Vec<MbNameCredit>>,
}

fn http_client() -> &'static reqwest::blocking::Client {
    crate::scraper::http_client()
}

fn credit_name(credit: &Option<Vec<MbNameCredit>>) -> String {
    credit
        .as_ref()
        .map(|list| {
            list.iter()
                .map(|c| {
                    c.name
                        .clone()
                        .or_else(|| c.artist.as_ref().and_then(|a| a.name.clone()))
                        .unwrap_or_default()
                })
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn year_of(date: &Option<String>) -> Option<String> {
    date.as_deref()
        .map(|d| d.chars().take(4).collect::<String>())
        .filter(|y| y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()))
}

fn year_of_any(a: &Option<String>, b: &Option<String>) -> String {
    year_of(a)
        .or_else(|| year_of(b))
        .unwrap_or_default()
}

fn get_json(url: &str) -> Result<Value> {
    rate_limit_wait();
    let client = http_client();
    let resp = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .context("MusicBrainz 请求失败")?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("MusicBrainz HTTP {status}"));
    }
    Ok(resp.json().context("MusicBrainz JSON 解析失败")?)
}

/// Lucene query → URL
fn mb_get(path_and_query: &str) -> Result<Value> {
    get_json(&format!("{MB_ROOT}/{path_and_query}"))
}

/// Search releases (album scrape). Returns display candidates.
pub fn search_releases(album: &str, artist: &str) -> Result<Vec<ScrapeCandidate>> {
    let album_esc = crate::net_util::escape_lucene(album.trim());
    let mut q = format!("release:\"{album_esc}\"");
    let artist = artist.trim();
    if !artist.is_empty() {
        let artist_esc = crate::net_util::escape_lucene(artist);
        q.push_str(&format!(" AND artist:\"{artist_esc}\""));
    }
    let url = format!(
        "release/?query={}&fmt=json&limit=15",
        urlencoding_lite(&q)
    );
    let v = mb_get(&url)?;
    let mut out = Vec::new();
    if let Some(list) = v.get("releases").and_then(|x| x.as_array()) {
        for item in list {
            let rel: MbRelease = serde_json::from_value(item.clone()).unwrap_or(MbRelease {
                id: None,
                title: None,
                date: None,
                country: None,
                status: None,
                disambiguation: None,
                track_count: None,
                artist_credit: None,
                release_group: None,
            });
            let Some(id) = rel.id.clone() else { continue };
            out.push(ScrapeCandidate {
                id: rel
                    .release_group
                    .as_ref()
                    .and_then(|g| g.id.clone())
                    .unwrap_or_else(|| id.clone()),
                kind: "release".into(),
                source: SRC_MB.into(),
                title: rel.title.unwrap_or_default(),
                artist: credit_name(&rel.artist_credit),
                year: year_of_any(
                    &rel.date,
                    &rel.release_group.as_ref().and_then(|g| g.first_release_date.clone()),
                ),
                track_count: rel.track_count.unwrap_or(0),
                country: rel.country.unwrap_or_default(),
                disambiguation: rel.disambiguation.unwrap_or_default(),
                release_id: id,
            });
        }
    }
    Ok(out)
}

/// Search recordings (single-track scrape).
pub fn search_recordings(title: &str, artist: &str) -> Result<Vec<ScrapeCandidate>> {
    let title_esc = crate::net_util::escape_lucene(title.trim());
    let mut q = format!("recording:\"{title_esc}\"");
    let artist = artist.trim();
    if !artist.is_empty() {
        let artist_esc = crate::net_util::escape_lucene(artist);
        q.push_str(&format!(" AND artist:\"{artist_esc}\""));
    }
    let url = format!(
        "recording/?query={}&fmt=json&limit=15",
        urlencoding_lite(&q)
    );
    let v = mb_get(&url)?;
    let mut out = Vec::new();
    if let Some(list) = v.get("recordings").and_then(|x| x.as_array()) {
        for item in list {
            let rec: MbRecording = match serde_json::from_value(item.clone()) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let Some(id) = rec.id.clone() else { continue };
            out.push(ScrapeCandidate {
                id: id.clone(),
                kind: "recording".into(),
                source: SRC_MB.into(),
                title: rec.title.unwrap_or_default(),
                artist: credit_name(&rec.artist_credit),
                year: String::new(),
                track_count: 1,
                country: String::new(),
                disambiguation: String::new(),
                release_id: id,
            });
        }
    }
    Ok(out)
}

/// Recording summary for single-track scrape.
pub struct RecordingDetail {
    pub id: String,
    pub title: String,
    pub artist: String,
    /// 首个发行的专辑名（挑专辑前的兜底展示）
    pub first_release_title: String,
    pub first_release_year: String,
}

/// GET /recording/{id}?inc=artist-credits+releases+release-groups
pub fn fetch_recording(recording_id: &str) -> Result<RecordingDetail> {
    if !crate::net_util::is_mbid(recording_id) {
        return Err(anyhow!("recording id 不合法"));
    }
    let v = mb_get(&format!(
        "recording/{recording_id}?inc=artist-credits+releases+release-groups&fmt=json"
    ))?;
    let albums = parse_recording_releases(&v);
    let (first_release_title, first_release_year) = albums
        .first()
        .map(|a| (a.title.clone(), a.year.clone()))
        .unwrap_or_default();
    let rec: MbRecording = serde_json::from_value(v).context("recording 结构解析失败")?;
    Ok(RecordingDetail {
        id: recording_id.to_string(),
        title: rec.title.unwrap_or_default(),
        artist: credit_name(&rec.artist_credit),
        first_release_title,
        first_release_year,
    })
}

/// 录音所属发行列表（挑专辑用）：原专优先，再按年份。
pub fn fetch_recording_albums(recording_id: &str) -> Result<Vec<super::TrackAlbum>> {
    if !crate::net_util::is_mbid(recording_id) {
        return Err(anyhow!("recording id 不合法"));
    }
    let v = mb_get(&format!(
        "recording/{recording_id}?inc=releases+release-groups+artist-credits&fmt=json"
    ))?;
    Ok(parse_recording_releases(&v))
}

/// 解析 recording lookup 的 `releases[]`（含 release-group primary-type）。
fn parse_recording_releases(v: &Value) -> Vec<super::TrackAlbum> {
    use super::TrackAlbum;

    let mut out: Vec<TrackAlbum> = Vec::new();
    let Some(list) = v.get("releases").and_then(|x| x.as_array()) else {
        return out;
    };
    for item in list {
        let Some(id) = item.get("id").and_then(|x| x.as_str()) else {
            continue;
        };
        let title = item
            .get("title")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string();
        let year = year_of(&item.get("date").and_then(|x| x.as_str()).map(|s| s.to_string()))
            .or_else(|| {
                year_of(
                    &item
                        .pointer("/release-group/first-release-date")
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string()),
                )
            })
            .unwrap_or_default();
        let country = item
            .get("country")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string();
        let track_count = item.get("track-count").and_then(|x| x.as_i64()).unwrap_or(0);
        let release_type = {
            let primary = item
                .pointer("/release-group/primary-type")
                .and_then(|x| x.as_str())
                .unwrap_or_default();
            // Live/Compilation/Remix 等在 secondary-types；合并展示便于挑专辑
            let mut secondary: Vec<String> = item
                .pointer("/release-group/secondary-types")
                .and_then(|x| x.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            if secondary.is_empty() {
                primary.to_string()
            } else {
                secondary.sort();
                format!("{} ({})", primary, secondary.join("+"))
            }
        };
        let disambiguation = item
            .get("disambiguation")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string();
        let artist = {
            let ac: Option<Vec<MbNameCredit>> = item
                .get("artist-credit")
                .and_then(|ac| serde_json::from_value(ac.clone()).ok());
            credit_name(&ac)
        };
        out.push(TrackAlbum {
            source: SRC_MB.into(),
            release_id: id.to_string(),
            title,
            artist,
            year,
            track_count,
            country,
            release_type,
            disambiguation,
        });
    }
    // 原专（Album）优先，其次 EP/Single；空年份排最后，再按年份升序
    out.sort_by(|a, b| {
        type_rank(&a.release_type)
            .cmp(&type_rank(&b.release_type))
            .then_with(|| match (a.year.is_empty(), b.year.is_empty()) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => a.year.cmp(&b.year),
            })
            .then_with(|| a.title.cmp(&b.title))
    });
    out
}

fn type_rank(t: &str) -> u8 {
    let k = t.to_ascii_lowercase();
    // secondary 拼在括号里（如 "Album (Live)"），含即降级
    let has_secondary = k.contains("live")
        || k.contains("compilation")
        || k.contains("remix")
        || k.contains("soundtrack")
        || k.contains("dj-mix")
        || k.contains("mixtape");
    if has_secondary {
        return 4;
    }
    if k.starts_with("album") {
        0
    } else if k.starts_with("ep") {
        1
    } else if k.starts_with("single") {
        2
    } else {
        3
    }
}

/// Fetch release + recordings + artist-credit.
pub fn fetch_release(release_id: &str) -> Result<ReleaseDetail> {
    if !crate::net_util::is_mbid(release_id) {
        return Err(anyhow!("release id 不合法"));
    }
    let url = format!(
        "release/{release_id}?inc=recordings+artist-credits&fmt=json"
    );
    let v = mb_get(&url)?;
    let rel: MbRelease = serde_json::from_value(v.clone())
        .map_err(|e| anyhow!("release 结构解析失败: {e}"))?;

    let album_artist = credit_name(&rel.artist_credit);
    let artist_parent = album_artist.clone();
    let title = rel.title.unwrap_or_default();
    let year = year_of_any(
        &rel.date,
        &rel.release_group.as_ref().and_then(|g| g.first_release_date.clone()),
    );

    let mut tracks = Vec::new();
    if let Some(mediums) = v.get("media").and_then(|m| m.as_array()) {
        for medium in mediums {
            // 碟号：多碟发行同一 position 每碟各出现一次，配对/去重都靠它区分
            let disc = medium
                .get("position")
                .and_then(|p| p.as_i64())
                .unwrap_or(1);
            if let Some(list) = medium.get("tracks").and_then(|t| t.as_array()) {
                for t in list {
                    let position = t
                        .get("position")
                        .and_then(|p| p.as_i64())
                        .or_else(|| t.get("number").and_then(|n| n.as_str()).and_then(|s| s.parse().ok()))
                        .unwrap_or(0);
                    let title = t
                        .get("title")
                        .and_then(|x| x.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let artist = {
                        let ac: Option<Vec<MbNameCredit>> = t
                            .get("artist-credit")
                            .and_then(|ac| serde_json::from_value(ac.clone()).ok());
                        credit_name(&ac)
                    };
                    let artist = if artist.is_empty() {
                        artist_parent.clone()
                    } else {
                        artist
                    };
                    let recording_id = t
                        .pointer("/recording/id")
                        .and_then(|x| x.as_str())
                        .unwrap_or_default()
                        .to_string();
                    tracks.push(ReleaseTrack {
                        position,
                        disc,
                        title,
                        artist,
                        recording_id,
                    });
                }
            }
        }
    }

    // Prefer release-group artist as album artist when present
    let album_artist = rel
        .release_group
        .as_ref()
        .map(|_| album_artist.clone())
        .unwrap_or(album_artist);

    Ok(ReleaseDetail {
        release_id: release_id.to_string(),
        title,
        artist: artist_parent,
        album_artist,
        year,
        release_type: rel
            .release_group
            .as_ref()
            .and_then(|g| g.primary_type.clone())
            .unwrap_or_default(),
        tracks,
    })
}

/// Minimal URL encoding for MusicBrainz Lucene queries.
fn urlencoding_lite(s: &str) -> String {
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

/// Title similarity (0..1) for track matching.
/// 简繁/全角/大小写/标点差异视为相同（[`crate::text_norm::match_key`]）；
/// 仅版本后缀不同（Live/Remix/feat…）计 0.92（[`crate::text_norm::title_match_key`]）。
pub fn title_similarity(a: &str, b: &str) -> f64 {
    let na = crate::text_norm::match_key(a);
    let nb = crate::text_norm::match_key(b);
    if na.is_empty() || nb.is_empty() {
        return 0.0;
    }
    if na == nb {
        return 1.0;
    }
    let fa = crate::text_norm::title_match_key(a);
    let fb = crate::text_norm::title_match_key(b);
    if !fa.is_empty() && fa == fb {
        return 0.92;
    }
    if na.contains(&nb) || nb.contains(&na) {
        return 0.85;
    }
    // token overlap
    let sa: std::collections::HashSet<&str> = na.split_whitespace().collect();
    let sb: std::collections::HashSet<&str> = nb.split_whitespace().collect();
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    let inter = sa.intersection(&sb).count();
    (inter as f64) / (sa.len().max(sb.len()) as f64)
}
