//! m3u8 歌单读写（`<库根>/playlists/*.m3u8`，条目存相对路径、正斜杠）。
//! 播放不依赖 SQLite；DB 仅用于展示富化。

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::library::{LibraryDb, TrackRow};

/// 系统「喜爱」歌单的磁盘文件名（不含 .m3u8），不可重命名/删除。
pub const FAVORITES_FILE: &str = "__favorites__";
/// 「喜爱」在界面上的固定展示名。
pub const FAVORITES_LABEL: &str = "喜爱";

/// 是否为系统「喜爱」歌单（接受内部名 / 展示名）。
pub fn is_favorites_id(name: &str) -> bool {
    let n = name.trim();
    n.eq_ignore_ascii_case(FAVORITES_FILE) || n == FAVORITES_LABEL || n.eq_ignore_ascii_case("favorites")
}

/// 把「喜爱」的各种叫法归一到内部文件名；其余走 validate_name。
fn resolve_name(name: &str) -> Result<String, String> {
    if is_favorites_id(name) {
        return Ok(FAVORITES_FILE.to_string());
    }
    validate_name(name)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistSummary {
    /// 文件名（不含 .m3u8）；喜爱固定为「喜爱」
    pub name: String,
    /// 条目总数（含失效项）
    pub track_count: usize,
    /// EXTINF 时长之和（未知计 0）
    pub total_ms: u64,
    /// 系统「喜爱」歌单（不可重命名/删除）
    #[serde(default)]
    pub is_favorites: bool,
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
    /// 喜爱固定为「喜爱」
    pub name: String,
    pub entries: Vec<PlaylistEntry>,
    /// 系统「喜爱」歌单
    #[serde(default)]
    pub is_favorites: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteToggleResult {
    /// 切换后是否已喜爱
    pub favorited: bool,
    /// 喜爱歌单当前条目数
    pub track_count: usize,
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
    root.join(crate::paths::PLAYLISTS_DIR_NAME)
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
        || RESERVED.iter().any(|r| n.eq_ignore_ascii_case(r))
        || is_favorites_id(n);
    if bad {
        return Err(if is_favorites_id(n) {
            "「喜爱」是系统歌单名，不可占用".into()
        } else {
            "歌单名不合法".into()
        });
    }
    Ok(n.to_string())
}

fn file_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    Ok(playlists_dir(root).join(format!("{}.m3u8", resolve_name(name)?)))
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
    // 区分「不存在」与真 IO 故障（占用等）：折叠成不存在会让调用方按空歌单整写回；
    // 编码上容错：非 UTF-8（GBK/ANSI）按 GB18030 回退解码，不再整条目丢失
    let text = crate::paths::read_text_lossy(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "歌单不存在".to_string()
        } else {
            format!("读取歌单失败：{e}")
        }
    })?;
    Ok(parse_m3u8(&text))
}

/// 读取喜爱歌单；文件不存在时视为空（喜爱始终可写），读取故障如实报错。
pub fn read_favorites(root: &Path) -> Result<Vec<ParsedEntry>, String> {
    let path = file_path(root, FAVORITES_FILE)?;
    match crate::paths::read_text_lossy(&path) {
        Ok(text) => Ok(parse_m3u8(&text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("读取喜爱歌单失败：{e}")),
    }
}

/// 展示名：喜爱 →「喜爱」，其余为文件名。
pub fn display_name(name: &str) -> String {
    if is_favorites_id(name) {
        FAVORITES_LABEL.to_string()
    } else {
        name.to_string()
    }
}

/// 规范写出（`#EXTM3U` + `#EXTINF` + 相对路径行），同目录唯一临时文件 + rename 原子替换。
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
    crate::paths::write_atomic(&path, text.as_bytes()).map_err(|e| format!("写歌单失败：{e}"))?;
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
            // 系统喜爱单独置顶注入，不进普通列表
            if is_favorites_id(&name) {
                continue;
            }
            // 单首读取失败（占用/编码）跳过并记录，不拖垮整个列表；文件仍在磁盘上
            let parsed = match read_playlist(root, &name) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("[AxMusic] 跳过无法读取的歌单 {name}: {e}");
                    continue;
                }
            };
            out.push(PlaylistSummary {
                track_count: parsed.len(),
                total_ms: parsed.iter().map(|x| x.duration_ms).sum(),
                name,
                is_favorites: false,
            });
        }
    }
    out.sort_by_key(|s| s.name.to_lowercase());
    let fav = read_favorites(root)?;
    out.insert(
        0,
        PlaylistSummary {
            name: FAVORITES_LABEL.to_string(),
            track_count: fav.len(),
            total_ms: fav.iter().map(|x| x.duration_ms).sum(),
            is_favorites: true,
        },
    );
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
    let is_favorites = is_favorites_id(name);
    PlaylistDetail {
        name: display_name(name),
        entries,
        is_favorites,
    }
}

