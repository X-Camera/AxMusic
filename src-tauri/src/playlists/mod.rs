//! m3u8 歌单读写（`<库根>/playlists/*.m3u8`，条目存相对路径、正斜杠）。
//! 播放不依赖 SQLite；DB 仅用于展示富化。

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::library::{LibraryDb, TrackRow};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistSummary {
    /// 文件名（不含 .m3u8）
    pub name: String,
    /// 条目总数（含失效项）
    pub track_count: usize,
    /// EXTINF 时长之和（未知计 0）
    pub total_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistAddItem {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistEntry {
    /// 解析后的绝对路径
    pub path: String,
    /// 文件里存的那一行（原样回写用）
    pub rel_path: String,
    /// EXTINF 逗号右侧；缺省用文件名
    pub title: String,
    /// 从 "artist - title" 尽力拆出，否则 ""
    pub artist: String,
    pub duration_ms: u64,
    /// 当前磁盘上是否存在
    pub exists: bool,
    /// DB 可用时按路径富化，仅展示用
    pub track: Option<TrackRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistDetail {
    pub name: String,
    pub entries: Vec<PlaylistEntry>,
}

/// 解析中间态（文件一行 + 其前的 EXTINF）
#[derive(Debug, Clone)]
pub struct ParsedEntry {
    pub rel: String,
    pub display: String,
    pub duration_ms: u64,
}

// ── 路径与名称 ─────────────────────────────────────────────────────

pub fn playlists_dir(root: &Path) -> PathBuf {
    root.join("playlists")
}

/// 校验歌单名；自动剥掉多余的 `.m3u8` 后缀。非法返回「歌单名不合法」。
pub fn validate_name(name: &str) -> Result<String, String> {
    let mut n = name.trim();
    // 仅当确为 ASCII 后缀时才按字节切（避免多字节名切在字符中间）
    if n.to_ascii_lowercase().ends_with(".m3u8") {
        n = n[..n.len() - 5].trim();
    }
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let bad = n.is_empty()
        || n.len() > 100
        || n == "."
        || n == ".."
        || n.ends_with(' ')
        || n.ends_with('.')
        || n.chars()
            .any(|c| "\\/:*?\"<>|".contains(c) || c.is_control())
        || RESERVED.iter().any(|r| n.eq_ignore_ascii_case(r));
    if bad {
        return Err("歌单名不合法".into());
    }
    Ok(n.to_string())
}

fn file_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    Ok(playlists_dir(root).join(format!("{}.m3u8", validate_name(name)?)))
}

/// 磁盘上是否已有同名歌单（Windows 文件名大小写不敏感）。
fn exists_ci(root: &Path, name: &str) -> bool {
    let dir = playlists_dir(root);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return false;
    };
    rd.flatten().any(|e| {
        let p = e.path();
        let m3u8 = p
            .extension()
            .map(|x| x.eq_ignore_ascii_case("m3u8"))
            .unwrap_or(false);
        m3u8
            && p.file_stem()
                .map(|s| s.to_string_lossy().eq_ignore_ascii_case(name))
                .unwrap_or(false)
    })
}

fn to_slashes(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// 词法归一化（折叠 `.`/`..`，不碰文件系统）。
fn normalize_lexical(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// base_dir → abs 的相对路径（**正斜杠**）；跨盘符等无公共前缀时回退绝对路径（也转正斜杠）。
pub fn rel_from(base_dir: &Path, abs: &Path) -> String {
    let base = normalize_lexical(base_dir);
    let target = normalize_lexical(abs);
    let base_comps: Vec<Component> = base.components().collect();
    let target_comps: Vec<Component> = target.components().collect();
    let eq = |a: &Component, b: &Component| {
        let (a, b) = (a.as_os_str().to_string_lossy(), b.as_os_str().to_string_lossy());
        if cfg!(windows) {
            a.eq_ignore_ascii_case(&b)
        } else {
            a == b
        }
    };
    let mut i = 0;
    while i < base_comps.len() && i < target_comps.len() && eq(&base_comps[i], &target_comps[i]) {
        i += 1;
    }
    if i == 0 {
        return to_slashes(&target);
    }
    let mut rel = PathBuf::new();
    for _ in i..base_comps.len() {
        rel.push("..");
    }
    for c in &target_comps[i..] {
        rel.push(c.as_os_str());
    }
    if rel.as_os_str().is_empty() {
        return to_slashes(&target);
    }
    to_slashes(&rel)
}

/// m3u8 行 → 绝对路径（接受 `/` 与 `\`、相对与绝对）。
pub fn abs_from(m3u8_dir: &Path, line: &str) -> PathBuf {
    let cleaned = line.trim().trim_matches('"');
    let p = Path::new(cleaned);
    if p.is_absolute() {
        normalize_lexical(p)
    } else {
        normalize_lexical(&m3u8_dir.join(p))
    }
}

// ── EXTINF ─────────────────────────────────────────────────────────

/// artist 空 → title，否则 `artist - title`。
fn extinf_display(artist: &str, title: &str) -> String {
    if artist.trim().is_empty() {
        title.to_string()
    } else {
        format!("{} - {}", artist.trim(), title.trim())
    }
}

/// 首个 ` - ` 拆 (artist, title)；拆不出 → ("", display)。仅展示兜底。
fn extinf_parse(display: &str) -> (String, String) {
    match display.split_once(" - ") {
        Some((a, t)) if !a.trim().is_empty() && !t.trim().is_empty() => {
            (a.trim().to_string(), t.trim().to_string())
        }
        _ => (String::new(), display.to_string()),
    }
}

// ── m3u8 读写 ──────────────────────────────────────────────────────

/// 容错解析：空行跳过；`#EXTINF:` 记 pending；其它 `#` 忽略；非注释行 = 路径。
pub fn parse_m3u8(text: &str) -> Vec<ParsedEntry> {
    let mut out = Vec::new();
    let mut pending: Option<(u64, String)> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let (dur_s, display) = match rest.split_once(',') {
                Some((d, disp)) => (d.trim(), disp.trim().to_string()),
                None => (rest.trim(), rest.trim().to_string()),
            };
            let secs: f64 = dur_s.parse().unwrap_or(-1.0);
            let duration_ms = if secs > 0.0 { (secs * 1000.0) as u64 } else { 0 };
            pending = Some((duration_ms, display));
        } else if line.starts_with('#') {
            continue;
        } else {
            let (duration_ms, display) = match pending.take() {
                Some(x) => x,
                None => {
                    let stem = Path::new(line)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| line.to_string());
                    (0, stem)
                }
            };
            out.push(ParsedEntry {
                rel: line.to_string(),
                display,
                duration_ms,
            });
        }
    }
    out
}

