//! 导入其他 AxMusic 库：识别源库 → 对比 → 选择性合并 catalog / 歌曲 / 歌词 / 封面 / 歌单 / 听歌史。
//!
//! 铁律对齐主库：
//! - 刮削结果只进 catalog（不改音频文件）
//! - 封面落 `<库>/covers/`，歌词落 `<库>/lrc/`，歌单落 `<库>/playlists/`
//! - 歌曲复制保持源库相对路径，保证歌单相对路径可继续解析
//! - 听歌史在 `data_root/listen_history.db`（不在库根），导入时在源路径附近探测

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::library::{CatalogRow, LibraryDb, TrackRow};
use crate::listen_history::{ListenDb, ListenEvent};
use crate::paths;

// ── IPC 类型 ────────────────────────────────────────────────────────

/// 一类可导入内容的对比数字。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportItemStats {
    /// 源库条目总数
    pub source_total: i64,
    /// 与当前库重复（导入时跳过）
    pub duplicate: i64,
    /// 将新增
    pub new: i64,
    /// 源文件缺失，无法复制（仅「歌曲」会非 0）
    pub missing: i64,
}

impl ImportItemStats {
    fn empty() -> Self {
        Self {
            source_total: 0,
            duplicate: 0,
            new: 0,
            missing: 0,
        }
    }
}

/// 导入预览：源库识别结果 + 与当前库的对比。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportPreview {
    pub source_root: String,
    pub current_root: String,
    /// 已刮削数据库（catalog）
    pub catalog: ImportItemStats,
    /// 歌曲（音频文件 + tracks 记录）
    pub songs: ImportItemStats,
    /// 外挂歌词（lrc/）
    pub lyrics: ImportItemStats,
    /// 封面（covers/）
    pub covers: ImportItemStats,
    /// 歌单（playlists/*.m3u8）
    pub playlists: ImportItemStats,
    /// 听歌记录（源侧 listen_history.db；找不到则 source_total=0）
    #[serde(default)]
    pub listen: ImportItemStats,
    /// 源侧听歌库路径（展示用；None = 未找到）
    #[serde(default)]
    pub listen_path: Option<String>,
}

/// 用户勾选的导入范围（默认全不选）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportSelection {
    #[serde(default)]
    pub catalog: bool,
    #[serde(default)]
    pub songs: bool,
    #[serde(default)]
    pub lyrics: bool,
    #[serde(default)]
    pub covers: bool,
    #[serde(default)]
    pub playlists: bool,
    #[serde(default)]
    pub listen: bool,
}

/// 导入执行结果（计数 + 非致命错误）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportResult {
    pub catalog_added: i64,
    pub catalog_skipped: i64,
    pub songs_added: i64,
    pub songs_skipped: i64,
    pub songs_failed: i64,
    pub lyrics_added: i64,
    pub lyrics_skipped: i64,
    pub covers_added: i64,
    pub covers_skipped: i64,
    pub playlists_added: i64,
    pub playlists_skipped: i64,
    pub listen_added: i64,
    pub listen_skipped: i64,
    /// 导入后 auto_match_unlinked 关联上的曲目数
    pub tracks_linked: i64,
    pub errors: Vec<String>,
}

// ── 源库访问 ────────────────────────────────────────────────────────

/// 只读打开源库 `axmusic.db`（不跑迁移，不写源库）。
fn open_source_db(source_root: &Path) -> Result<Connection> {
    let db_path = source_root.join(paths::DB_FILE_NAME);
    if !db_path.is_file() {
        bail!("所选目录不是 AxMusic 库（缺少 axmusic.db）");
    }
    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("打开源库数据库失败: {}", db_path.display()))?;
    Ok(conn)
}

fn has_column(conn: &Connection, table: &str, col: &str) -> bool {
    conn.prepare(&format!("SELECT {col} FROM {table} LIMIT 0"))
        .is_ok()
}

fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |_| Ok(()),
    )
    .optional()
    .map(|r| r.is_some())
    .unwrap_or(false)
}

/// 听歌史不在库根，在 `data_root/`。源侧按常见布局探测：
/// 库根旁 `data/`、库根自身、父目录 `data/`（绿色版 exe 与库目录同级时）。
fn find_source_listen_db(source_root: &Path) -> Option<PathBuf> {
    let mut candidates = vec![
        source_root.join("listen_history.db"),
        source_root.join("data").join("listen_history.db"),
    ];
    if let Some(parent) = source_root.parent() {
        candidates.push(parent.join("data").join("listen_history.db"));
        candidates.push(parent.join("listen_history.db"));
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn load_source_listen_events(path: &Path) -> Result<Vec<ListenEvent>> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("打开听歌库失败: {}", path.display()))?;
    ListenDb::load_all_from(&conn)
}

