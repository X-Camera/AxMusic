//! 归档状态检查与规范化。
//!
//! 当前只检查歌词一项（后续可扩展封面/标签等）。
//! 歌词规范：位于 `<库根>/lrc/` 目录，命名为 `歌手 - 歌名.lrc`。
//! 无歌词不报；有歌词才检查命名与位置。

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::library::TrackRow;
use crate::lyrics;

/// 一项归档问题
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveIssue {
    /// 问题类别："lyrics_name" | "lyrics_location"
    pub kind: String,
    /// 中文描述
    pub message: String,
    /// 当前路径（不存在为空串）
    pub current: String,
    /// 期望路径
    pub expected: String,
}

/// 单曲归档状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveStatus {
    pub ok: bool,
    pub issues: Vec<ArchiveIssue>,
}

/// 期望歌词路径：`<库根>/lrc/{artist} - {title}.lrc`
fn expected_lrc_path(library_root: &Path, artist: &str, title: &str) -> PathBuf {
    library_root
        .join("lrc")
        .join(lyrics::normalized_lrc_filename(artist, title))
}

/// 找到当前外挂歌词实际位置：规范化命名 → stem 命名 → 音频旁 sidecar。
fn find_current_lrc(
    library_root: &Path,
    audio_path: &Path,
    artist: &str,
    title: &str,
) -> Option<PathBuf> {
    // 规范化命名
    let normalized = expected_lrc_path(library_root, artist, title);
    if normalized.is_file() {
        return Some(normalized);
    }
    // stem 命名
    let stem = audio_path.file_stem().unwrap_or_default();
    let mut name = stem.to_os_string();
    name.push(".lrc");
    let stem_path = library_root.join("lrc").join(&name);
    if stem_path.is_file() {
        return Some(stem_path);
    }
    // 音频旁 sidecar
    let sidecar = audio_path.with_extension("lrc");
    if sidecar.is_file() {
        return Some(sidecar);
    }
    None
}

/// 检查单曲归档状态。无歌词 / 未关联 catalog 不报。
pub fn check_track(library_root: &Path, track: &TrackRow) -> ArchiveStatus {
    // 未关联不检查
    if track.catalog_id.is_none() {
        return ArchiveStatus { ok: true, issues: vec![] };
    }
    // 无外挂歌词不报
    if !track.has_lrc {
        return ArchiveStatus { ok: true, issues: vec![] };
    }

    let expected = expected_lrc_path(library_root, &track.artist, &track.title);
    let Some(current) = find_current_lrc(
        library_root,
        Path::new(&track.path),
        &track.artist,
        &track.title,
    ) else {
        // has_lrc 为 true 但找不到文件（可能刚被删），报位置问题
        return ArchiveStatus {
            ok: false,
            issues: vec![ArchiveIssue {
                kind: "lyrics_location".into(),
                message: "外挂歌词文件丢失".into(),
                current: String::new(),
                expected: expected.to_string_lossy().into(),
            }],
        };
    };

    let mut issues = vec![];

    // 位置检查：是否在 <库根>/lrc/
    let lrc_dir = library_root.join("lrc");
    let in_lrc_dir = current.parent().map(|p| p == lrc_dir).unwrap_or(false);
    if !in_lrc_dir {
        issues.push(ArchiveIssue {
            kind: "lyrics_location".into(),
            message: "歌词不在库 lrc/ 目录".into(),
            current: current.to_string_lossy().into(),
            expected: expected.to_string_lossy().into(),
        });
    }

    // 命名检查：是否为 `歌手 - 歌名.lrc`
    let expected_name = expected.file_name().unwrap_or_default();
    let current_name = current.file_name().unwrap_or_default();
    if current_name != expected_name {
        issues.push(ArchiveIssue {
            kind: "lyrics_name".into(),
            message: "歌词命名不规范".into(),
            current: current.to_string_lossy().into(),
            expected: expected.to_string_lossy().into(),
        });
    }

    ArchiveStatus { ok: issues.is_empty(), issues }
}

/// 规范化：把外挂歌词移动到 `<库根>/lrc/{artist} - {title}.lrc`。
pub fn normalize_track(library_root: &Path, track: &TrackRow) -> Result<PathBuf> {
    let expected = expected_lrc_path(library_root, &track.artist, &track.title);
    let current = find_current_lrc(
        library_root,
        Path::new(&track.path),
        &track.artist,
        &track.title,
    )
    .ok_or_else(|| anyhow!("找不到外挂歌词文件"))?;

    if current == expected {
        return Ok(expected);
    }

    // 确保目标目录存在
    if let Some(dir) = expected.parent() {
        std::fs::create_dir_all(dir)?;
    }

    // 读旧写新（跨盘安全），再删旧文件
    let text = crate::paths::read_text_lossy(&current)?;
    crate::paths::write_atomic(&expected, text.as_bytes())?;
    if current != expected {
        let _ = std::fs::remove_file(&current);
    }
    Ok(expected)
}
