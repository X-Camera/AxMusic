//! MusicBrainz + Cover Art Archive scrape (search → candidate → ApplyPlan).
//! Never writes audio files here — writing goes through [`crate::tagger`].

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

mod coverart;
pub mod musicbrainz;

pub use coverart::fetch_front_cover;
pub use musicbrainz::{search_recordings, search_releases};

/// MusicBrainz requires a descriptive User-Agent with contact info.
const USER_AGENT: &str = concat!(
    "AxMusic/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/axmusic/axmusic )"
);

/// ~1 request / second (MusicBrainz rate limit).
const MIN_INTERVAL: Duration = Duration::from_millis(1100);

static RATE: Mutex<Option<Instant>> = Mutex::new(None);

pub fn user_agent() -> &'static str {
    USER_AGENT
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
    /// release-group or recording MBID (lookup key)
    pub id: String,
    /// "release" | "recording"
    pub kind: String,
    pub title: String,
    pub artist: String,
    pub year: String,
    pub track_count: i64,
    pub country: String,
    pub disambiguation: String,
    /// release MBID when kind=release (for tracklist + cover)
    pub release_id: String,
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
    /// MusicBrainz track title we matched to (empty if unmatched)
    pub matched_title: String,
    /// Every field we would write (old → new), including unchanged, for review.
    pub changes: Vec<FieldChange>,
}

/// One catalog row to store when a candidate is adopted.
/// For album mode this covers the WHOLE release tracklist (subset backup of the
/// online DB), not just the locally-matched tracks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogTrackDraft {
    /// recording MBID
    pub mbid: String,
    pub release_mbid: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: String,
    pub track_no: Option<i64>,
    pub release_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPlan {
    pub candidate_id: String,
    pub release_id: String,
    pub candidate_label: String,
    pub tracks: Vec<TrackPlan>,
    /// All catalog rows to persist on adopt (full release for album mode).
    pub catalog_tracks: Vec<CatalogTrackDraft>,
    /// plan intends to cache a front cover into `<library>/covers/`
    pub cover_will_write: bool,
    pub unmatched: Vec<String>,
}