/// 去掉 Windows `\\?\` 前缀、统一分隔符、去尾部 `\`（不改大小写）。
fn normalize_seps(p: &str) -> String {
    let mut s = p.replace('/', "\\");
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        s = format!(r"\\{rest}");
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        s = rest.to_string();
    }
    s.trim_end_matches('\\').to_string()
}

/// 规范化路径比较键：归一 + 小写。
/// Windows `canonicalize()` 会加 `\\?\`，与 tracks.path 存储形式不一致，必须归一后再比。
fn path_key(p: &str) -> String {
    normalize_seps(p).to_lowercase()
}

/// 取 full 相对 root 的**原始大小写**相对路径。
/// 不在 root 下、前缀未落在路径段边界、或相对段含 `.`/`..` 时返回 None（防路径穿越）。
fn rel_under(root: &str, full: &str) -> Option<String> {
    let root_n = normalize_seps(root);
    let full_n = normalize_seps(full);
    if root_n.is_empty() || full_n.len() < root_n.len() {
        return None;
    }
    let (head, rest) = full_n.split_at(root_n.len());
    if !head.eq_ignore_ascii_case(&root_n) {
        return None;
    }
    // 路径段边界：前缀之后必须紧跟 `\`，否则 d:\music 会吞掉 d:\music-old\x.mp3
    if !rest.starts_with('\\') {
        return None;
    }
    let rel = rest[1..].to_string();
    if rel.is_empty() {
        return None;
    }
    // 源库 tracks.path 来自外部 db（非可信）：拒绝 . 与 .. 段
    if rel.split('\\').any(|seg| seg == "." || seg == "..") {
        return None;
    }
    Some(rel)
}

/// 比较用相对键（小写）。拼接目标路径请用 [`rel_under`] 保留原始大小写。
fn rel_key(root: &str, full: &str) -> Option<String> {
    rel_under(root, full).map(|r| r.to_lowercase())
}

/// 同一路径判断（归一后比较）。
fn same_path(a: &str, b: &str) -> bool {
    path_key(a) == path_key(b)
}

// ── 源库行 ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct SrcCatalogRow {
    source: String,
    kind: String,
    mbid: String,
    release_mbid: String,
    title: String,
    artist: String,
    album: String,
    album_artist: String,
    year: String,
    track_no: Option<i64>,
    disc_no: Option<i64>,
    release_type: String,
    cover_path: Option<String>,
}

#[derive(Debug, Clone)]
struct SrcTrackRow {
    path: String,
    filename: String,
    title: String,
    artist: String,
    album: String,
    album_artist: String,
    year: String,
    track_no: Option<i64>,
    disc_no: Option<i64>,
    duration_ms: i64,
    format: String,
    sample_rate: Option<i64>,
    bit_rate: Option<i64>,
    has_cover: bool,
    has_lyrics: bool,
    has_lrc: bool,
    has_year: bool,
    has_mb_id: bool,
    tag_status: String,
    missing: String,
    release_type: String,
    mb_recording_mbid: String,
    mb_release_mbid: String,
    file_size: i64,
    mtime: i64,
}

