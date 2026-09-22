//! 封面多源刮取：CAA（按发行 MBID）/ iTunes / 网易云 / QQ音乐（按专辑+歌手）。
//! 只产出候选与下载封面字节，存盘/写 tag 走 commands/tagger。失败快速返回，不阻塞采纳。

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::rate_limit_wait;
use crate::lyrics::encode;

const UA_BROWSER: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

/// 封面候选（搜索结果项）。`url` 为大图地址，采纳时 [`download_image`] 下载。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverCandidate {
    pub id: String,
    /// "caa" | "itunes" | "netease" | "qq"
    pub source: String,
    pub title: String,
    pub artist: String,
    pub thumb_url: String,
    pub url: String,
}

/// 四源并发搜索封面候选（失败的源跳过）。顺序：CAA → iTunes → 网易云 → QQ音乐。
pub fn search_all(release_mbid: &str, album: &str, artist: &str) -> Vec<CoverCandidate> {
    let mut out = Vec::new();
    std::thread::scope(|s| {
        let handles = [
            s.spawn(|| search_caa(release_mbid)),
            s.spawn(|| search_itunes(album, artist)),
            s.spawn(|| search_netease(album, artist)),
            s.spawn(|| search_qq(album, artist)),
        ];
        for h in handles {
            if let Ok(Ok(mut v)) = h.join() {
                out.append(&mut v);
            }
        }
    });
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert(c.url.clone()));
    out
}

/// 下载封面字节（>1KB 才算有效）。网络错误给出可操作的中文提示。
pub fn download_image(url: &str) -> Result<Vec<u8>> {
    let resp = get(url).map_err(|e| friendly_net_err(&e))?;
    if !resp.status().is_success() {
        bail!("封面下载失败：HTTP {}", resp.status());
    }
    let bytes = resp.bytes().map_err(|e| friendly_net_err(&e))?;
    if bytes.len() < 1024 {
        bail!("封面下载失败：图片数据无效");
    }
    Ok(bytes.to_vec())
}

fn get(url: &str) -> reqwest::Result<reqwest::blocking::Response> {
    reqwest::blocking::Client::builder()
        .user_agent(UA_BROWSER)
        .timeout(std::time::Duration::from_secs(15))
        .connect_timeout(std::time::Duration::from_secs(6))
        .build()?
        .get(url)
        .send()
}

/// 网络层错误 → 友好提示（境外源国内常见不可达）。
fn friendly_net_err(e: &reqwest::Error) -> anyhow::Error {
    if e.is_timeout() || e.is_connect() {
        anyhow!("网络不可达（archive.org 等境外源国内可能需要代理，可改用 iTunes / 网易云 / QQ音乐 候选）")
    } else {
        anyhow!("封面请求失败：{e}")
    }
}

// ── CAA（按发行 MBID，最精确；图片托管在 archive.org，国内可能需代理）──

#[derive(Debug, Deserialize)]
struct CaaImages {
    images: Vec<CaaImage>,
}

#[derive(Debug, Deserialize)]
struct CaaImage {
    front: bool,
    image: Option<String>,
    thumbnails: Option<CaaThumbs>,
}

#[derive(Debug, Deserialize)]
struct CaaThumbs {
    #[serde(rename = "250")]
    t250: Option<String>,
    #[serde(rename = "500")]
    t500: Option<String>,
    large: Option<String>,
}

fn search_caa(release_mbid: &str) -> Result<Vec<CoverCandidate>> {
    let mbid = release_mbid.trim();
    if mbid.is_empty() {
        return Ok(Vec::new());
    }
    rate_limit_wait();
    let url = format!("https://coverartarchive.org/release/{mbid}");
    let resp = get(&url).map_err(|e| friendly_net_err(&e))?;
    if !resp.status().is_success() {
        return Ok(Vec::new()); // 404 = 该发行无封面，不算错误
    }
    let data: CaaImages = resp.json().context("CAA JSON 解析失败")?;
    Ok(data
        .images
        .into_iter()
        .filter(|i| i.front)
        .filter_map(|i| {
            let full = i.image?;
            let thumb = i
                .thumbnails
                .and_then(|t| t.t500.or(t.t250.or(t.large)))
                .unwrap_or_else(|| full.clone());
            Some(CoverCandidate {
                id: format!("caa:{mbid}"),
                source: "caa".into(),
                title: "Cover Art Archive（正封）".into(),
                artist: String::new(),
                thumb_url: thumb,
                url: full,
            })
        })
        .collect())
}

// ── iTunes Search API（官方免费，国内直连）──────────────────────────

#[derive(Debug, Deserialize)]
struct ItunesResp {
    results: Vec<ItunesAlbum>,
}

#[derive(Debug, Deserialize)]
struct ItunesAlbum {
    #[serde(rename = "collectionName")]
    collection_name: Option<String>,
    #[serde(rename = "artistName")]
    artist_name: Option<String>,
    #[serde(rename = "artworkUrl100")]
    artwork_url100: Option<String>,
    #[serde(rename = "artworkUrl60")]
    artwork_url60: Option<String>,
}

