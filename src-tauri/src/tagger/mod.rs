//! Write tags / cover back to audio files (lofty).
//!
//! 写回安全契约：
//! - 全程在**同目录临时副本**上改标签，成功后 `rename` 原子替换原文件；
//!   中途崩溃/失败只留无害临时文件（`.axtmp-*`），原音频不动。
//! - 只写 primary 标签容器（缺失则新建），不向 ID3v1 等受限次级容器退化。
//! - 空值永不写入；字段值非法（track_no/year）或字段名未知时显式报错。

use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::{Accessor, ItemKey};
use lofty::probe::Probe;

use crate::scraper::FieldChange;

fn apply_change(tag: &mut lofty::tag::Tag, field: &str, new: &str) -> Result<()> {
    // Never write empty values — an empty catalog field must not wipe file tags.
    if new.trim().is_empty() {
        return Ok(());
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
            // RecordingDate 保存原始日期串（可为 "2019-05-01"）；
            // lofty 的 set_year 同样落在 RecordingDate，无需重复写。
            let b = new.as_bytes();
            if b.len() < 4 || !b[..4].iter().all(|c| c.is_ascii_digit()) {
                bail!("year 字段值非法（应为年份或日期）: {new:?}");
            }
            tag.insert_text(ItemKey::RecordingDate, new.to_string());
        }
        "album_artist" => {
            tag.insert_text(ItemKey::AlbumArtist, new.to_string());
        }
        "track_no" => {
            let n: u32 = new
                .trim()
                .parse()
                .map_err(|_| anyhow!("track_no 字段值非法（应为数字）: {new:?}"))?;
            tag.set_track(n);
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
        other => bail!("不支持的标签字段: {other}"),
    }
    Ok(())
}

/// 封面字节 → MIME：只认 PNG/JPEG 魔数，其余（WebP/GIF/截断数据）拒绝写入。
fn cover_mime(bytes: &[u8]) -> Result<MimeType> {
    if bytes.is_empty() {
        bail!("封面数据为空");
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Ok(MimeType::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Ok(MimeType::Jpeg)
    } else {
        bail!("无法识别的封面格式（仅支持 PNG/JPEG）");
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
    let tmp = crate::paths::temp_path_for(path);
    let result = write_track_inner(path, &tmp, changes, cover, musicbrainz_release, musicbrainz_recording);
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn write_track_inner(
    path: &Path,
    tmp: &Path,
    changes: &[FieldChange],
    cover: Option<&[u8]>,
    musicbrainz_release: &str,
    musicbrainz_recording: &str,
) -> Result<()> {
    // 显式 file_type：临时文件名没有音频扩展名，不靠内容嗅探碰运气
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
    let ft = FileType::from_ext(ext).ok_or_else(|| anyhow!("无法识别音频格式: {ext}"))?;

    // 复制原文件 → 临时副本，之后所有改动都落在副本上
    std::fs::copy(path, tmp).with_context(|| format!("创建临时副本失败 {}", path.display()))?;

    let mut tagged = Probe::open(tmp)
        .with_context(|| format!("打开失败 {}", path.display()))?
        .set_file_type(ft)
        .read()
        .with_context(|| format!("解析失败 {}", path.display()))?;

    // 始终写 primary 容器；缺失（含仅 ID3v1 的文件）就新建，
    // 不向 ID3v1 等受限次级标签退化（定长 30 字节会静默截断/丢字段）
    if tagged.primary_tag().is_none() {
        let primary = tagged.primary_tag_type();
        tagged.insert_tag(lofty::tag::Tag::new(primary));
    }
    let tag = tagged.primary_tag_mut().context("无标签容器")?;

    for ch in changes {
        apply_change(tag, &ch.field, &ch.new)?;
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
        let pic = Picture::new_unchecked(
            PictureType::CoverFront,
            Some(cover_mime(bytes)?),
            None,
            bytes.to_vec(),
        );
        tag.remove_picture_type(PictureType::CoverFront);
        tag.push_picture(pic);
    }

    tagged
        .save_to_path(tmp, WriteOptions::default())
        .with_context(|| format!("写回失败 {}", path.display()))?;
    std::fs::rename(tmp, path).with_context(|| format!("替换失败 {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scraper::FieldChange;

    fn ch(field: &str, new: &str) -> FieldChange {
        FieldChange {
            field: field.into(),
            old: String::new(),
            new: new.into(),
        }
    }

    /// 最小合法 PCM WAV（8kHz 单声道 8bit 静音）
    fn make_wav(path: &Path) {
        let data_size = 1600u32;
        let mut v: Vec<u8> = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data_size).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes()); // PCM
        v.extend_from_slice(&1u16.to_le_bytes()); // mono
        v.extend_from_slice(&8000u32.to_le_bytes());
        v.extend_from_slice(&8000u32.to_le_bytes()); // byte rate
        v.extend_from_slice(&1u16.to_le_bytes()); // block align
        v.extend_from_slice(&8u16.to_le_bytes()); // bits
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data_size.to_le_bytes());
        v.extend(std::iter::repeat(0u8).take(data_size as usize));
        std::fs::write(path, v).unwrap();
    }

    #[test]
    fn atomic_write_roundtrip_and_failure_safety() {
        let dir = std::env::temp_dir().join(format!("axmusic-tagger-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.wav");
        make_wav(&file);

        write_track(
            &file,
            &[ch("title", "晴天"), ch("track_no", "3"), ch("year", "2003-07-31")],
            None,
            "",
            "",
        )
        .unwrap();

        // 标签可完整读回
        let row = crate::scanner::read_track(&file).unwrap();
        assert_eq!(row.title, "晴天");
        assert_eq!(row.track_no, Some(3));
        // 完整日期串在 RecordingDate 里（scanner 展示侧 year() 只取年份部分）
        let tagged = Probe::open(&file).unwrap().read().unwrap();
        let tag = tagged.primary_tag().unwrap();
        assert_eq!(
            tag.get_string(&ItemKey::RecordingDate).unwrap(),
            "2003-07-31"
        );
        // 临时文件无残留
        assert!(!std::fs::read_dir(&dir)
            .unwrap()
            .any(|e| e.unwrap().file_name().to_string_lossy().contains("axtmp")));

        // 失败不写原文件：非法 track_no / 未知字段 / 垃圾封面都报错，且文件字节不变
        for bad in [
            vec![ch("track_no", "abc")],
            vec![ch("year", "今年")],
            vec![ch("composer", "x")],
            vec![],
        ] {
            let snapshot = std::fs::read(&file).unwrap();
            let cover: Option<&[u8]> = if bad.is_empty() { Some(b"not-an-image") } else { None };
            assert!(write_track(&file, &bad, cover, "", "").is_err());
            assert_eq!(std::fs::read(&file).unwrap(), snapshot);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