fn load_source_catalog(conn: &Connection) -> Result<Vec<SrcCatalogRow>> {
    if !table_exists(conn, "catalog") {
        return Ok(Vec::new());
    }
    let disc = has_column(conn, "catalog", "disc_no");
    let release_type = if has_column(conn, "catalog", "release_type") {
        "release_type"
    } else {
        "''"
    };
    let disc_expr = if disc { "disc_no" } else { "NULL" };
    let sql = format!(
        "SELECT source, kind, COALESCE(mbid,''), COALESCE(release_mbid,''), title, artist, album,
                album_artist, year, track_no, {disc_expr}, {release_type}, cover_path
         FROM catalog"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(SrcCatalogRow {
                source: r.get(0)?,
                kind: r.get(1)?,
                mbid: r.get(2)?,
                release_mbid: r.get(3)?,
                title: r.get(4)?,
                artist: r.get(5)?,
                album: r.get(6)?,
                album_artist: r.get(7)?,
                year: r.get(8)?,
                track_no: r.get(9)?,
                disc_no: r.get(10)?,
                release_type: r.get(11)?,
                cover_path: r.get(12)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn load_source_tracks(conn: &Connection) -> Result<Vec<SrcTrackRow>> {
    if !table_exists(conn, "tracks") {
        return Ok(Vec::new());
    }
    let disc = has_column(conn, "tracks", "disc_no");
    let lrc = has_column(conn, "tracks", "has_lrc");
    // 旧库缺列时占位（migrate 用 ALTER 补齐），保持列序不变以对齐 query_map 下标
    let release_type = if has_column(conn, "tracks", "release_type") {
        " release_type,"
    } else {
        " '',"
    };
    let mb_rec = if has_column(conn, "tracks", "mb_recording_mbid") {
        " mb_recording_mbid,"
    } else {
        " '',"
    };
    let mb_rel = if has_column(conn, "tracks", "mb_release_mbid") {
        " mb_release_mbid,"
    } else {
        " '',"
    };
    let mut cols = String::from(
        "path, filename, title, artist, album, album_artist, year, track_no,
         duration_ms, format, sample_rate, bit_rate,
         has_cover, has_lyrics, has_year, has_mb_id, tag_status, missing,",
    );
    cols.push_str(release_type);
    cols.push_str(mb_rec);
    cols.push_str(mb_rel);
    cols.push_str(" file_size, mtime");
    if lrc {
        cols.push_str(", has_lrc");
    } else {
        cols.push_str(", 0");
    }
    if disc {
        cols.push_str(", disc_no");
    } else {
        cols.push_str(", NULL");
    }
    let sql = format!(
        "SELECT {cols} FROM tracks WHERE is_deleted = 0 OR is_deleted IS NULL"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(SrcTrackRow {
                path: r.get(0)?,
                filename: r.get(1)?,
                title: r.get(2)?,
                artist: r.get(3)?,
                album: r.get(4)?,
                album_artist: r.get(5)?,
                year: r.get(6)?,
                track_no: r.get(7)?,
                duration_ms: r.get(8)?,
                format: r.get(9)?,
                sample_rate: r.get(10)?,
                bit_rate: r.get(11)?,
                has_cover: r.get::<_, i64>(12)? != 0,
                has_lyrics: r.get::<_, i64>(13)? != 0,
                has_year: r.get::<_, i64>(14)? != 0,
                has_mb_id: r.get::<_, i64>(15)? != 0,
                tag_status: r.get(16)?,
                missing: r.get(17)?,
                release_type: r.get(18)?,
                mb_recording_mbid: r.get(19)?,
                mb_release_mbid: r.get(20)?,
                file_size: r.get(21)?,
                mtime: r.get(22)?,
                has_lrc: r.get::<_, i64>(23)? != 0,
                disc_no: r.get(24)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ── 当前库索引（对比用） ───────────────────────────────────────────

#[derive(Default)]
struct CurrentCatalogKeys {
    mbids: HashSet<String>,
    /// (source, release_mbid, track_no, disc_no) — 无 recording MBID
    rel_keys: HashSet<(String, String, i64, i64)>,
    /// (title, artist, album, track_no) — 两个 MBID 都空
    soft_keys: HashSet<(String, String, String, i64)>,
}

fn opt_i64_key(v: Option<i64>) -> i64 {
    // NULL 与「无碟号/无轨号」统一到 i64::MIN，避免与 0 撞车
    v.unwrap_or(i64::MIN)
}

fn soft_key(title: &str, artist: &str, album: &str, track_no: Option<i64>) -> (String, String, String, i64) {
    (
        title.to_lowercase(),
        artist.to_lowercase(),
        album.to_lowercase(),
        opt_i64_key(track_no),
    )
}

/// 把一行的全部去重键登记进集合（主键 + 次级 soft 键），供跨键强度去重。
fn register_catalog_keys(keys: &mut CurrentCatalogKeys, c: &SrcCatalogRow) {
    keys.soft_keys.insert(soft_key(&c.title, &c.artist, &c.album, c.track_no));
    if !c.mbid.is_empty() {
        keys.mbids.insert(c.mbid.to_lowercase());
    }
    if !c.release_mbid.is_empty() {
        keys.rel_keys.insert((
            c.source.clone(),
            c.release_mbid.to_lowercase(),
            opt_i64_key(c.track_no),
            opt_i64_key(c.disc_no),
        ));
    }
}

fn load_current_catalog_keys(db: &LibraryDb) -> Result<CurrentCatalogKeys> {
    let mut keys = CurrentCatalogKeys::default();
    let mut stmt = db.raw_conn().prepare(
        "SELECT COALESCE(mbid,''), COALESCE(release_mbid,''), COALESCE(source,''),
                title, artist, album, track_no, disc_no
         FROM catalog",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, Option<i64>>(6)?,
            r.get::<_, Option<i64>>(7)?,
        ))
    })?;
    for row in rows {
        let (mbid, release_mbid, source, title, artist, album, track_no, disc_no) = row?;
        let c = SrcCatalogRow {
            source,
            kind: String::new(),
            mbid,
            release_mbid,
            title,
            artist,
            album,
            album_artist: String::new(),
            year: String::new(),
            track_no,
            disc_no,
            release_type: String::new(),
            cover_path: None,
        };
        register_catalog_keys(&mut keys, &c);
    }
    Ok(keys)
}

// ── 对比 ────────────────────────────────────────────────────────────

fn catalog_row_exists(keys: &CurrentCatalogKeys, c: &SrcCatalogRow) -> bool {
    if !c.mbid.is_empty() && keys.mbids.contains(&c.mbid.to_lowercase()) {
        return true;
    }
    if !c.release_mbid.is_empty() {
        let k = (
            c.source.clone(),
            c.release_mbid.to_lowercase(),
            opt_i64_key(c.track_no),
            opt_i64_key(c.disc_no),
        );
        if keys.rel_keys.contains(&k) {
            return true;
        }
    }
    // 次级 soft 键：两边键强度不同时（一边有 MBID、一边没有）也能判重
    keys.soft_keys
        .contains(&soft_key(&c.title, &c.artist, &c.album, c.track_no))
}

#[derive(Default)]
struct CurrentTrackKeys {
    abs_paths: HashSet<String>,
    rel_paths: HashSet<String>,
    mbids: HashSet<String>,
    title_artist_dur: HashSet<(String, String, i64)>,
    file_size_name: HashSet<(String, i64)>,
}

fn load_current_track_keys(db: &LibraryDb, current_root: &str) -> Result<CurrentTrackKeys> {
    let mut keys = CurrentTrackKeys::default();
    let mut stmt = db.raw_conn().prepare(
        "SELECT path, filename, title, artist, duration_ms, file_size, mb_recording_mbid
         FROM tracks WHERE is_deleted = 0",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, String>(6)?,
        ))
    })?;
    for row in rows {
        let (path, filename, title, artist, duration_ms, file_size, mbid) = row?;
        keys.abs_paths.insert(path_key(&path));
        if let Some(rel) = rel_key(current_root, &path) {
            keys.rel_paths.insert(rel);
        }
        if !mbid.is_empty() {
            keys.mbids.insert(mbid.to_lowercase());
        }
        if !title.is_empty() && !artist.is_empty() && duration_ms > 0 {
            keys.title_artist_dur
                .insert((title.to_lowercase(), artist.to_lowercase(), duration_ms));
        }
        if file_size > 0 && !filename.is_empty() {
            keys.file_size_name
                .insert((filename.to_lowercase(), file_size));
        }
    }
    Ok(keys)
}

