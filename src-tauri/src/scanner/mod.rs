//! Incremental library scanner (FLAC / MP3 for this slice).
//! Reads tags via lofty; writes archives to SQLite. Background-safe.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::Accessor;
use lofty::probe::Probe;
use lofty::tag::ItemKey;
use walkdir::WalkDir;

use crate::library::{LibraryDb, TrackRow};

pub const AUDIO_EXTS: &[&str] = &["flac", "mp3", "m4a", "mp4", "aac", "ogg", "opus", "wav", "aiff"];

pub struct ScanProgress {
    pub scanned: u64,
    pub total_files: u64,
    pub added: u64,
    pub updated: u64,
    pub errors: u64,
    pub current: String,
}

/// Collect audio files under `root`.
pub fn collect_audio_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        if AUDIO_EXTS.contains(&ext.as_str()) {
            out.push(entry.path().to_path_buf());
        }
    }
    out
}

pub struct ScanStats {
    pub added: u64,
    pub updated: u64,
    pub errors: u64,
    pub total: u64,
}

/// Full incremental scan of `root` into `db`.
/// `on_progress` is invoked every file.
pub fn scan_library<F>(
    db: &LibraryDb,
    root: &Path,
    mut on_progress: F,
) -> Result<ScanStats>
where
    F: FnMut(&ScanProgress),
{
    let files = collect_audio_files(root);
    let total = files.len() as u64;
    let mut present = Vec::with_capacity(files.len());
    let mut added = 0u64;
    let mut updated = 0u64;
    let mut errors = 0u64;

    for (i, path) in files.iter().enumerate() {
        let path_str = path.to_string_lossy().to_string();
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        match read_track(path) {
            Ok(mut row) => {
                let meta = std::fs::metadata(path).ok();
                let file_size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let mtime = meta
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                row.path = path_str.clone();
                row.filename = name.clone();
                let existed = db.get_track_by_path(&path_str)?.is_some();
                db.upsert_track(&row, file_size, mtime)?;
                if existed {
                    updated += 1;
                } else {
                    added += 1;
                }
                present.push(path_str.clone());
            }
            Err(_) => {
                errors += 1;
            }
        }

        on_progress(&ScanProgress {
            scanned: (i as u64) + 1,
            total_files: total,
            added,
            updated,
            errors,
            current: name,
        });
    }

    db.mark_missing_paths(&present)?;

    Ok(ScanStats {
        added,
        updated,
        errors,
        total,
    })
}

/// Parse one audio file into a `TrackRow` (path filled by caller).
pub fn read_track(path: &Path) -> Result<TrackRow> {
    let tagged = Probe::open(path)?.read()?;
    let props = tagged.properties();
    let duration_ms = props.duration().as_millis() as i64;
    let sample_rate = props.sample_rate().map(|v| v as i64);
    let bit_rate = props.audio_bitrate().map(|v| v as i64);

    let format = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

    let mut row = TrackRow {
        id: 0,
        path: path.to_string_lossy().to_string(),
        filename: path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        title: String::new(),
        artist: String::new(),
        album: String::new(),
        album_artist: String::new(),
        year: String::new(),
        track_no: None,
        duration_ms,
        format,
        sample_rate,
        bit_rate,
        has_cover: false,
        has_lyrics: false,
        has_year: false,
        has_mb_id: false,
        tag_status: "unmatched".into(),
        missing: String::new(),
        release_type: String::new(),
        mb_recording_mbid: String::new(),
        mb_release_mbid: String::new(),
        catalog_id: None,
    };

    if let Some(tag) = tag {
        row.title = tag.title().map(|s| s.to_string()).unwrap_or_default();
        row.artist = tag.artist().map(|s| s.to_string()).unwrap_or_default();
        row.album = tag.album().map(|s| s.to_string()).unwrap_or_default();
        row.year = tag
            .year()
            .map(|y| y.to_string())
            .or_else(|| {
                tag.get_string(&ItemKey::RecordingDate)
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();
        row.has_year = !row.year.is_empty();
        row.track_no = tag.track().map(|t| t as i64);

        row.album_artist = tag
            .get_string(&ItemKey::AlbumArtist)
            .map(|s| s.to_string())
            .unwrap_or_default();

        // Album type (Album / EP / Single …) when the file carries it.
        // Picard writes RELEASETYPE (Vorbis) / TXXX:RELEASETYPE; fall back to MB album type.
        row.release_type = tag
            .get_string(&ItemKey::Unknown("RELEASETYPE".into()))
            .or_else(|| tag.get_string(&ItemKey::Unknown("MusicBrainz Album Type".into())))
            .map(|s| s.to_string())
            .unwrap_or_default();

        // Any picture counts as cover for status chip
        row.has_cover = !tag.pictures().is_empty();

        row.has_lyrics = tag
            .get_string(&ItemKey::Lyrics)
            .map(|s| !s.is_empty())
            .unwrap_or(false);

        // MBID values (used for catalog matching) — recording first, then track id.
        row.mb_recording_mbid = tag
            .get_string(&ItemKey::MusicBrainzRecordingId)
            .or_else(|| tag.get_string(&ItemKey::MusicBrainzTrackId))
            .map(|s| s.to_string())
            .unwrap_or_default();
        row.mb_release_mbid = tag
            .get_string(&ItemKey::MusicBrainzReleaseId)
            .map(|s| s.to_string())
            .unwrap_or_default();

        let mb_keys = [
            ItemKey::MusicBrainzRecordingId,
            ItemKey::MusicBrainzTrackId,
            ItemKey::MusicBrainzReleaseId,
            ItemKey::MusicBrainzArtistId,
            ItemKey::MusicBrainzReleaseGroupId,
        ];
        row.has_mb_id = !row.mb_recording_mbid.is_empty()
            || !row.mb_release_mbid.is_empty()
            || mb_keys
                .iter()
                .any(|k| tag.get_string(k).map(|s| !s.is_empty()).unwrap_or(false));
    }

    // Fallback title from filename when tag empty
    if row.title.is_empty() {
        row.title = row
            .filename
            .rsplit_once('.')
            .map(|(s, _)| s)
            .unwrap_or(&row.filename)
            .to_string();
    }

    compute_status(&mut row);
    Ok(row)
}

fn compute_status(row: &mut TrackRow) {
    let mut missing: Vec<&str> = Vec::new();
    if !row.has_cover {
        missing.push("封");
    }
    if !row.has_lyrics {
        missing.push("词");
    }
    if !row.has_year {
        missing.push("年");
    }
    if row.release_type.is_empty() {
        missing.push("型");
    }
    if !row.has_mb_id {
        missing.push("MB");
    }
    row.missing = missing.join(",");

    let core_ok = !row.title.is_empty() && !row.artist.is_empty() && !row.album.is_empty();
    let complete = core_ok && row.has_cover && row.has_year && row.has_mb_id;
    row.tag_status = if complete {
        "complete".into()
    } else if core_ok || !row.artist.is_empty() {
        "partial".into()
    } else {
        "unmatched".into()
    };
}

/// Sanitize a single path segment (see docs/产品需求.md §6).
pub fn sanitize_segment(raw: &str) -> String {
    let mut s: String = raw
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect();
    // strip leading/trailing whitespace and dots
    s = s.trim().trim_matches('.').trim().to_string();
    if s.is_empty() {
        return "Unknown".into();
    }
    // Windows reserved names
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
        "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let upper = s.to_ascii_uppercase();
    if RESERVED.iter().any(|r| *r == upper.as_str()) {
        s = format!("_{s}");
    }
    s
}