pub fn read_playlist(root: &Path, name: &str) -> Result<Vec<ParsedEntry>, String> {
    let path = file_path(root, name)?;
    let text = std::fs::read_to_string(&path).map_err(|_| "歌单不存在".to_string())?;
    Ok(parse_m3u8(&text))
}

/// 规范写出（`#EXTM3U` + `#EXTINF` + 相对路径行），`.tmp` + rename 原子替换。
fn write_playlist(root: &Path, name: &str, entries: &[ParsedEntry]) -> Result<(), String> {
    let path = file_path(root, name)?;
    let dir = playlists_dir(root);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建歌单目录失败：{e}"))?;
    let mut text = String::from("#EXTM3U\n");
    for e in entries {
        let secs = if e.duration_ms == 0 {
            -1i64
        } else {
            (e.duration_ms / 1000) as i64
        };
        text.push_str(&format!("#EXTINF:{},{}\n{}\n", secs, e.display, e.rel));
    }
    let tmp = path.with_extension("m3u8.tmp");
    std::fs::write(&tmp, &text).map_err(|e| format!("写歌单失败：{e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("写歌单失败：{e}"))?;
    Ok(())
}

pub fn list_playlists(root: &Path) -> Result<Vec<PlaylistSummary>, String> {
    let dir = playlists_dir(root);
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if !p
                .extension()
                .map(|x| x.eq_ignore_ascii_case("m3u8"))
                .unwrap_or(false)
            {
                continue;
            }
            let Some(name) = p.file_stem().map(|s| s.to_string_lossy().to_string()) else {
                continue;
            };
            let parsed = read_playlist(root, &name)?;
            out.push(PlaylistSummary {
                track_count: parsed.len(),
                total_ms: parsed.iter().map(|x| x.duration_ms).sum(),
                name,
            });
        }
    }
    out.sort_by_key(|s| s.name.to_lowercase());
    Ok(out)
}

fn item_to_entry(root: &Path, item: &PlaylistAddItem) -> ParsedEntry {
    let abs = PathBuf::from(&item.path);
    let rel = rel_from(&playlists_dir(root), &abs);
    let title = if item.title.trim().is_empty() {
        abs.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| item.path.clone())
    } else {
        item.title.clone()
    };
    ParsedEntry {
        rel,
        display: extinf_display(&item.artist, &title),
        duration_ms: item.duration_ms,
    }
}

/// parsed → 带绝对路径/存在性/可选 DB 富化的展示结构。
pub fn to_entries(
    root: &Path,
    name: &str,
    parsed: &[ParsedEntry],
    db: Option<&LibraryDb>,
) -> PlaylistDetail {
    let dir = playlists_dir(root);
    let entries = parsed
        .iter()
        .map(|e| {
            let abs = abs_from(&dir, &e.rel);
            let (artist, title) = extinf_parse(&e.display);
            let track = db.and_then(|d| d.get_track_by_path(&abs.to_string_lossy()).ok().flatten());
            PlaylistEntry {
                path: abs.to_string_lossy().to_string(),
                rel_path: e.rel.clone(),
                title,
                artist,
                duration_ms: e.duration_ms,
                exists: abs.is_file(),
                track,
            }
        })
        .collect();
    PlaylistDetail {
        name: name.to_string(),
        entries,
    }
}