fn song_exists(
    keys: &CurrentTrackKeys,
    source_root: &str,
    t: &SrcTrackRow,
    current_root: &str,
) -> bool {
    if keys.abs_paths.contains(&path_key(&t.path)) {
        return true;
    }
    if let Some(rel_orig) = rel_under(source_root, &t.path) {
        if keys.rel_paths.contains(&rel_orig.to_lowercase()) {
            return true;
        }
        // 相对路径命中磁盘也算已有（DB 可能未扫）；用原始大小写拼接
        let candidate = Path::new(current_root)
            .join(rel_orig.replace('\\', std::path::MAIN_SEPARATOR_STR));
        if candidate.is_file() {
            return true;
        }
    }
    if !t.mb_recording_mbid.is_empty()
        && keys.mbids.contains(&t.mb_recording_mbid.to_lowercase())
    {
        return true;
    }
    if !t.title.is_empty() && !t.artist.is_empty() && t.duration_ms > 0 {
        let k = (
            t.title.to_lowercase(),
            t.artist.to_lowercase(),
            t.duration_ms,
        );
        if keys.title_artist_dur.contains(&k) {
            return true;
        }
    }
    if t.file_size > 0 && !t.filename.is_empty() {
        let k = (t.filename.to_lowercase(), t.file_size);
        if keys.file_size_name.contains(&k) {
            return true;
        }
    }
    false
}

/// 列目录下一层文件名（小写），跳过子目录与隐藏项。
fn list_file_names(dir: &Path, skip_hidden_prefix: bool) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for ent in rd.flatten() {
        let Ok(meta) = ent.metadata() else { continue };
        if meta.is_dir() {
            continue;
        }
        let name = ent.file_name().to_string_lossy().to_string();
        if skip_hidden_prefix && name.starts_with('.') {
            continue;
        }
        out.push(name.to_lowercase());
    }
    out
}

fn list_lrc_names(root: &Path) -> Vec<String> {
    let lrc_dir = root.join(paths::LRC_DIR_NAME);
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(&lrc_dir) else {
        return out;
    };
    for ent in rd.flatten() {
        let Ok(meta) = ent.metadata() else { continue };
        if meta.is_dir() {
            continue;
        }
        let name = ent.file_name().to_string_lossy().to_string();
        if name.to_lowercase().ends_with(".lrc") {
            out.push(name.to_lowercase());
        }
    }
    out
}

fn list_cover_names(root: &Path) -> Vec<String> {
    list_file_names(&root.join(paths::COVERS_DIR_NAME), true)
}