// ── 变更操作（read-parse-rewrite）────────────────────────────────────

pub fn create(root: &Path, name: &str, items: &[PlaylistAddItem]) -> Result<(), String> {
    if is_favorites_id(name) {
        return Err("「喜爱」是系统歌单，不可新建覆盖".into());
    }
    let name = validate_name(name)?;
    if exists_ci(root, &name) {
        return Err(format!("歌单已存在：{name}"));
    }
    let entries: Vec<ParsedEntry> = items.iter().map(|i| item_to_entry(root, i)).collect();
    write_playlist(root, &name, &entries)
}

pub fn rename(root: &Path, name: &str, new_name: &str) -> Result<(), String> {
    if is_favorites_id(name) || is_favorites_id(new_name) {
        return Err("「喜爱」是系统歌单，不可重命名".into());
    }
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
    if is_favorites_id(name) {
        return Err("「喜爱」是系统歌单，不可删除".into());
    }
    let path = file_path(root, name)?;
    if !path.is_file() {
        return Err("歌单不存在".into());
    }
    std::fs::remove_file(&path).map_err(|e| format!("删除失败：{e}"))
}

/// 变更前读取：喜爱文件可不存在（视为空），普通歌单缺失报错；读取故障如实上抛，不按空歌单整写回。
fn read_for_update(root: &Path, name: &str) -> Result<Vec<ParsedEntry>, String> {
    if is_favorites_id(name) {
        read_favorites(root)
    } else {
        read_playlist(root, name)
    }
}

