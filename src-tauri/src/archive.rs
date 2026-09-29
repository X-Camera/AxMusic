//! 归档状态检查与规范化 + 库根杂项扫描。
//!
//! 歌曲规范：`<库根>/archived/{artist}/{artist} - {title}{version}.{ext}`
//! 歌词规范：`<库根>/lrc/{artist} - {title}{version}.lrc`（version 与音频一致）
//! 库根白名单外的直接子项视为杂项，整理进 `Unarchived/`。
//!
//! 详见 docs/歌曲归档.md。

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::library::TrackRow;
use crate::paths;
use crate::scanner::{self, AUDIO_EXTS};

/// 一项归档问题
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveIssue {
    /// "song_location" | "song_name" | "lyrics_location" | "lyrics_name"
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

/// 单曲整理结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizeResult {
    /// 整理后的音频路径
    pub song_path: String,
    /// 整理后的歌词路径（无外挂歌词为 None）
    pub lrc_path: Option<String>,
    pub message: String,
}

// ── 命名 ───────────────────────────────────────────────────────────

/// 从文件主名末尾提取版本括号，如 `晴天(Live)` → `(Live)`。
/// 只认末尾一层 `(...)`；空括号或嵌套括号不提取。
pub fn extract_version_suffix(stem: &str) -> Option<String> {
    let t = stem.trim_end();
    let rest = t.strip_suffix(')')?;
    let open = rest.rfind('(')?;
    let inner = &rest[open + 1..];
    if inner.is_empty() || inner.contains(['(', ')']) {
        return None;
    }
    Some(format!("({inner})"))
}

fn norm_artist(artist: &str) -> String {
    let a = artist.trim();
    if a.is_empty() {
        "Unknown Artist".into()
    } else {
        scanner::sanitize_segment(a)
    }
}

fn norm_title(title: &str, fallback_stem: &str) -> String {
    let t = title.trim();
    if t.is_empty() {
        if fallback_stem.trim().is_empty() {
            "Unknown".into()
        } else {
            scanner::sanitize_segment(fallback_stem)
        }
    } else {
        scanner::sanitize_segment(t)
    }
}