fn list_playlist_stems(root: &Path) -> Vec<String> {
    let dir = root.join(paths::PLAYLISTS_DIR_NAME);
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return out;
    };
    for ent in rd.flatten() {
        let Ok(meta) = ent.metadata() else { continue };
        if meta.is_dir() {
            continue;
        }
        let name = ent.file_name().to_string_lossy().to_string();
        let lower = name.to_lowercase();
        if lower.ends_with(".m3u8") || lower.ends_with(".m3u") {
            let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&name);
            out.push(stem.to_lowercase());
        }
    }
    out
}

// ── 预览 ────────────────────────────────────────────────────────────

/// 识别源库并生成与当前库的对比预览。
pub fn preview(source_root: &Path, current_root: &Path, db: &LibraryDb) -> Result<ImportPreview> {
    let source_str = source_root.to_string_lossy().to_string();
    let current_str = current_root.to_string_lossy().to_string();
    // 不用 canonicalize（Windows 会加 \\?\，与 tracks.path 形式不一致）；归一化后比较
    if same_path(&source_str, &current_str) {
        bail!("不能导入当前库自身");
    }
    if !source_root.is_dir() {
        bail!("所选路径不是文件夹");
    }

    let src_conn = open_source_db(source_root)?;
    if !table_exists(&src_conn, "catalog") && !table_exists(&src_conn, "tracks") {
        bail!("源库数据库缺少 catalog/tracks 表，无法识别");
    }

    // catalog
    let src_catalog = load_source_catalog(&src_conn)?;
    let cur_cat_keys = load_current_catalog_keys(db)?;
    let mut catalog = ImportItemStats::empty();
    catalog.source_total = src_catalog.len() as i64;
    for c in &src_catalog {
        if catalog_row_exists(&cur_cat_keys, c) {
            catalog.duplicate += 1;
        } else {
            catalog.new += 1;
        }
    }

    // songs
    let src_tracks = load_source_tracks(&src_conn)?;
    let cur_track_keys = load_current_track_keys(db, &current_str)?;
    let mut songs = ImportItemStats::empty();
    songs.source_total = src_tracks.len() as i64;
    for t in &src_tracks {
        if song_exists(&cur_track_keys, &source_str, t, &current_str) {
            songs.duplicate += 1;
        } else if Path::new(&t.path).is_file() {
            songs.new += 1;
        } else {
            songs.missing += 1;
        }
    }

    // lyrics / covers / playlists（按文件名对比）
    let src_lrc = list_lrc_names(&source_root);
    let cur_lrc: HashSet<String> = list_lrc_names(&current_root).into_iter().collect();
    let mut lyrics = ImportItemStats::empty();
    lyrics.source_total = src_lrc.len() as i64;
    for name in &src_lrc {
        if cur_lrc.contains(name) {
            lyrics.duplicate += 1;
        } else {
            lyrics.new += 1;
        }
    }

    let src_covers = list_cover_names(&source_root);
    let cur_covers: HashSet<String> = list_cover_names(&current_root).into_iter().collect();
    let mut covers = ImportItemStats::empty();
    covers.source_total = src_covers.len() as i64;
    for name in &src_covers {
        if cur_covers.contains(name) {
            covers.duplicate += 1;
        } else {
            covers.new += 1;
        }
    }

    let src_pls = list_playlist_stems(&source_root);
    let cur_pls: HashSet<String> = list_playlist_stems(&current_root).into_iter().collect();
    let mut playlists = ImportItemStats::empty();
    playlists.source_total = src_pls.len() as i64;
    for name in &src_pls {
        if cur_pls.contains(name) {
            playlists.duplicate += 1;
        } else {
            playlists.new += 1;
        }
    }

    // 听歌史（data_root 独立库；在源路径附近探测）
    let mut listen = ImportItemStats::empty();
    let mut listen_path = None;
    if let Some(lp) = find_source_listen_db(source_root) {
        let events = load_source_listen_events(&lp)?;
        listen.source_total = events.len() as i64;
        let cur = ListenDb::open_default()?;
        for e in &events {
            if cur.has_event(e.started_at, &e.path, e.play_ms).unwrap_or(false) {
                listen.duplicate += 1;
            } else {
                listen.new += 1;
            }
        }
        listen_path = Some(lp.to_string_lossy().to_string());
    }

    Ok(ImportPreview {
        source_root: source_str,
        current_root: current_str,
        catalog,
        songs,
        lyrics,
        covers,
        playlists,
        listen,
        listen_path,
    })
}

// ── 执行导入 ────────────────────────────────────────────────────────

fn copy_file_if_absent(src: &Path, dest: &Path) -> Result<bool> {
    if dest.is_file() {
        return Ok(false);
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败: {}", parent.display()))?;
    }
    std::fs::copy(src, dest).with_context(|| {
        format!(
            "复制失败: {} → {}",
            src.display(),
            dest.display()
        )
    })?;
    Ok(true)
}