// ── 变更操作（read-parse-rewrite）────────────────────────────────────

pub fn create(root: &Path, name: &str, items: &[PlaylistAddItem]) -> Result<(), String> {
    let name = validate_name(name)?;
    if exists_ci(root, &name) {
        return Err(format!("歌单已存在：{name}"));
    }
    let entries: Vec<ParsedEntry> = items.iter().map(|i| item_to_entry(root, i)).collect();
    write_playlist(root, &name, &entries)
}

pub fn rename(root: &Path, name: &str, new_name: &str) -> Result<(), String> {
    let name = validate_name(name)?;
    let new_name = validate_name(new_name)?;
    if exists_ci(root, &new_name) {
        return Err(format!("歌单已存在：{new_name}"));
    }
    let from = file_path(root, &name)?;
    if !from.is_file() {
        return Err("歌单不存在".into());
    }
    let to = file_path(root, &new_name)?;
    std::fs::rename(&from, &to).map_err(|e| format!("重命名失败：{e}"))
}

pub fn delete(root: &Path, name: &str) -> Result<(), String> {
    let path = file_path(root, name)?;
    if !path.is_file() {
        return Err("歌单不存在".into());
    }
    std::fs::remove_file(&path).map_err(|e| format!("删除失败：{e}"))
}

/// 追加条目；按归一化绝对路径去重（Windows 不区分大小写），已在的跳过。
pub fn add_tracks(root: &Path, name: &str, items: &[PlaylistAddItem]) -> Result<(), String> {
    let mut entries = read_playlist(root, name)?;
    let dir = playlists_dir(root);
    let mut known: Vec<String> = entries
        .iter()
        .map(|e| abs_from(&dir, &e.rel).to_string_lossy().to_lowercase())
        .collect();
    for item in items {
        let entry = item_to_entry(root, item);
        let key = abs_from(&dir, &entry.rel).to_string_lossy().to_lowercase();
        if known.contains(&key) {
            continue;
        }
        known.push(key);
        entries.push(entry);
    }
    write_playlist(root, name, &entries)
}

pub fn remove_track(root: &Path, name: &str, index: usize) -> Result<(), String> {
    let mut entries = read_playlist(root, name)?;
    if index >= entries.len() {
        return Err("位置无效".into());
    }
    entries.remove(index);
    write_playlist(root, name, &entries)
}

pub fn move_track(root: &Path, name: &str, from_index: usize, to_index: usize) -> Result<(), String> {
    let mut entries = read_playlist(root, name)?;
    if from_index >= entries.len() || to_index >= entries.len() {
        return Err("位置无效".into());
    }
    if from_index == to_index {
        return Ok(());
    }
    let e = entries.remove(from_index);
    entries.insert(to_index, e);
    write_playlist(root, name, &entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bare_paths() {
        let entries = parse_m3u8("a/b.flac\r\n\r\nc\\d.mp3\n");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].rel, "a/b.flac");
        assert_eq!(entries[0].display, "b");
        assert_eq!(entries[1].rel, "c\\d.mp3");
    }

    #[test]
    fn parse_extinf() {
        let entries = parse_m3u8("#EXTM3U\n#EXTINF:245,周杰伦 - 晴天\n../x.flac\n#EXTINF:-1,\n../y.flac\n");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].duration_ms, 245_000);
        assert_eq!(entries[0].display, "周杰伦 - 晴天");
        assert_eq!(entries[1].duration_ms, 0);
    }

    #[test]
    fn extinf_artist_split() {
        assert_eq!(extinf_parse("A - B"), ("A".into(), "B".into()));
        assert_eq!(extinf_parse("no-split"), ("".into(), "no-split".into()));
    }

    #[test]
    fn rel_same_drive_goes_up() {
        let base = Path::new("C:\\music\\playlists");
        let abs = Path::new("C:\\music\\a\\b.flac");
        assert_eq!(rel_from(base, abs), "../a/b.flac");
    }

    #[test]
    fn rel_cross_drive_falls_back_absolute() {
        let base = Path::new("C:\\music\\playlists");
        let abs = Path::new("E:\\x\\y.flac");
        assert_eq!(rel_from(base, abs), "E:/x/y.flac");
    }

    #[test]
    fn abs_accepts_both_separators() {
        let dir = Path::new("C:\\music\\playlists");
        assert_eq!(abs_from(dir, "../a\\b.flac"), PathBuf::from("C:\\music\\a\\b.flac"));
    }

    #[test]
    fn name_rules() {
        assert!(validate_name(" 我的歌单 ").is_ok());
        assert!(validate_name("abc.m3u8").unwrap() == "abc");
        assert!(validate_name("").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("CON").is_err());
        assert!(validate_name("x.").is_err());
    }
}