/// 追加条目；按归一化绝对路径去重（Windows 不区分大小写），已在的跳过。
pub fn add_tracks(root: &Path, name: &str, items: &[PlaylistAddItem]) -> Result<(), String> {
    let mut entries = read_for_update(root, name)?;
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

/// 按条目定位移除（`rel_path` 即文件里存的那一行，界面持有的稳定标识）。
/// 不用下标：连续点「移除」时两次调用看到的列表可能不同步，下标会移错行。
pub fn remove_entry(root: &Path, name: &str, rel_path: &str) -> Result<(), String> {
    let mut entries = read_for_update(root, name)?;
    let before = entries.len();
    entries.retain(|e| e.rel != rel_path);
    if entries.len() == before {
        return Err("条目不存在（可能已被移除）".into());
    }
    write_playlist(root, name, &entries)
}

/// 按条目定位移动一格（delta = ±1）；目标越界则不动。同理不拿下标。
pub fn move_entry(root: &Path, name: &str, rel_path: &str, delta: i64) -> Result<(), String> {
    let mut entries = read_for_update(root, name)?;
    let Some(idx) = entries.iter().position(|e| e.rel == rel_path) else {
        return Err("条目不存在（可能已被移除）".into());
    };
    let to = idx as i64 + delta;
    if to < 0 || to >= entries.len() as i64 {
        return Ok(());
    }
    entries.swap(idx, to as usize);
    write_playlist(root, name, &entries)
}

/// 批量清理失效条目（磁盘上已不存在），一次读写完事；返回清掉条数。
/// 先库内兜底重匹配：能找回的改写路径保留，找不回的才删。
pub fn clean_missing(root: &Path, name: &str, db: Option<&LibraryDb>) -> Result<usize, String> {
    let dir = playlists_dir(root);
    let mut entries = read_for_update(root, name)?;
    let (healed, _) = rematch_missing_entries(root, &mut entries, db);
    let before = entries.len();
    entries.retain(|e| abs_from(&dir, &e.rel).is_file());
    let removed = before - entries.len();
    if removed > 0 || healed > 0 {
        write_playlist(root, name, &entries)?;
    }
    Ok(removed)
}

/// 喜爱歌单里各条目的绝对路径（原样）。
/// 先做库内兜底重匹配并写回自愈，心形状态才能跟归档/移动后的新路径对齐。
pub fn favorite_paths(root: &Path, db: Option<&LibraryDb>) -> Result<Vec<String>, String> {
    let mut entries = read_favorites(root)?;
    let (healed, _) = rematch_missing_entries(root, &mut entries, db);
    if healed > 0 {
        write_playlist(root, FAVORITES_FILE, &entries)?;
    }
    let dir = playlists_dir(root);
    Ok(entries
        .iter()
        .map(|e| abs_from(&dir, &e.rel).to_string_lossy().to_string())
        .collect())
}

/// 失效条目库内兜底重匹配：命中现存曲目则就地改写 `rel` 为新路径。
/// 返回 (自愈条数, 仍缺失条数)。无库时只统计缺失、不改动。
/// 归档/移动后路径变了，靠 EXTINF 的歌手/歌名、时长、原文件名找回；匹配不到才算真失效。
pub fn rematch_missing_entries(
    root: &Path,
    entries: &mut [ParsedEntry],
    db: Option<&LibraryDb>,
) -> (usize, usize) {
    let Some(db) = db else {
        let dir = playlists_dir(root);
        let missing = entries
            .iter()
            .filter(|e| !abs_from(&dir, &e.rel).is_file())
            .count();
        return (0, missing);
    };
    let dir = playlists_dir(root);
    let mut healed = 0usize;
    let mut missing = 0usize;
    for e in entries.iter_mut() {
        let abs = abs_from(&dir, &e.rel);
        if abs.is_file() {
            continue;
        }
        let (artist, title) = extinf_parse(&e.display);
        let hit = db
            .find_track_for_playlist_rematch(
                &abs.to_string_lossy(),
                &artist,
                &title,
                e.duration_ms,
            )
            .ok()
            .flatten()
            .filter(|t| Path::new(&t.path).is_file());
        match hit {
            Some(t) => {
                e.rel = rel_from(&dir, Path::new(&t.path));
                healed += 1;
            }
            None => missing += 1,
        }
    }
    (healed, missing)
}

/// 读歌单/喜爱 → 兜底重匹配（命中则写回自愈）→ 展示结构。返回 (详情, 自愈条数)。
pub fn detail_with_rematch(
    root: &Path,
    name: &str,
    db: Option<&LibraryDb>,
) -> Result<(PlaylistDetail, usize), String> {
    let mut entries = if is_favorites_id(name) {
        read_favorites(root)?
    } else {
        read_playlist(root, name)?
    };
    let (healed, _) = rematch_missing_entries(root, &mut entries, db);
    if healed > 0 {
        write_playlist(root, name, &entries)?;
    }
    Ok((to_entries(root, name, &entries, db), healed))
}

fn path_key(root: &Path, abs: &Path) -> String {
    abs_from(&playlists_dir(root), &rel_from(&playlists_dir(root), abs))
        .to_string_lossy()
        .to_lowercase()
}

/// 切换喜爱：已存在则移除，否则追加。返回切换后状态与条目数。
pub fn favorite_toggle(root: &Path, item: &PlaylistAddItem) -> Result<FavoriteToggleResult, String> {
    let mut entries = read_favorites(root)?;
    let abs = PathBuf::from(&item.path);
    let key = path_key(root, &abs);
    let dir = playlists_dir(root);
    let before = entries.len();
    entries.retain(|e| path_key(root, &abs_from(&dir, &e.rel)) != key);
    let was_present = entries.len() < before;
    if !was_present {
        entries.push(item_to_entry(root, item));
    }
    write_playlist(root, FAVORITES_FILE, &entries)?;
    Ok(FavoriteToggleResult {
        favorited: !was_present,
        track_count: entries.len(),
    })
}

/// 归档移动后，把所有歌单（含喜爱）里的旧绝对路径条目改写为新路径。
/// 返回改写条数。路径比较忽略大小写（Windows）。
/// 按磁盘文件路径读写，不经名称校验/别名归一，避免 `favorites.m3u8` 等被重定向漏改。
pub fn rewrite_path_in_playlists(root: &Path, old_abs: &Path, new_abs: &Path) -> Result<usize, String> {
    let dir = playlists_dir(root);
    let old_key = path_key(root, old_abs);
    let new_rel = rel_from(&dir, new_abs);

    let files = list_playlist_files(root)?;
    let mut total = 0usize;
    let mut failures = Vec::new();
    for path in files {
        match rewrite_one_playlist_file(&dir, &path, &old_key, &new_rel) {
            Ok(n) => total += n,
            Err(e) => {
                failures.push(format!("{}（{e}）", path.display()));
            }
        }
    }
    if !failures.is_empty() {
        return Err(format!(
            "已改写 {total} 条；{} 个歌单失败：{}",
            failures.len(),
            failures.join("、")
        ));
    }
    Ok(total)
}

/// 改写单个歌单文件（按完整路径读写，不经过 resolve_name）。
fn rewrite_one_playlist_file(
    dir: &Path,
    path: &Path,
    old_key: &str,
    new_rel: &str,
) -> Result<usize, String> {
    let text = crate::paths::read_text_lossy(path).map_err(|e| format!("读取失败：{e}"))?;
    let mut entries = parse_m3u8(&text);
    let mut n = 0usize;
    let mut changed = false;
    for e in entries.iter_mut() {
        if path_key_of(dir, &abs_from(dir, &e.rel)) == old_key {
            e.rel = new_rel.to_string();
            changed = true;
            n += 1;
        }
    }
    if changed {
        write_playlist_at(path, &entries)?;
    }
    Ok(n)
}

/// 按完整路径原子写出 m3u8（跳过名称校验）。
fn write_playlist_at(path: &Path, entries: &[ParsedEntry]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建歌单目录失败：{e}"))?;
    }
    let mut text = String::from("#EXTM3U\n");
    for e in entries {
        let secs = if e.duration_ms == 0 {
            -1i64
        } else {
            (e.duration_ms / 1000) as i64
        };
        text.push_str(&format!("#EXTINF:{},{}\n{}\n", secs, e.display, e.rel));
    }
    crate::paths::write_atomic(path, text.as_bytes()).map_err(|e| format!("写歌单失败：{e}"))
}