/// 把源 cover_path 映射到当前库 covers/；covers 勾选时顺带复制文件。
fn map_cover_path(
    source_cover: &str,
    current_root: &Path,
    copy_files: bool,
) -> Option<String> {
    if source_cover.is_empty() {
        return None;
    }
    let src = Path::new(source_cover);
    let filename = src.file_name()?.to_string_lossy().to_string();
    let dest = current_root
        .join(paths::COVERS_DIR_NAME)
        .join(&filename);
    if copy_files && src.is_file() {
        let _ = copy_file_if_absent(src, &dest);
    }
    if dest.is_file() {
        return Some(dest.to_string_lossy().to_string());
    }
    // 当前库没有该封面：保留源绝对路径（源库仍在磁盘上时仍可显示）
    if src.is_file() {
        return Some(source_cover.to_string());
    }
    None
}

fn dest_song_path(source_root: &str, current_root: &Path, src_path: &str) -> PathBuf {
    // rel_under 已拒绝 `.`/`..` 段，不会写出库根
    if let Some(rel) = rel_under(source_root, src_path) {
        return current_root.join(rel.replace('\\', std::path::MAIN_SEPARATOR_STR));
    }
    // 源路径不在库根下：丢进 Unarchived，文件名冲突则加后缀
    let src = Path::new(src_path);
    let name = src
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "track.bin".into());
    let base = current_root.join(paths::UNARCHIVED_DIR_NAME).join(&name);
    if !base.exists() {
        return base;
    }
    let stem = Path::new(&name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "track".into());
    let ext = src
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    for i in 2..1000 {
        let candidate = if ext.is_empty() {
            current_root
                .join(paths::UNARCHIVED_DIR_NAME)
                .join(format!("{stem}-{i}"))
        } else {
            current_root
                .join(paths::UNARCHIVED_DIR_NAME)
                .join(format!("{stem}-{i}.{ext}"))
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    // 全部候选都被占用：不要返回已存在的 base（会把他人文件当成本曲目）
    current_root
        .join(paths::UNARCHIVED_DIR_NAME)
        .join(format!("{stem}-import.{}", if ext.is_empty() { "bin".into() } else { ext }))
}

/// 执行导入。selection 全 false 时直接返回空结果。
pub fn run(
    source_root: &Path,
    current_root: &Path,
    db: &LibraryDb,
    listen_db: &ListenDb,
    selection: &ImportSelection,
) -> Result<ImportResult> {
    let mut result = ImportResult::default();
    if !(selection.catalog
        || selection.songs
        || selection.lyrics
        || selection.covers
        || selection.playlists
        || selection.listen)
    {
        return Ok(result);
    }

    let source_str = source_root.to_string_lossy().to_string();
    let current_str = current_root.to_string_lossy().to_string();
    if same_path(&source_str, &current_str) {
        bail!("不能导入当前库自身");
    }

    // 0) 听歌史（不依赖源库 axmusic.db；找不到就记 0）
    if selection.listen {
        if let Some(lp) = find_source_listen_db(source_root) {
            match load_source_listen_events(&lp) {
                Ok(events) => match listen_db.merge_events(&events) {
                    Ok((added, skipped)) => {
                        result.listen_added = added;
                        result.listen_skipped = skipped;
                    }
                    Err(e) => result.errors.push(format!("听歌记录合并失败: {e}")),
                },
                Err(e) => result.errors.push(format!("读取听歌库失败: {e}")),
            }
        }
    }

    // 其余项都要源库 axmusic.db
    if !(selection.catalog
        || selection.songs
        || selection.lyrics
        || selection.covers
        || selection.playlists)
    {
        return Ok(result);
    }

    let src_conn = open_source_db(source_root)?;
    let mut linked_now = 0i64;

    // 1) 封面文件（先于 catalog，便于 cover_path 指到当前库）
    if selection.covers {
        let src_dir = source_root.join(paths::COVERS_DIR_NAME);
        let dest_dir = current_root.join(paths::COVERS_DIR_NAME);
        let cur: HashSet<String> = list_cover_names(&current_root).into_iter().collect();
        let src_map = build_ci_name_map(&src_dir);
        for name in list_cover_names(&source_root) {
            let Some(src_path) = src_map.get(&name) else { continue };
            if cur.contains(&name) {
                result.covers_skipped += 1;
                continue;
            }
            let dest = dest_dir.join(src_path.file_name().unwrap_or_default());
            match copy_file_if_absent(src_path, &dest) {
                Ok(true) => result.covers_added += 1,
                Ok(false) => result.covers_skipped += 1,
                Err(e) => result.errors.push(format!("封面 {}: {e}", src_path.display())),
            }
        }
    }

    // 2) catalog（最重要）
    if selection.catalog {
        let src_rows = load_source_catalog(&src_conn)?;
        let mut keys = load_current_catalog_keys(db)?;
        let tx = db.transaction()?;
        for c in &src_rows {
            if catalog_row_exists(&keys, c) {
                result.catalog_skipped += 1;
                continue;
            }
            let cover_path = c
                .cover_path
                .as_deref()
                .and_then(|p| map_cover_path(p, &current_root, selection.covers));
            let row = CatalogRow {
                id: 0,
                source: c.source.clone(),
                kind: c.kind.clone(),
                mbid: c.mbid.clone(),
                release_mbid: c.release_mbid.clone(),
                title: c.title.clone(),
                artist: c.artist.clone(),
                album: c.album.clone(),
                album_artist: c.album_artist.clone(),
                year: c.year.clone(),
                track_no: c.track_no,
                disc_no: c.disc_no,
                release_type: c.release_type.clone(),
                cover_path,
                created_at: String::new(),
            };
            match tx_insert_catalog(&tx, &row) {
                Ok(_) => {
                    result.catalog_added += 1;
                    // 回填 keys：源库内部重复行不再重复插入/重复计数
                    register_catalog_keys(&mut keys, c);
                }
                Err(e) => result.errors.push(format!("catalog「{}」: {e}", c.title)),
            }
        }
        tx.commit()?;
        // 导入后立刻字段匹配，把已入库曲目挂上
        if result.catalog_added > 0 {
            match db.auto_match_unlinked() {
                Ok(n) => linked_now += n as i64,
                Err(e) => result.errors.push(format!("自动关联 catalog 失败: {e}")),
            }
        }
    }

    // 3) 歌曲（复制音频 + 写 tracks）
    if selection.songs {
        let src_tracks = load_source_tracks(&src_conn)?;
        let keys = load_current_track_keys(db, &current_str)?;
        for t in &src_tracks {
            if song_exists(&keys, &source_str, t, &current_str) {
                result.songs_skipped += 1;
                continue;
            }
            let src_path = Path::new(&t.path);
            if !src_path.is_file() {
                result.songs_failed += 1;
                result
                    .errors
                    .push(format!("歌曲源文件缺失: {}", t.path));
                continue;
            }
            let dest = dest_song_path(&source_str, &current_root, &t.path);
            match copy_file_if_absent(src_path, &dest) {
                Ok(true) => {}
                Ok(false) => {
                    // 目标已存在且并非本次复制：不要把他人文件当成本曲目
                    result.songs_failed += 1;
                    result
                        .errors
                        .push(format!("歌曲目标已存在，跳过入库: {}", dest.display()));
                    continue;
                }
                Err(e) => {
                    result.songs_failed += 1;
                    result.errors.push(format!("歌曲 {}: {e}", t.path));
                    continue;
                }
            }
            let dest_str = dest.to_string_lossy().to_string();
            let filename = dest
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| t.filename.clone());
            let dest_meta = std::fs::metadata(&dest).ok();
            let row = TrackRow {
                id: 0,
                path: dest_str,
                filename,
                title: t.title.clone(),
                artist: t.artist.clone(),
                album: t.album.clone(),
                album_artist: t.album_artist.clone(),
                year: t.year.clone(),
                track_no: t.track_no,
                disc_no: t.disc_no,
                duration_ms: t.duration_ms,
                format: t.format.clone(),
                sample_rate: t.sample_rate,
                bit_rate: t.bit_rate,
                has_cover: t.has_cover,
                has_lyrics: t.has_lyrics,
                has_lrc: t.has_lrc,
                has_year: t.has_year,
                has_mb_id: t.has_mb_id,
                tag_status: t.tag_status.clone(),
                missing: t.missing.clone(),
                release_type: t.release_type.clone(),
                mb_recording_mbid: t.mb_recording_mbid.clone(),
                mb_release_mbid: t.mb_release_mbid.clone(),
                catalog_id: None,
                mtime: dest_meta
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(t.mtime),
                file_size: dest_meta.map(|m| m.len() as i64).unwrap_or(t.file_size),
                catalog_title: None,
                catalog_artist: None,
                catalog_album: None,
                catalog_year: None,
                catalog_track_no: None,
            };
            match db.upsert_track(&row, row.file_size as u64, row.mtime as u64) {
                Ok(_) => result.songs_added += 1,
                Err(e) => {
                    result.songs_failed += 1;
                    result.errors.push(format!("歌曲入库 {}: {e}", dest.display()));
                }
            }
        }
        if result.songs_added > 0 {
            match db.auto_match_unlinked() {
                Ok(n) => linked_now += n as i64,
                Err(e) => result.errors.push(format!("自动关联 catalog 失败: {e}")),
            }
        }
    }

    // 4) 歌词
    if selection.lyrics {
        let src_dir = source_root.join(paths::LRC_DIR_NAME);
        let dest_dir = current_root.join(paths::LRC_DIR_NAME);
        let cur: HashSet<String> = list_lrc_names(&current_root).into_iter().collect();
        let src_map = build_ci_name_map(&src_dir);
        for name in list_lrc_names(&source_root) {
            let Some(src_path) = src_map.get(&name) else {
                continue;
            };
            if cur.contains(&name) {
                result.lyrics_skipped += 1;
                continue;
            }
            let dest = dest_dir.join(src_path.file_name().unwrap_or_default());
            match copy_file_if_absent(src_path, &dest) {
                Ok(true) => result.lyrics_added += 1,
                Ok(false) => result.lyrics_skipped += 1,
                Err(e) => result.errors.push(format!("歌词 {}: {e}", src_path.display())),
            }
        }
    }

    // 5) 歌单（重名跳过，不覆盖）
    if selection.playlists {
        let src_dir = source_root.join(paths::PLAYLISTS_DIR_NAME);
        let dest_dir = current_root.join(paths::PLAYLISTS_DIR_NAME);
        let cur: HashSet<String> = list_playlist_stems(&current_root).into_iter().collect();
        if let Ok(rd) = std::fs::read_dir(&src_dir) {
            for ent in rd.flatten() {
                let Ok(meta) = ent.metadata() else { continue };
                if meta.is_dir() {
                    continue;
                }
                let name = ent.file_name().to_string_lossy().to_string();
                let lower = name.to_lowercase();
                if !(lower.ends_with(".m3u8") || lower.ends_with(".m3u")) {
                    continue;
                }
                let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&name);
                if cur.contains(&stem.to_lowercase()) {
                    result.playlists_skipped += 1;
                    continue;
                }
                let dest = dest_dir.join(&name);
                match copy_file_if_absent(&ent.path(), &dest) {
                    Ok(true) => result.playlists_added += 1,
                    Ok(false) => result.playlists_skipped += 1,
                    Err(e) => {
                        result.errors.push(format!("歌单 {}: {e}", name));
                    }
                }
            }
        }
    }

    result.tracks_linked = linked_now;
    // 错误太多时截断展示
    if result.errors.len() > 20 {
        let extra = result.errors.len() - 20;
        result.errors.truncate(20);
        result.errors.push(format!("…另有 {extra} 条错误未列出"));
    }
    Ok(result)
}

