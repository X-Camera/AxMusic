//! Write tags / cover back to audio files (lofty). Backup before write.

use std::path::Path;

use anyhow::{Context, Result};
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::{Accessor, ItemKey};
use lofty::probe::Probe;

use crate::scraper::FieldChange;

/// Copy original file to backup dir before mutating tags.
pub fn backup_file(path: &Path, backup_root: &Path) -> Result<std::path::PathBuf> {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "track".into());
    let dest_dir = backup_root.join("tag_backups");
    std::fs::create_dir_all(&dest_dir)?;
    let dest = dest_dir.join(format!(
        "{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        name
    ));
    std::fs::copy(path, &dest).with_context(|| format!("备份失败 {}", path.display()))?;
    Ok(dest)
}

fn apply_change(tag: &mut lofty::tag::Tag, field: &str, new: &str) {
    // Never write empty values — an empty catalog field must not wipe file tags.
    if new.trim().is_empty() {
        return;
    }
    match field {
        "title" => {
            tag.set_title(new.to_string());
        }
        "artist" => {
            tag.set_artist(new.to_string());
        }
        "album" => {
            tag.set_album(new.to_string());
        }
        "year" => {
            if let Ok(y) = new.parse::<u32>() {
                tag.set_year(y);
            }
            tag.insert_text(ItemKey::RecordingDate, new.to_string());
        }
        "album_artist" => {
            tag.insert_text(ItemKey::AlbumArtist, new.to_string());
        }
        "track_no" => {
            if let Ok(n) = new.parse::<u32>() {
                tag.set_track(n);
            }
        }
        "lyrics" => {
            tag.insert_text(ItemKey::Lyrics, new.to_string());
        }
        "release_type" => {
            tag.insert_text(ItemKey::Unknown("RELEASETYPE".into()), new.to_string());
        }
        "musicbrainz_recording" => {
            tag.insert_text(ItemKey::MusicBrainzRecordingId, new.to_string());
        }
        "musicbrainz_release" => {
            tag.insert_text(ItemKey::MusicBrainzReleaseId, new.to_string());
        }
        "musicbrainz_releasegroup" => {
            tag.insert_text(ItemKey::MusicBrainzReleaseGroupId, new.to_string());
        }
        "musicbrainz_artist" => {
            tag.insert_text(ItemKey::MusicBrainzArtistId, new.to_string());
        }
        _ => {}
    }
}

/// Write a batch of field changes onto one file. `cover` = raw image bytes (jpeg/png).
pub fn write_track(
    path: &Path,
    changes: &[FieldChange],
    cover: Option<&[u8]>,
    musicbrainz_release: &str,
    musicbrainz_recording: &str,
) -> Result<()> {
    let mut tagged = Probe::open(path)
        .with_context(|| format!("打开失败 {}", path.display()))?
        .read()
        .with_context(|| format!("解析失败 {}", path.display()))?;

    if tagged.primary_tag().is_none() && tagged.first_tag().is_none() {
        let primary = tagged.primary_tag_type();
        let tag = lofty::tag::Tag::new(primary);
        tagged.insert_tag(tag);
    }

    let has_primary = tagged.primary_tag().is_some();
    let tag = if has_primary {
        tagged.primary_tag_mut()
    } else {
        tagged.first_tag_mut()
    }
    .context("无标签容器")?;

    for ch in changes {
        apply_change(tag, &ch.field, &ch.new);
    }
    if !musicbrainz_release.is_empty() {
        tag.insert_text(ItemKey::MusicBrainzReleaseId, musicbrainz_release.to_string());
    }
    if !musicbrainz_recording.is_empty() {
        tag.insert_text(
            ItemKey::MusicBrainzRecordingId,
            musicbrainz_recording.to_string(),
        );
    }
    if let Some(bytes) = cover {
        let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
            Some(MimeType::Png)
        } else {
            Some(MimeType::Jpeg)
        };
        let pic = Picture::new_unchecked(PictureType::CoverFront, mime, None, bytes.to_vec());
        tag.remove_picture_type(PictureType::CoverFront);
        tag.push_picture(pic);
    }

    tagged
        .save_to_path(path, WriteOptions::default())
        .with_context(|| format!("写回失败 {}", path.display()))?;
    Ok(())
}
