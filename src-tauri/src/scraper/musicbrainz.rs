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
}

/// GET /recording/{id}?inc=artist-credits
pub fn fetch_recording(recording_id: &str) -> Result<RecordingDetail> {
    if !crate::net_util::is_mbid(recording_id) {
        return Err(anyhow!("recording id 不合法"));
    }
    let v = mb_get(&format!("recording/{recording_id}?inc=artist-credits&fmt=json"))?;
    let rec: MbRecording = serde_json::from_value(v).context("recording 结构解析失败")?;
    Ok(RecordingDetail {
        id: recording_id.to_string(),
        title: rec.title.unwrap_or_default(),
        artist: credit_name(&rec.artist_credit),
    })
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
pub fn title_similarity(a: &str, b: &str) -> f64 {
    let na = normalize_title(a);
    let nb = normalize_title(b);
    if na.is_empty() || nb.is_empty() {
        return 0.0;
    }
    if na == nb {
        return 1.0;
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

fn normalize_title(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