/// path_key 的目录参数化版本（rewrite 时用已解析的 playlists 目录）。
fn path_key_of(dir: &Path, abs: &Path) -> String {
    abs_from(dir, &rel_from(dir, abs)).to_string_lossy().to_lowercase()
}

/// 磁盘上全部歌单文件绝对路径（含喜爱）。目录不存在视为空；其它 IO 错误上抛。
fn list_playlist_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let dir = playlists_dir(root);
    let rd = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("读取歌单目录失败：{e}")),
    };
    let mut files = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.extension()
            .map(|x| x.eq_ignore_ascii_case("m3u8"))
            .unwrap_or(false)
        {
            files.push(p);
        }
    }
    Ok(files)
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

    #[test]
    fn favorites_reserved() {
        assert!(is_favorites_id("喜爱"));
        assert!(is_favorites_id("__favorites__"));
        assert!(is_favorites_id("__FAVORITES__"));
        assert!(!is_favorites_id("我的最爱"));
        assert!(validate_name("喜爱").is_err());
        assert!(validate_name("__favorites__").is_err());
        assert_eq!(resolve_name("喜爱").unwrap(), FAVORITES_FILE);
        assert_eq!(display_name("喜爱"), FAVORITES_LABEL);
        assert_eq!(display_name("跑步"), "跑步");
    }

    #[test]
    fn rematch_missing_heals_playlist_paths() {
        use crate::library::{LibraryDb, TrackRow};

        let root = std::env::temp_dir().join(format!("axmusic-pl-rematch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(playlists_dir(&root)).unwrap();
        let db = LibraryDb::open(&root.join("axmusic.db")).unwrap();

        // 归档后的新文件 + DB 行
        let new_path = root
            .join("archived")
            .join("周杰伦")
            .join("周杰伦 - 晴天.flac");
        std::fs::create_dir_all(new_path.parent().unwrap()).unwrap();
        std::fs::write(&new_path, b"x").unwrap();
        let row = TrackRow {
            id: 0,
            path: new_path.to_string_lossy().to_string(),
            filename: "周杰伦 - 晴天.flac".into(),
            title: "晴天".into(),
            artist: "周杰伦".into(),
            album: "叶惠美".into(),
            album_artist: "周杰伦".into(),
            year: "2003".into(),
            track_no: Some(1),
            disc_no: None,
            duration_ms: 269_000,
            format: "flac".into(),
            sample_rate: Some(44100),
            bit_rate: Some(900),
            has_cover: false,
            has_lyrics: false,
            has_lrc: false,
            has_year: true,
            has_mb_id: false,
            tag_status: String::new(),
            missing: String::new(),
            release_type: String::new(),
            mb_recording_mbid: String::new(),
            mb_release_mbid: String::new(),
            catalog_id: None,
            mtime: 0,
            file_size: 1,
            catalog_title: None,
            catalog_artist: None,
            catalog_album: None,
            catalog_year: None,
            catalog_track_no: None,
        };
        db.upsert_track(&row, 1000, 1).unwrap();

        // 喜爱里存的是归档前的旧路径
        write_playlist(
            &root,
            FAVORITES_FILE,
            &[ParsedEntry {
                rel: "../Unarchived/晴天.flac".into(),
                display: "周杰伦 - 晴天".into(),
                duration_ms: 269_000,
            }],
        )
        .unwrap();

        let (detail, healed) = detail_with_rematch(&root, FAVORITES_FILE, Some(&db)).unwrap();
        assert_eq!(healed, 1, "应自愈 1 条");
        assert!(detail.entries[0].exists, "自愈后不应标缺失");
        assert_eq!(detail.entries[0].path, new_path.to_string_lossy());

        // 磁盘歌单已写回新路径
        let paths = favorite_paths(&root, Some(&db)).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], new_path.to_string_lossy());

        let _ = std::fs::remove_dir_all(&root);
    }
}