fn search_itunes(album: &str, artist: &str) -> Result<Vec<CoverCandidate>> {
    let q = format!("{} {}", album.trim(), artist.trim()).trim().to_string();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let url = format!(
        "https://itunes.apple.com/search?media=music&entity=album&limit=4&country=CN&term={}",
        encode(&q)
    );
    let resp = get(&url).map_err(|e| friendly_net_err(&e))?;
    if !resp.status().is_success() {
        return Ok(Vec::new());
    }
    let data: ItunesResp = resp.json().context("iTunes JSON 解析失败")?;
    Ok(data
        .results
        .into_iter()
        .filter_map(|a| {
            let art = a.artwork_url100.or(a.artwork_url60)?;
            // 100x100bb / 60x60bb → 大图 600x600bb，缩略保持原 URL
            let full = art
                .replace("100x100bb", "600x600bb")
                .replace("60x60bb", "600x600bb");
            Some(CoverCandidate {
                id: format!("itunes:{}", full),
                source: "itunes".into(),
                title: a.collection_name.unwrap_or_default(),
                artist: a.artist_name.unwrap_or_default(),
                thumb_url: art,
                url: full,
            })
        })
        .collect())
}

// ── 网易云（专辑搜索，图床国内 CDN）────────────────────────────────

#[derive(Debug, Deserialize)]
struct NeteaseResp {
    result: Option<NeteaseResult>,
}

#[derive(Debug, Deserialize)]
struct NeteaseResult {
    albums: Option<Vec<NeteaseAlbum>>,
}

#[derive(Debug, Deserialize)]
struct NeteaseAlbum {
    name: Option<String>,
    #[serde(rename = "picUrl")]
    pic_url: Option<String>,
    artist: Option<NeteaseArtist>,
}

#[derive(Debug, Deserialize)]
struct NeteaseArtist {
    name: Option<String>,
}

fn search_netease(album: &str, artist: &str) -> Result<Vec<CoverCandidate>> {
    let q = format!("{} {}", album.trim(), artist.trim()).trim().to_string();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    // type=10 专辑搜索
    let url = format!(
        "https://music.163.com/api/search/get/web?type=10&limit=4&s={}",
        encode(&q)
    );
    // 网易云要求带 Referer
    let resp = reqwest::blocking::Client::builder()
        .user_agent(UA_BROWSER)
        .timeout(std::time::Duration::from_secs(15))
        .build()?
        .get(&url)
        .header("Referer", "https://music.163.com")
        .header("Accept", "application/json")
        .send()
        .map_err(|e| friendly_net_err(&e))?;
    if !resp.status().is_success() {
        return Ok(Vec::new());
    }
    let data: NeteaseResp = resp.json().context("网易云 JSON 解析失败")?;
    Ok(data
        .result
        .and_then(|r| r.albums)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|a| {
            let pic = a.pic_url?;
            let pic = pic.replace("http://", "https://");
            Some(CoverCandidate {
                id: format!("netease:{}", pic),
                source: "netease".into(),
                title: a.name.unwrap_or_default(),
                artist: a.artist.and_then(|x| x.name).unwrap_or_default(),
                thumb_url: format!("{pic}?param=300y300"),
                url: format!("{pic}?param=1000y1000"),
            })
        })
        .collect())
}

// ── QQ音乐（曲目搜索取专辑图，图床国内 CDN）────────────────────────

#[derive(Debug, Deserialize)]
struct QqResp {
    data: Option<QqData>,
}

#[derive(Debug, Deserialize)]
struct QqData {
    song: Option<QqSongList>,
}

#[derive(Debug, Deserialize)]
struct QqSongList {
    list: Option<Vec<QqSong>>,
}

#[derive(Debug, Deserialize)]
struct QqSong {
    singer: Option<Vec<QqSinger>>,
    albumname: Option<String>,
    albummid: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QqSinger {
    name: Option<String>,
}

fn search_qq(album: &str, artist: &str) -> Result<Vec<CoverCandidate>> {
    let q = format!("{} {}", album.trim(), artist.trim()).trim().to_string();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let url = format!(
        "https://c.y.qq.com/soso/fcgi-bin/client_search_cp?format=json&n=4&p=1&t=0&w={}",
        encode(&q)
    );
    let resp = reqwest::blocking::Client::builder()
        .user_agent(UA_BROWSER)
        .timeout(std::time::Duration::from_secs(15))
        .build()?
        .get(&url)
        .header("Referer", "https://y.qq.com")
        .header("Accept", "application/json")
        .send()
        .map_err(|e| friendly_net_err(&e))?;
    if !resp.status().is_success() {
        return Ok(Vec::new());
    }
    let data: QqResp = resp.json().context("QQ音乐 JSON 解析失败")?;
    let list = data
        .data
        .and_then(|d| d.song)
        .and_then(|s| s.list)
        .unwrap_or_default();
    let mut out = Vec::new();
    for s in list {
        let Some(mid) = s.albummid.filter(|m| !m.is_empty()) else {
            continue;
        };
        let artist = s
            .singer
            .unwrap_or_default()
            .into_iter()
            .filter_map(|x| x.name)
            .collect::<Vec<_>>()
            .join("/");
        out.push(CoverCandidate {
            id: format!("qq:{mid}"),
            source: "qq".into(),
            title: s.albumname.unwrap_or_default(),
            artist,
            thumb_url: format!("https://y.gtimg.cn/music/photo_new/T002R300x300M000{mid}.jpg"),
            url: format!("https://y.gtimg.cn/music/photo_new/T002R500x500M000{mid}.jpg"),
        });
    }
    Ok(out)
}