fn file_ext(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Windows 路径比较（忽略大小写）。
fn path_eq(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

/// 期望文件主名（不含扩展名）：`{artist} - {title}{version}`
pub fn expected_stem(artist: &str, title: &str, version: Option<&str>) -> String {
    let v = version.unwrap_or("");
    format!("{} - {}{}", norm_artist(artist), norm_title(title, ""), v)
}

/// 期望歌曲文件名：`{artist} - {title}{version}.{ext}`
pub fn expected_song_filename(
    artist: &str,
    title: &str,
    version: Option<&str>,
    ext: &str,
) -> String {
    let ext = if ext.is_empty() {
        "bin".to_string()
    } else {
        ext.to_ascii_lowercase()
    };
    format!("{}.{}", expected_stem(artist, title, version), ext)
}

/// 期望歌曲路径：`<库根>/archived/{artist}/{artist} - {title}{version}.{ext}`
/// version 取自当前音频文件名末尾括号。
pub fn expected_song_path(library_root: &Path, track: &TrackRow) -> PathBuf {
    let audio = Path::new(&track.path);
    let stem = audio
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let version = extract_version_suffix(&stem);
    let title = title_with_fallback(track, &stem, version.as_deref());
    expected_song_path_from(library_root, &track.artist, &title, audio)
}

/// 由歌手/歌名/源文件推期望歌曲路径（纳入库管理等无完整 TrackRow 的场景）。
pub fn expected_song_path_from(
    library_root: &Path,
    artist: &str,
    title: &str,
    src: &Path,
) -> PathBuf {
    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let version = extract_version_suffix(&stem);
    let title = if title.trim().is_empty() {
        let mut s = stem.as_str();
        if let Some(v) = version.as_deref() {
            if let Some(stripped) = s.strip_suffix(v) {
                s = stripped.trim_end();
            }
        }
        s.to_string()
    } else {
        title.to_string()
    };
    let name = expected_song_filename(artist, &title, version.as_deref(), &file_ext(src));
    paths::library_archived_dir(library_root)
        .join(norm_artist(artist))
        .join(name)
}

/// 期望歌词路径：`<库根>/lrc/{artist} - {title}{version}.lrc`
pub fn expected_lrc_path(library_root: &Path, track: &TrackRow) -> PathBuf {
    let audio = Path::new(&track.path);
    let stem = audio
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let version = extract_version_suffix(&stem);
    let title = title_with_fallback(track, &stem, version.as_deref());
    let name = format!(
        "{}.lrc",
        expected_stem(&track.artist, &title, version.as_deref())
    );
    paths::library_lrc_dir(library_root).join(name)
}

/// 标题空时用原主名（去掉版本括号）回退。
fn title_with_fallback(track: &TrackRow, stem: &str, version: Option<&str>) -> String {
    if !track.title.trim().is_empty() {
        return track.title.clone();
    }
    let mut s = stem;
    if let Some(v) = version {
        if let Some(stripped) = s.strip_suffix(v) {
            s = stripped.trim_end();
        }
    }
    s.to_string()
}

// ── 检查 ───────────────────────────────────────────────────────────

/// 找到当前外挂歌词实际位置：规范化命名 → stem 命名 → 音频旁 sidecar → 音频旁规范化名。
fn find_current_lrc(library_root: &Path, track: &TrackRow) -> Option<PathBuf> {
    let audio_path = Path::new(&track.path);
    let normalized = expected_lrc_path(library_root, track);
    if normalized.is_file() {
        return Some(normalized);
    }
    let stem = audio_path.file_stem().unwrap_or_default();
    let mut name = stem.to_os_string();
    name.push(".lrc");
    let stem_path = paths::library_lrc_dir(library_root).join(&name);
    if stem_path.is_file() {
        return Some(stem_path);
    }
    let sidecar = audio_path.with_extension("lrc");
    if sidecar.is_file() {
        return Some(sidecar);
    }
    // 音频旁规范化名（命名整理后、尚未进 lrc/ 时）
    if let (Some(parent), Some(fname)) = (audio_path.parent(), normalized.file_name()) {
        let beside = parent.join(fname);
        if beside.is_file() {
            return Some(beside);
        }
    }
    None
}

/// 挪/改名音频时，把旁边的同主名 `.lrc` 一起带走（保持 sidecar 关系）。
/// 不把歌词送进 lrc/，避免「修歌曲」顺手修掉歌词归档意见。
fn move_sidecar_with_audio(src_audio: &Path, dest_audio: &Path) {
    let src_lrc = src_audio.with_extension("lrc");
    if !src_lrc.is_file() {
        return;
    }
    let dest_lrc = dest_audio.with_extension("lrc");
    if path_eq(&src_lrc, &dest_lrc) || dest_lrc.exists() {
        return;
    }
    if let Some(dir) = dest_lrc.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::rename(&src_lrc, &dest_lrc);
}

/// 检查单曲归档状态（歌曲文件 + 外挂歌词）。
/// **仅已关联 catalog 的曲目**做归档要求；未关联不报（前端提示先刮削）。
/// 歌词仅在有外挂歌词时查。
pub fn check_track(library_root: &Path, track: &TrackRow) -> ArchiveStatus {
    if track.catalog_id.is_none() {
        return ArchiveStatus {
            ok: true,
            issues: vec![],
        };
    }
    let mut issues = vec![];

    // ── 歌曲文件 ──
    let expected = expected_song_path(library_root, track);
    let current = Path::new(&track.path);
    if !current.is_file() {
        issues.push(ArchiveIssue {
            kind: "song_location".into(),
            message: "歌曲文件丢失".into(),
            current: track.path.clone(),
            expected: expected.to_string_lossy().into(),
        });
    } else {
        let expected_parent = expected.parent();
        let current_parent = current.parent();
        let in_place = match (current_parent, expected_parent) {
            (Some(c), Some(e)) => path_eq(c, e),
            _ => false,
        };
        if !in_place {
            issues.push(ArchiveIssue {
                kind: "song_location".into(),
                message: "歌曲不在归档目录".into(),
                current: current.to_string_lossy().into(),
                expected: expected.to_string_lossy().into(),
            });
        }

        let expected_name = expected.file_name().unwrap_or_default();
        let current_name = current.file_name().unwrap_or_default();
        let name_ok = current_name
            .to_string_lossy()
            .eq_ignore_ascii_case(&expected_name.to_string_lossy());
        if !name_ok {
            issues.push(ArchiveIssue {
                kind: "song_name".into(),
                message: "歌曲命名不规范".into(),
                current: current.to_string_lossy().into(),
                expected: expected.to_string_lossy().into(),
            });
        }
    }

    // ── 歌词 ──（能找到就查；has_lrc 滞后时也按磁盘实况报）
    let expected_lrc = expected_lrc_path(library_root, track);
    match find_current_lrc(library_root, track) {
        Some(current) => {
            let lrc_dir = paths::library_lrc_dir(library_root);
            let in_lrc_dir = current.parent().map(|p| p == lrc_dir).unwrap_or(false);
            if !in_lrc_dir {
                issues.push(ArchiveIssue {
                    kind: "lyrics_location".into(),
                    message: "歌词不在库 lrc/ 目录".into(),
                    current: current.to_string_lossy().into(),
                    expected: expected_lrc.to_string_lossy().into(),
                });
            }
            let expected_name = expected_lrc.file_name().unwrap_or_default();
            let current_name = current.file_name().unwrap_or_default();
            if current_name != expected_name {
                issues.push(ArchiveIssue {
                    kind: "lyrics_name".into(),
                    message: "歌词命名不规范".into(),
                    current: current.to_string_lossy().into(),
                    expected: expected_lrc.to_string_lossy().into(),
                });
            }
        }
        None => {
            if track.has_lrc {
                issues.push(ArchiveIssue {
                    kind: "lyrics_location".into(),
                    message: "外挂歌词文件丢失".into(),
                    current: String::new(),
                    expected: expected_lrc.to_string_lossy().into(),
                });
            }
        }
    }

    ArchiveStatus {
        ok: issues.is_empty(),
        issues,
    }
}

// ── 规范化 ─────────────────────────────────────────────────────────

/// 单条归档意见的整理结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueFixResult {
    pub message: String,
    /// 歌曲路径若变化
    pub song_path: Option<String>,
    /// 歌词路径若变化
    pub lrc_path: Option<String>,
}

fn move_file(src: &Path, dest: &Path) -> Result<()> {
    if path_eq(src, dest) {
        return Ok(());
    }
    if dest.exists() {
        return Err(anyhow!(
            "目标已存在，请改名或改版本说明后重试: {}",
            dest.display()
        ));
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::rename(src, dest).map_err(|e| anyhow!("移动失败 {}: {e}", src.display()))?;
    Ok(())
}

/// 规范化单曲：先歌词后歌曲，返回整理后路径。
/// 目标已存在则整项失败，不覆盖。
pub fn normalize_track(library_root: &Path, track: &TrackRow) -> Result<NormalizeResult> {
    let audio_src = PathBuf::from(&track.path);
    if !audio_src.is_file() {
        return Err(anyhow!("歌曲文件不存在: {}", track.path));
    }

    let song_dest = expected_song_path(library_root, track);
    let lrc_dest = expected_lrc_path(library_root, track);
    let current_lrc = if track.has_lrc {
        find_current_lrc(library_root, track)
    } else {
        None
    };

    // 预检目标冲突（歌词目标 / 歌曲目标），避免半途失败
    if let Some(cur) = current_lrc.as_ref() {
        if !path_eq(cur, &lrc_dest) && lrc_dest.exists() {
            return Err(anyhow!(
                "歌词目标已存在，请改名或改版本说明后重试: {}",
                lrc_dest.display()
            ));
        }
    }
    if !path_eq(&audio_src, &song_dest) && song_dest.exists() {
        return Err(anyhow!(
            "目标已存在，请改名或改版本说明后重试: {}",
            song_dest.display()
        ));
    }

    // 1) 歌词（依赖旧音频路径找 sidecar，必须先做）
    let mut lrc_out = None;
    if let Some(cur) = current_lrc {
        if !path_eq(&cur, &lrc_dest) {
            move_file(&cur, &lrc_dest)?;
            lrc_out = Some(lrc_dest.clone());
        } else {
            lrc_out = Some(cur);
        }
    }

    // 2) 歌曲
    let mut song_moved = false;
    if !path_eq(&audio_src, &song_dest) {
        move_file(&audio_src, &song_dest)?;
        // 若歌词仍是 sidecar，跟到新位置（规范化歌词已在步骤 1 挪走时不会进这里）
        move_sidecar_with_audio(&audio_src, &song_dest);
        song_moved = true;
    }

    let message = if song_moved && lrc_out.is_some() {
        format!("歌曲已整理到 {}", song_dest.display())
    } else if song_moved {
        format!("歌曲已整理到 {}", song_dest.display())
    } else if lrc_out.is_some() {
        "歌词已整理".to_string()
    } else {
        "已规范".to_string()
    };

    Ok(NormalizeResult {
        song_path: song_dest.to_string_lossy().into(),
        lrc_path: lrc_out.map(|p| p.to_string_lossy().into()),
        message,
    })
}

/// 只修一条归档意见（kind: song_location | song_name | lyrics_location | lyrics_name）。
/// 位置类：挪到目标目录、**保留当前文件名**；命名类：就地改成期望名。
pub fn normalize_issue(
    library_root: &Path,
    track: &TrackRow,
    kind: &str,
) -> Result<IssueFixResult> {
    match kind {
        "song_location" | "song_name" => {
            let src = PathBuf::from(&track.path);
            if !src.is_file() {
                return Err(anyhow!("歌曲文件不存在: {}", track.path));
            }
            let expected = expected_song_path(library_root, track);
            let dest = if kind == "song_location" {
                // 只改目录，文件名暂不动
                let name = src
                    .file_name()
                    .ok_or_else(|| anyhow!("无法读取文件名"))?;
                expected
                    .parent()
                    .ok_or_else(|| anyhow!("无法解析目标目录"))?
                    .join(name)
            } else {
                // 只改文件名，目录暂不动
                let name = expected
                    .file_name()
                    .ok_or_else(|| anyhow!("无法解析目标文件名"))?;
                src.parent()
                    .ok_or_else(|| anyhow!("无法解析当前目录"))?
                    .join(name)
            };
            move_file(&src, &dest)?;
            // sidecar 歌词跟着走，避免 has_lrc 丢失导致歌词意见被“假修复”
            move_sidecar_with_audio(&src, &dest);
            Ok(IssueFixResult {
                message: if kind == "song_location" {
                    format!("已移到 {}", dest.display())
                } else {
                    format!("已重命名为 {}", dest.display())
                },
                song_path: Some(dest.to_string_lossy().into()),
                lrc_path: None,
            })
        }
        "lyrics_location" | "lyrics_name" => {
            let Some(src) = find_current_lrc(library_root, track) else {
                return Err(anyhow!("找不到外挂歌词文件"));
            };
            let expected = expected_lrc_path(library_root, track);
            let dest = if kind == "lyrics_location" {
                let name = src
                    .file_name()
                    .ok_or_else(|| anyhow!("无法读取歌词文件名"))?;
                paths::library_lrc_dir(library_root).join(name)
            } else {
                let name = expected
                    .file_name()
                    .ok_or_else(|| anyhow!("无法解析目标歌词名"))?;
                src.parent()
                    .ok_or_else(|| anyhow!("无法解析当前目录"))?
                    .join(name)
            };
            move_file(&src, &dest)?;
            Ok(IssueFixResult {
                message: if kind == "lyrics_location" {
                    format!("歌词已移到 {}", dest.display())
                } else {
                    format!("歌词已重命名为 {}", dest.display())
                },
                song_path: None,
                lrc_path: Some(dest.to_string_lossy().into()),
            })
        }
        other => Err(anyhow!("未知归档意见类型: {other}")),
    }
}

// ── 库根杂项扫描 ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryRootItem {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    /// "audio" | "lyrics" | "other_file" | "other_dir"
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryRootScan {
    pub ok: bool,
    pub items: Vec<LibraryRootItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeResult {
    pub moved: Vec<String>,
    pub failed: Vec<String>,
}

fn is_whitelisted_name(name: &str) -> bool {
    if paths::LIBRARY_ROOT_DIRS
        .iter()
        .any(|d| d.eq_ignore_ascii_case(name))
    {
        return true;
    }
    if paths::LIBRARY_ROOT_FILES
        .iter()
        .any(|f| f.eq_ignore_ascii_case(name))
    {
        return true;
    }
    // 同目录临时文件 `.xxx.axtmp-*` 忽略
    if name.starts_with('.') && name.contains(".axtmp-") {
        return true;
    }
    false
}

fn classify_item(path: &Path, is_dir: bool) -> String {
    if is_dir {
        return "other_dir".into();
    }
    let ext = file_ext(path);
    if AUDIO_EXTS.contains(&ext.as_str()) {
        "audio".into()
    } else if ext == "lrc" {
        "lyrics".into()
    } else {
        "other_file".into()
    }
}

/// 扫描库根直接子项，列出白名单外杂项。
pub fn scan_library_root(library_root: &Path) -> Result<LibraryRootScan> {
    let mut items = Vec::new();
    let rd = std::fs::read_dir(library_root)
        .map_err(|e| anyhow!("无法读取库根目录: {e}"))?;
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if is_whitelisted_name(&name) {
            continue;
        }
        let path = entry.path();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        items.push(LibraryRootItem {
            kind: classify_item(&path, is_dir),
            name,
            path: path.to_string_lossy().into(),
            is_dir,
        });
    }
    items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(LibraryRootScan {
        ok: items.is_empty(),
        items,
    })
}

/// 把库根杂项全部挪进 `Unarchived/`。重名项保留并记入 failed。
pub fn organize_library_root(library_root: &Path) -> Result<OrganizeResult> {
    let scan = scan_library_root(library_root)?;
    let dest_dir = paths::library_unarchived_dir(library_root);
    std::fs::create_dir_all(&dest_dir)?;

    let mut moved = Vec::new();
    let mut failed = Vec::new();
    for item in &scan.items {
        let src = PathBuf::from(&item.path);
        let dest = dest_dir.join(&item.name);
        if dest.exists() {
            failed.push(format!("{}（重名）", item.name));
            continue;
        }
        match std::fs::rename(&src, &dest) {
            Ok(()) => moved.push(item.name.clone()),
            Err(e) => failed.push(format!("{}（{e}）", item.name)),
        }
    }
    Ok(OrganizeResult { moved, failed })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_suffix_extract() {
        assert_eq!(extract_version_suffix("晴天(Live)"), Some("(Live)".into()));
        assert_eq!(
            extract_version_suffix("晴天 (2011 Remaster)"),
            Some("(2011 Remaster)".into())
        );
        assert_eq!(extract_version_suffix("晴天"), None);
        assert_eq!(extract_version_suffix("晴天()"), None);
        assert_eq!(extract_version_suffix("晴天(a(b))"), None);
    }

    #[test]
    fn expected_names() {
        assert_eq!(
            expected_song_filename("周杰伦", "晴天", Some("(Live)"), "flac"),
            "周杰伦 - 晴天(Live).flac"
        );
        assert_eq!(
            expected_song_filename("周杰伦", "晴天", None, "MP3"),
            "周杰伦 - 晴天.mp3"
        );
        assert_eq!(expected_stem("", "", None), "Unknown Artist - Unknown");
    }

    #[test]
    fn root_whitelist() {
        assert!(is_whitelisted_name("archived"));
        assert!(is_whitelisted_name("Unarchived"));
        assert!(is_whitelisted_name("data"));
        assert!(is_whitelisted_name("axmusic.db-wal"));
        assert!(is_whitelisted_name("axmusic.db-shm"));
        assert!(is_whitelisted_name("axmusic.db-journal"));
        assert!(is_whitelisted_name(".x.axtmp-1-2-3"));
        assert!(!is_whitelisted_name("晴天.flac"));
        assert!(!is_whitelisted_name("misc"));
    }

    #[test]
    fn song_location_fix_keeps_lyrics_issue() {
        let dir = std::env::temp_dir().join(format!("axmusic-arch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Unarchived")).unwrap();
        std::fs::create_dir_all(dir.join("lrc")).unwrap();
        std::fs::create_dir_all(dir.join("archived")).unwrap();

        let audio = dir.join("Unarchived").join("foo.flac");
        let lrc = dir.join("Unarchived").join("foo.lrc");
        std::fs::write(&audio, b"a").unwrap();
        std::fs::write(&lrc, b"[00:00.00]hi").unwrap();

        let mut track = TrackRow {
            id: 1,
            path: audio.to_string_lossy().into(),
            filename: "foo.flac".into(),
            title: "晴天".into(),
            artist: "周杰伦".into(),
            album: "叶惠美".into(),
            album_artist: "周杰伦".into(),
            year: "2003".into(),
            track_no: Some(1),
            disc_no: None,
            duration_ms: 1000,
            format: "flac".into(),
            sample_rate: None,
            bit_rate: None,
            has_cover: false,
            has_lyrics: false,
            has_lrc: true,
            has_year: true,
            has_mb_id: false,
            tag_status: "partial".into(),
            missing: String::new(),
            release_type: String::new(),
            mb_recording_mbid: String::new(),
            mb_release_mbid: String::new(),
            catalog_id: Some(1),
            mtime: 0,
            file_size: 1,
            catalog_title: None,
            catalog_artist: None,
            catalog_album: None,
            catalog_year: None,
            catalog_track_no: None,
        };

        let before = check_track(&dir, &track);
        assert!(before.issues.iter().any(|i| i.kind == "song_location"));
        assert!(before.issues.iter().any(|i| i.kind == "lyrics_location"));

        // 只修歌曲位置
        let fixed = normalize_issue(&dir, &track, "song_location").unwrap();
        let new_song = fixed.song_path.unwrap();
        track.path = new_song.clone();
        track.filename = "foo.flac".into();
        // sidecar 应跟到新目录，歌词意见仍在
        assert!(Path::new(&new_song).with_extension("lrc").is_file());
        let after = check_track(&dir, &track);
        assert!(
            !after.issues.iter().any(|i| i.kind == "song_location"),
            "歌曲位置应已修复: {:?}",
            after.issues
        );
        assert!(
            after.issues.iter().any(|i| i.kind == "lyrics_location"),
            "歌词位置不应被顺手修掉: {:?}",
            after.issues
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
