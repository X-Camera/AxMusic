//! 元数据刮削：MusicBrainz + iTunes + 网易云 + QQ音乐 四源聚合（搜索 → 候选 → ApplyPlan）。
//! Never writes audio files here — writing goes through [`crate::tagger`].

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

mod coverart;
pub mod itunes;
pub mod musicbrainz;
pub mod netease;
pub mod qqmusic;

pub use coverart::{download_image, search_all as search_covers, CoverCandidate};
pub use musicbrainz::{search_recordings, search_releases};

/// 刮削源标识（写入 catalog.source；与歌词来源各自独立）
pub const SRC_MB: &str = "musicbrainz";
pub const SRC_ITUNES: &str = "itunes";
pub const SRC_NETEASE: &str = "netease";
pub const SRC_QQ: &str = "qq";

/// MusicBrainz requires a descriptive User-Agent with contact info.
const USER_AGENT: &str = concat!(
    "AxMusic/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/axmusic/axmusic )"
);

/// 浏览器 UA：网易云/QQ/封面 CDN 要求。
const UA_BROWSER: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

/// ~1 request / second (MusicBrainz rate limit).
const MIN_INTERVAL: Duration = Duration::from_millis(1100);

static RATE: Mutex<Option<Instant>> = Mutex::new(None);
static APP_CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
static BROWSER_CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();

/// MusicBrainz 合规 UA（对外暴露便于排查/测试）。
#[allow(dead_code)]
pub fn user_agent() -> &'static str {
    USER_AGENT
}

/// 进程级共享 Client（App UA）：复用连接池，避免每请求新建。
pub fn http_client() -> &'static reqwest::blocking::Client {
    APP_CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(6))
            .build()
            .expect("reqwest client")
    })
}

/// 进程级共享 Client（浏览器 UA）：网易云/QQ/封面图床。
pub fn browser_client() -> &'static reqwest::blocking::Client {
    BROWSER_CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .user_agent(UA_BROWSER)
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(6))
            .build()
            .expect("reqwest client")
    })
}

/// Serialize outbound MusicBrainz/CAA calls to ≤1 req/s.
/// Do not hold the lock while sleeping.
pub fn rate_limit_wait() {
    let wait = {
        let slot = RATE.lock().unwrap();
        slot.map(|last| MIN_INTERVAL.saturating_sub(last.elapsed()))
            .unwrap_or_default()
    };
    if !wait.is_zero() {
        std::thread::sleep(wait);
    }
    if let Ok(mut slot) = RATE.lock() {
        *slot = Some(Instant::now());
    }
}

// ── IPC types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeCandidate {
    /// release-group or recording id（各源各自的原始 id，搭配 source 使用）
    pub id: String,
    /// "release" | "recording"
    pub kind: String,
    /// 来源：musicbrainz / itunes / netease / qq
    pub source: String,
    pub title: String,
    pub artist: String,
    pub year: String,
    pub track_count: i64,
    pub country: String,
    pub disambiguation: String,
    /// 发行 id（kind=release 时用于拉曲目表；各源原始 id）
    pub release_id: String,
}

/// 专辑曲目表中的一首（各源通用结构）
#[derive(Debug, Clone)]
pub struct ReleaseTrack {
    pub position: i64,
    /// 碟号（多碟发行；源无此信息时为 1）
    pub disc: i64,
    pub title: String,
    pub artist: String,
    /// 录音 MBID（仅 MusicBrainz 有；其余源为空串）
    pub recording_id: String,
}

/// 发行详情（各源通用结构）
#[derive(Debug, Clone)]
pub struct ReleaseDetail {
    pub release_id: String,
    pub title: String,
    pub artist: String,
    pub album_artist: String,
    pub year: String,
    /// Release-group primary type（仅 MusicBrainz 有；其余源为空串）
    pub release_type: String,
    pub tracks: Vec<ReleaseTrack>,
}

/// 单曲详情（单曲模式各源通用结构）
#[derive(Debug, Clone)]
pub struct TrackDetail {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: String,
}

/// 按来源分发：拉发行曲目表
pub fn fetch_release_by_source(source: &str, id: &str) -> anyhow::Result<ReleaseDetail> {
    match source {
        SRC_MB => musicbrainz::fetch_release(id),
        SRC_ITUNES => itunes::fetch_release(id),
        SRC_NETEASE => netease::fetch_release(id),
        SRC_QQ => qqmusic::fetch_release(id),
        other => anyhow::bail!("未知刮削源: {other}"),
    }
}

/// 按来源分发：拉单曲详情
pub fn fetch_track_by_source(source: &str, id: &str) -> anyhow::Result<TrackDetail> {
    match source {
        SRC_MB => musicbrainz::fetch_recording(id).map(|r| TrackDetail {
            id: r.id,
            title: r.title,
            artist: r.artist,
            album: String::new(),
            year: String::new(),
        }),
        SRC_ITUNES => itunes::fetch_track(id),
        SRC_NETEASE => netease::fetch_track(id),
        SRC_QQ => qqmusic::fetch_track(id),
        other => anyhow::bail!("未知刮削源: {other}"),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChange {
    pub field: String,
    pub old: String,
    pub new: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackPlan {
    pub track_id: i64,
    pub path: String,
    /// local display title
    pub display: String,
    /// 云端匹配到的曲名（空 = 未匹配）
    pub matched_title: String,
    /// Every field we would write (old → new), including unchanged, for review.
    pub changes: Vec<FieldChange>,
}

/// One catalog row to store when a candidate is adopted.
/// For album mode this covers the WHOLE release tracklist (subset backup of the
/// online DB), not just the locally-matched tracks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogTrackDraft {
    /// 录音 MBID（仅 MusicBrainz 源有）
    pub mbid: String,
    /// 发行 id（各源原始 id，搭配 catalog.source 使用）
    pub release_mbid: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: String,
    pub track_no: Option<i64>,
    /// 碟号（多碟发行；单碟/无信息为 None）
    #[serde(default)]
    pub disc_no: Option<i64>,
    pub release_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPlan {
    pub candidate_id: String,
    pub release_id: String,
    /// 来源（写入 catalog.source）
    pub source: String,
    pub candidate_label: String,
    pub tracks: Vec<TrackPlan>,
    /// All catalog rows to persist on adopt (full release for album mode).
    pub catalog_tracks: Vec<CatalogTrackDraft>,
    pub unmatched: Vec<String>,
}