/// 目录内文件名（小写）→ 原始路径映射，一次 read_dir 供整个循环复用。
fn build_ci_name_map(dir: &Path) -> HashMap<String, PathBuf> {
    let mut map = HashMap::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for ent in rd.flatten() {
            if ent.path().is_file() {
                map.insert(ent.file_name().to_string_lossy().to_lowercase(), ent.path());
            }
        }
    }
    map
}

/// 在事务里插入 catalog 行（与 LibraryDb::insert_catalog 同语义，但挂在传入事务上）。
fn tx_insert_catalog(tx: &rusqlite::Transaction<'_>, c: &CatalogRow) -> Result<i64> {
    use rusqlite::params;
    let existing: Option<i64> = if !c.mbid.is_empty() {
        tx.query_row(
            "SELECT id FROM catalog WHERE mbid = ?1 ORDER BY id LIMIT 1",
            params![c.mbid],
            |r| r.get(0),
        )
        .optional()?
    } else if !c.release_mbid.is_empty() {
        tx.query_row(
            "SELECT id FROM catalog WHERE mbid = '' AND source = ?1 AND release_mbid = ?2
               AND track_no IS ?3 AND disc_no IS ?4
             ORDER BY id LIMIT 1",
            params![c.source, c.release_mbid, c.track_no, c.disc_no],
            |r| r.get(0),
        )
        .optional()?
    } else {
        None
    };
    if let Some(id) = existing {
        tx.execute(
            "UPDATE catalog SET
                source=?1, kind=?2, release_mbid=?3, title=?4, artist=?5,
                album=?6, album_artist=?7, year=?8, track_no=?9,
                release_type=?10, cover_path=COALESCE(?11, cover_path), disc_no=?13
             WHERE id=?12",
            params![
                c.source,
                c.kind,
                c.release_mbid,
                c.title,
                c.artist,
                c.album,
                c.album_artist,
                c.year,
                c.track_no,
                c.release_type,
                c.cover_path,
                id,
                c.disc_no,
            ],
        )?;
        return Ok(id);
    }
    tx.execute(
        "INSERT INTO catalog (
            source, kind, mbid, release_mbid, title, artist, album,
            album_artist, year, track_no, release_type, cover_path, created_at, disc_no
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        params![
            c.source,
            c.kind,
            c.mbid,
            c.release_mbid,
            c.title,
            c.artist,
            c.album,
            c.album_artist,
            c.year,
            c.track_no,
            c.release_type,
            c.cover_path,
            crate::library::now_unix_secs(),
            c.disc_no,
        ],
    )?;
    Ok(tx.last_insert_rowid())
}
