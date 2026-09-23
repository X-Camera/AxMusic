//! Tauri IPC commands.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::library::{AlbumCard, ArtistCard, LibraryDb, LibraryRoot, LibraryStats, TrackFilter, TrackRow};
use crate::player::{Player, PlayerSnapshot, QueueItem, TrackInfo};
use crate::playlists::{PlaylistAddItem, PlaylistDetail, PlaylistSummary};
use crate::settings::AppSettings;
use crate::{playlists, scanner, settings};

pub struct AppState {
    /// Library working DB (`<library>/axmusic.db`). None until a library root is set.
    pub db: Mutex<Option<LibraryDb>>,
    pub player: Mutex<Player>,
    pub scanning: Mutex<bool>,
    pub settings: Mutex<AppSettings>,
}

fn require_db<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, Option<LibraryDb>>, String> {
    let guard = state.db.lock().map_err(|e| e.to_string())?;
    if guard.is_none() {
        return Err("尚未初始化库目录".into());
    }
    Ok(guard)
}

fn db_ref<'a>(guard: &'a std::sync::MutexGuard<'_, Option<LibraryDb>>) -> Result<&'a LibraryDb, String> {
    guard
        .as_ref()
        .ok_or_else(|| "尚未初始化库目录".to_string())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InitLibraryRequest {
    /// "new" | "existing"
    pub mode: String,
    pub parent: Option<String>,
    pub name: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IncludeResult {
    pub copied_to: String,
    pub track: TrackRow,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanResult {
    pub total: u64,
    pub added: u64,
    pub updated: u64,
    pub errors: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PathsInfo {
    pub data_root: String,
    pub portable: bool,
    pub db_path: String,
    pub settings_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

// ── paths / meta ──────────────────────────────────────────────────

#[tauri::command]
pub fn get_paths() -> PathsInfo {
    crate::paths::ensure_data_root();
    let app_settings = settings::load();
    let db_path = app_settings
        .library_root
        .as_deref()
        .map(|root| crate::paths::library_db_path(std::path::Path::new(root)))
        .filter(|p| p.exists());
    PathsInfo {
        data_root: crate::paths::data_root().to_string_lossy().to_string(),
        portable: crate::paths::is_portable(),
        db_path: db_path
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        settings_path: crate::settings::settings_path().to_string_lossy().to_string(),
    }
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        name: "AxMusic".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    }
}

// ── settings ──────────────────────────────────────────────────────

/// 设置页部分更新：只覆盖非 null 字段。
#[derive(Debug, Default, Deserialize)]
pub struct SettingsPatch {
    pub volume: Option<f32>,
    pub play_mode: Option<crate::settings::PlayMode>,
    pub restore_volume: Option<bool>,
    pub lyrics_save_mode: Option<crate::settings::LyricsSaveMode>,
    pub lyrics_prefer: Option<crate::settings::LyricsPrefer>,
    pub lyrics_sources: Option<crate::settings::LyricsSources>,
    pub songs_view: Option<crate::settings::SongsView>,
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<crate::settings::AppSettings, String> {
    let guard = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(guard.clone())
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<crate::settings::AppSettings, String> {
    let mut guard = state.settings.lock().map_err(|e| e.to_string())?;
    if let Some(v) = patch.volume {
        guard.volume = v.clamp(0.0, 1.0);
    }
    if let Some(m) = patch.play_mode {
        guard.play_mode = m;
        let mode = match m {
            crate::settings::PlayMode::Sequential => crate::player::PlayMode::Sequential,
            crate::settings::PlayMode::Shuffle => crate::player::PlayMode::Shuffle,
            crate::settings::PlayMode::RepeatOne => crate::player::PlayMode::RepeatOne,
        };
        if let Ok(mut player) = state.player.lock() {
            player.set_play_mode(mode);
        }
    }
    if let Some(v) = patch.restore_volume {
        guard.restore_volume = v;
    }
    if let Some(m) = patch.lyrics_save_mode {
        guard.lyrics_save_mode = m;
    }
    if let Some(p) = patch.lyrics_prefer {
        guard.lyrics_prefer = p;
    }
    if let Some(s) = patch.lyrics_sources {
        // 至少保留一个源，避免搜索永远空跑
        if s.lrclib || s.netease || s.qq {
            guard.lyrics_sources = s;
        }
    }
    if let Some(v) = patch.songs_view {
        guard.songs_view = v;
    }
    settings::save(&guard).map_err(|e| e.to_string())?;
    let snapshot = guard.clone();
    drop(guard);
    let _ = app.emit("settings://changed", &snapshot);
    Ok(snapshot)
}

/// 在资源管理器中打开目录（设置页「打开数据目录」）。
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        Err("暂不支持打开目录".into())
    }
}

// ── library ───────────────────────────────────────────────────────

#[tauri::command]
pub fn get_library_root(state: State<'_, AppState>) -> Result<Option<LibraryRoot>, String> {
    let root = state
        .settings
        .lock()
        .map_err(|e| e.to_string())?
        .library_root
        .clone();
    Ok(root.map(|path| LibraryRoot {
        id: 0,
        initialized_at: String::new(),
        path,
    }))
}

#[tauri::command]
pub fn init_library(
    state: State<'_, AppState>,
    req: InitLibraryRequest,
) -> Result<LibraryRoot, String> {
    let dir: PathBuf = match req.mode.as_str() {
        "new" => {
            let parent = PathBuf::from(req.parent.ok_or("缺少父目录")?);
            let name = req.name.ok_or("缺少文件夹名")?;
            crate::library::create_library_dir(&parent, &name).map_err(|e| e.to_string())?
        }
        "existing" => {
            let path = PathBuf::from(req.path.ok_or("缺少目录路径")?);
            if !path.is_dir() {
                return Err("所选路径不是文件夹".into());
            }
            path
        }
        other => return Err(format!("未知模式: {other}")),
    };

    // App config: remember library root in settings.json
    {
        let mut app_settings = state.settings.lock().map_err(|e| e.to_string())?;
        app_settings.library_root = Some(dir.to_string_lossy().to_string());
        settings::save(&app_settings).map_err(|e| e.to_string())?;
    }

    // Working DB lives in the library root
    let db_path = crate::paths::library_db_path(&dir);
    let db = LibraryDb::open(&db_path).map_err(|e| e.to_string())?;
    let root_row = db.set_library_root(&dir).map_err(|e| e.to_string())?;
    *state.db.lock().map_err(|e| e.to_string())? = Some(db);

    Ok(root_row)
}

#[tauri::command]
pub fn get_tracks(
    state: State<'_, AppState>,
    filter: Option<TrackFilter>,
) -> Result<Vec<TrackRow>, String> {
    let guard = require_db(&state)?;
    let db = db_ref(&guard)?;
    db.list_tracks(&filter.unwrap_or_default())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_albums(state: State<'_, AppState>) -> Result<Vec<AlbumCard>, String> {
    let guard = require_db(&state)?;
    let db = db_ref(&guard)?;
    db.list_albums().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_album_tracks(
    state: State<'_, AppState>,
    album: String,
    album_artist: String,
) -> Result<Vec<TrackRow>, String> {
    let guard = require_db(&state)?;
    let db = db_ref(&guard)?;
    db.tracks_of_album(&album, &album_artist)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_artists(state: State<'_, AppState>) -> Result<Vec<ArtistCard>, String> {
    let guard = require_db(&state)?;
    let db = db_ref(&guard)?;
    db.list_artists().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_artist_albums(state: State<'_, AppState>, artist: String) -> Result<Vec<AlbumCard>, String> {
    let guard = require_db(&state)?;
    let db = db_ref(&guard)?;
    db.albums_of_artist(&artist).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_artist_tracks(state: State<'_, AppState>, artist: String) -> Result<Vec<TrackRow>, String> {
    let guard = require_db(&state)?;
    let db = db_ref(&guard)?;
    db.tracks_of_artist(&artist).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_track_count(state: State<'_, AppState>) -> Result<i64, String> {
    let guard = require_db(&state)?;
    let db = db_ref(&guard)?;
    db.track_count().map_err(|e| e.to_string())
}

/// 库统计（管理页右栏未选中曲目时展示）。
#[tauri::command]
pub fn library_stats(state: State<'_, AppState>) -> Result<LibraryStats, String> {
    let guard = require_db(&state)?;
    let db = db_ref(&guard)?;
    db.library_stats().map_err(|e| e.to_string())
}

/// 提取文件内嵌封面 → 96px JPEG data URL（按 path+mtime+size 磁盘缓存于 `<库>/covers/.thumbs/`）。
/// 无封面或解析失败返回 Ok(None)，前端显示占位图。
#[tauri::command]
pub async fn track_cover_thumb(
    state: State<'_, AppState>,
    path: String,
) -> Result<Option<String>, String> {
    // 缩略图缓存在 `<库>/covers/.thumbs/`（随库走）；无库根时只提取不缓存
    let thumbs_dir = state
        .settings
        .lock()
        .map_err(|e| e.to_string())?
        .library_root
        .clone()
        .map(|root| PathBuf::from(root).join("covers").join(".thumbs"));
    tauri::async_runtime::spawn_blocking(move || {
        cover_thumb_blocking(Path::new(&path), thumbs_dir.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn cover_thumb_blocking(path: &Path, thumbs_dir: Option<&Path>) -> Result<Option<String>, String> {
    cover_image_blocking(path, thumbs_dir, 96)
}

/// Larger cover for the now-playing page (cached beside thumbs at 640px).
fn cover_image_blocking(
    path: &Path,
    thumbs_dir: Option<&Path>,
    max_edge: u32,
) -> Result<Option<String>, String> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    use lofty::file::TaggedFileExt as _;
    use lofty::picture::PictureType;
    use lofty::probe::Probe;

    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let key = {
        use std::hash::{Hash as _, Hasher as _};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        format!("{}|{}|{}|{}", path.to_string_lossy(), mtime, meta.len(), max_edge)
            .hash(&mut h);
        format!("{:016x}", h.finish())
    };

    let cache_path = thumbs_dir.map(|dir| dir.join(format!("{key}.jpg")));
    if let Some(dir) = thumbs_dir {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }

    let cached = cache_path.as_ref().and_then(|p| std::fs::read(p).ok());
    let bytes = match cached {
        Some(b) => b,
        None => {
            let tagged = Probe::open(path)
                .and_then(|p| p.read())
                .map_err(|e| e.to_string())?;
            let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else {
                return Ok(None);
            };
            let pic = tag
                .pictures()
                .iter()
                .find(|p| p.pic_type() == PictureType::CoverFront)
                .or_else(|| tag.pictures().first());
            let Some(pic) = pic else { return Ok(None) };

            // webp/gif 等未启用格式解码失败 → 占位图（已知限制）
            let img = match image::load_from_memory(pic.data()) {
                Ok(i) => i,
                Err(_) => return Ok(None),
            };
            let thumb = img.thumbnail(max_edge, max_edge);
            let mut buf = std::io::Cursor::new(Vec::new());
            thumb
                .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
                    &mut buf, 85,
                ))
                .map_err(|e| e.to_string())?;
            let b = buf.into_inner();
            if let Some(p) = &cache_path {
                std::fs::write(p, &b).ok();
            }
            b
        }
    };

    Ok(Some(format!("data:image/jpeg;base64,{}", B64.encode(bytes))))
}

/// 正在播放页元数据：标签 + 大图封面 + 内嵌/外挂歌词。
#[tauri::command]
pub async fn track_media_info(path: String) -> Result<serde_json::Value, String> {
    let thumbs_dir = state_thumbs_dir_hint();
    tauri::async_runtime::spawn_blocking(move || {
        let p = Path::new(&path);
        let (title, artist, album, album_artist, year) = {
            use lofty::file::TaggedFileExt as _;
            use lofty::prelude::{Accessor, ItemKey};
            use lofty::probe::Probe;
            let tagged = Probe::open(p).and_then(|x| x.read()).map_err(|e| e.to_string())?;
            let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
            match tag {
                Some(tag) => {
                    let get = |k: ItemKey| {
                        tag.get_string(&k).map(|s| s.to_string()).unwrap_or_default()
                    };
                    (
                        tag.title().map(|s| s.to_string()).unwrap_or_default(),
                        tag.artist().map(|s| s.to_string()).unwrap_or_default(),
                        tag.album().map(|s| s.to_string()).unwrap_or_default(),
                        get(ItemKey::AlbumArtist),
                        tag.year().map(|y| y.to_string()).unwrap_or_default(),
                    )
                }
                None => (
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                ),
            }
        };
        let filename = p
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        let cover = cover_image_blocking(p, thumbs_dir.as_deref(), 640)?;
        let embedded = crate::lyrics::read_embedded(p).ok().flatten();
        let sidecar = crate::lyrics::read_sidecar(p)
            .ok()
            .filter(|s| !s.trim().is_empty());
        Ok(serde_json::json!({
            "path": path,
            "filename": filename,
            "title": title,
            "artist": artist,
            "album": album,
            "album_artist": album_artist,
            "year": year,
            "has_lyrics": embedded.is_some(),
            "cover_data": cover,
            "embedded": embedded,
            "sidecar": sidecar,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn state_thumbs_dir_hint() -> Option<PathBuf> {
    // 正在播放可能播库外文件：不依赖 AppState，缓存到临时目录即可。
    Some(std::env::temp_dir().join("axmusic").join("covers"))
}

/// Incremental scan. Progress events: `scan://progress`, `scan://done`.
#[tauri::command]
pub async fn refresh_scan(app: AppHandle, state: State<'_, AppState>) -> Result<ScanResult, String> {
    let root = state
        .settings
        .lock()
        .map_err(|e| e.to_string())?
        .library_root
        .clone()
        .ok_or("尚未初始化库目录")?;

    {
        let mut flag = state.scanning.lock().map_err(|e| e.to_string())?;
        if *flag {
            return Err("扫描已在进行中".into());
        }
        *flag = true;
    }

    let root_path = PathBuf::from(&root);
    let db_path = crate::paths::library_db_path(&root_path);
    let result = std::thread::spawn({
        let app = app.clone();
        move || -> anyhow::Result<ScanResult> {
            let db = LibraryDb::open(&db_path)?;
            let stats = scanner::scan_library(&db, &root_path, |p| {
                let _ = app.emit(
                    "scan://progress",
                    serde_json::json!({
                        "scanned": p.scanned,
                        "totalFiles": p.total_files,
                        "added": p.added,
                        "updated": p.updated,
                        "errors": p.errors,
                        "current": p.current,
                    }),
                );
            })?;
            Ok(ScanResult {
                total: stats.total,
                added: stats.added,
                updated: stats.updated,
                errors: stats.errors,
            })
        }
    })
    .join()
    .map_err(|_| "扫描线程异常结束".to_string())?;

    {
        let mut flag = state.scanning.lock().map_err(|e| e.to_string())?;
        *flag = false;
    }

    match result {
        Ok(r) => {
            let _ = app.emit("scan://done", &r);
            Ok(r)
        }
        Err(e) => Err(format!("{e:#}")),
    }
}

/// Whether a file path already lives inside the library root.
/// 纳入库管理 is for 库外 files only (playback side).
#[tauri::command]
pub fn is_in_library(state: State<'_, AppState>, path: String) -> Result<bool, String> {
    let root = state
        .settings
        .lock()
        .map_err(|e| e.to_string())?
        .library_root
        .clone();
    let Some(root) = root else {
        return Ok(false);
    };
    let src = PathBuf::from(&path);
    let (Ok(src_can), Ok(root_can)) = (src.canonicalize(), PathBuf::from(&root).canonicalize())
    else {
        // Fallback: prefix check on normalized strings
        let norm = |s: &str| s.replace('/', "\\").to_lowercase();
        return Ok(norm(&path).starts_with(&norm(&root)));
    };
    Ok(src_can.starts_with(&root_can))
}

/// 纳入库管理：copy file into library (lossless-archive template) + register.
#[tauri::command]
pub fn include_in_library(
    state: State<'_, AppState>,
    path: String,
) -> Result<IncludeResult, String> {
    let src = PathBuf::from(&path);
    if !src.is_file() {
        return Err("文件不存在".into());
    }

    let db = state.db.lock().map_err(|e| e.to_string())?;
    let db = db.as_ref().ok_or("尚未初始化库目录")?;
    let root = state
        .settings
        .lock()
        .map_err(|e| e.to_string())?
        .library_root
        .clone()
        .ok_or("尚未初始化库目录")?;
    let root = PathBuf::from(root);

    // Same-file shortcut
    if let Ok(src_can) = src.canonicalize() {
        if let Ok(root_can) = root.canonicalize() {
            if src_can.starts_with(&root_can) {
                // already inside library — just ensure it's registered via a rescan of one file
                let mut row = scanner::read_track(&src).map_err(|e| e.to_string())?;
                let meta = std::fs::metadata(&src).ok();
                let file_size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let mtime = meta
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                row.path = src.to_string_lossy().to_string();
                db.upsert_track(&row, file_size, mtime).map_err(|e| e.to_string())?;
                return Ok(IncludeResult {
                    copied_to: row.path.clone(),
                    track: row,
                });
            }
        }
    }

    let mut row = scanner::read_track(&src).map_err(|e| e.to_string())?;
    let dest = build_library_dest(&root, &row, &src);

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if dest.exists() {
        return Err(format!(
            "目标已存在，未覆盖: {}",
            dest.display()
        ));
    }
    std::fs::copy(&src, &dest).map_err(|e| format!("复制失败: {e}"))?;

    let meta = std::fs::metadata(&dest).ok();
    let file_size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let mtime = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    row.path = dest.to_string_lossy().to_string();
    row.filename = dest
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    db.upsert_track(&row, file_size, mtime)
        .map_err(|e| e.to_string())?;

    Ok(IncludeResult {
        copied_to: row.path.clone(),
        track: row,
    })
}

/// lossless-archive: `{albumartist}\{year} - {album}\{track} - {title}.{ext}`
fn build_library_dest(root: &Path, row: &TrackRow, src: &Path) -> PathBuf {
    use crate::scanner::sanitize_segment;

    let album_artist = if !row.album_artist.is_empty() {
        row.album_artist.as_str()
    } else if !row.artist.is_empty() {
        row.artist.as_str()
    } else {
        "Unknown Artist"
    };
    let album = if !row.album.is_empty() {
        row.album.as_str()
    } else {
        "Unknown Album"
    };
    let year_seg = if row.year.is_empty() {
        "0000".to_string()
    } else {
        let digits: String = row.year.chars().filter(|c| c.is_ascii_digit()).collect();
        let y = digits.chars().take(4).collect::<String>();
        if y.len() == 4 {
            y
        } else {
            "0000".into()
        }
    };

    let track_no = row.track_no.unwrap_or(0).clamp(0, 999);
    let track_seg = format!("{track_no:02}");
    let title = if row.title.is_empty() {
        src.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".into())
    } else {
        row.title.clone()
    };

    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "bin".into());

    root.join(sanitize_segment(album_artist))
        .join(sanitize_segment(&format!("{year_seg} - {album}")))
        .join(sanitize_segment(&format!("{track_seg} - {title}.{ext}")))
}

// ── player ────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_player_state(state: State<'_, AppState>) -> Result<PlayerSnapshot, String> {
    let mut player = state.player.lock().map_err(|e| e.to_string())?;
    // auto-advance tick
    player.tick();
    Ok(player.snapshot())
}

#[tauri::command]
pub fn play_file(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<PlayerSnapshot, String> {
    let mut player = state.player.lock().map_err(|e| e.to_string())?;
    let info = player
        .play_path(Path::new(&path))
        .map_err(|e| e.to_string())?;
    let mut snap = player.snapshot();
    // 以本次点击为准（worker 打开解码前 snapshot 可能仍指向上一首）
    snap.track = Some(info);
    snap.status = crate::player::PlayStatus::Playing;
    let _ = app.emit("player://state", &snap);
    Ok(snap)
}

#[tauri::command]
pub fn play_queue(
    app: AppHandle,
    state: State<'_, AppState>,
    items: Vec<QueueItem>,
    start: usize,
) -> Result<PlayerSnapshot, String> {
    let mut player = state.player.lock().map_err(|e| e.to_string())?;
    let info = player
        .play_queue(items, start)
        .map_err(|e| e.to_string())?;
    let mut snap = player.snapshot();
    // 以本次点击为准，避免界面显示新歌、出声还是上一首
    snap.track = Some(info);
    snap.status = crate::player::PlayStatus::Playing;
    let _ = app.emit("player://state", &snap);
    Ok(snap)
}

#[tauri::command]
pub fn player_play(app: AppHandle, state: State<'_, AppState>) -> Result<PlayerSnapshot, String> {
    let mut player = state.player.lock().map_err(|e| e.to_string())?;
    player.engine.play_inner();
    let snap = player.snapshot();
    let _ = app.emit("player://state", &snap);
    Ok(snap)
}

#[tauri::command]
pub fn player_pause(app: AppHandle, state: State<'_, AppState>) -> Result<PlayerSnapshot, String> {
    let mut player = state.player.lock().map_err(|e| e.to_string())?;
    player.engine.pause_inner();
    let snap = player.snapshot();
    let _ = app.emit("player://state", &snap);
    Ok(snap)
}

#[tauri::command]
pub fn player_toggle(app: AppHandle, state: State<'_, AppState>) -> Result<PlayerSnapshot, String> {
    let mut player = state.player.lock().map_err(|e| e.to_string())?;
    player.engine.play_pause();
    let snap = player.snapshot();
    let _ = app.emit("player://state", &snap);
    Ok(snap)
}

#[tauri::command]
pub fn player_next(app: AppHandle, state: State<'_, AppState>) -> Result<PlayerSnapshot, String> {
    let mut player = state.player.lock().map_err(|e| e.to_string())?;
    let _ = player.next().map_err(|e| e.to_string())?;
    let snap = player.snapshot();
    let _ = app.emit("player://state", &snap);
    Ok(snap)
}

#[tauri::command]
pub fn player_prev(app: AppHandle, state: State<'_, AppState>) -> Result<PlayerSnapshot, String> {
    let mut player = state.player.lock().map_err(|e| e.to_string())?;
    let _ = player.prev().map_err(|e| e.to_string())?;
    let snap = player.snapshot();
    let _ = app.emit("player://state", &snap);
    Ok(snap)
}

#[tauri::command]
pub fn player_seek(
    app: AppHandle,
    state: State<'_, AppState>,
    ms: u64,
) -> Result<PlayerSnapshot, String> {
    let mut player = state.player.lock().map_err(|e| e.to_string())?;
    player.engine.seek_to(ms).map_err(|e| e.to_string())?;
    let snap = player.snapshot();
    let _ = app.emit("player://state", &snap);
    Ok(snap)
}

#[tauri::command]
pub fn player_set_volume(
    app: AppHandle,
    state: State<'_, AppState>,
    volume: f32,
) -> Result<PlayerSnapshot, String> {
    let mut player = state.player.lock().map_err(|e| e.to_string())?;
    player.engine.set_volume_f32(volume.clamp(0.0, 1.0));
    if let Ok(mut s) = state.settings.lock() {
        s.volume = volume.clamp(0.0, 1.0);
        let _ = settings::save(&s);
    }
    let snap = player.snapshot();
    let _ = app.emit("player://state", &snap);
    Ok(snap)
}

/// 切换播放模式（顺序 / 随机 / 单曲），同时写入 settings.json
#[tauri::command]
pub fn player_set_play_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: crate::player::PlayMode,
) -> Result<PlayerSnapshot, String> {
    let mut player = state.player.lock().map_err(|e| e.to_string())?;
    player.set_play_mode(mode);
    if let Ok(mut s) = state.settings.lock() {
        s.play_mode = match mode {
            crate::player::PlayMode::Sequential => crate::settings::PlayMode::Sequential,
            crate::player::PlayMode::Shuffle => crate::settings::PlayMode::Shuffle,
            crate::player::PlayMode::RepeatOne => crate::settings::PlayMode::RepeatOne,
        };
        let _ = settings::save(&s);
    }
    let snap = player.snapshot();
    let _ = app.emit("player://state", &snap);
    Ok(snap)
}

// ── browse helpers ────────────────────────────────────────────────

/// Fallback browse: list audio files in a folder (any path). Used by placeholder/album open.
#[tauri::command]
pub fn list_dir_audio(path: String) -> Result<Vec<TrackInfo>, String> {
    let dir = PathBuf::from(path);
    if !dir.is_dir() {
        return Err("不是文件夹".into());
    }
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let p = e.path();
            if !p.is_file() {
                continue;
            }
            let ext = p
                .extension()
                .and_then(|x| x.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            if scanner::AUDIO_EXTS.contains(&ext.as_str()) {
                let info = TrackInfo {
                    path: p.to_string_lossy().to_string(),
                    title: p
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    duration_ms: 0,
                    sample_rate: 0,
                    channels: 2,
                };
                out.push(info);
            }
        }
    }
    Ok(out)
}


// ── lyrics (LRCLIB + 网易云 + QQ音乐) ─────────────────────────────

use crate::lyrics::{self, LyricsCandidate, LyricsContent};

/// 歌词操作目标：库内 `track_id`（再查 path）或直接 `path`（满窗播放的库外文件）。
/// 库外返回 `(None, path)`，不碰 DB。
fn resolve_lyrics_target(
    state: &State<'_, AppState>,
    track_id: Option<i64>,
    path: Option<String>,
) -> Result<(Option<i64>, PathBuf), String> {
    if let Some(id) = track_id.filter(|&id| id > 0) {
        let guard = require_db(state)?;
        let db = db_ref(&guard)?;
        let row = db
            .get_track_by_id(id)
            .map_err(|e| e.to_string())?
            .ok_or("曲目不存在")?;
        return Ok((Some(id), PathBuf::from(row.path)));
    }
    let p = path
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or("缺少曲目路径")?;
    Ok((None, PathBuf::from(p)))
}

/// 按绝对路径查库内曲目；无库/未入库返回 null（满窗搜索歌词前解析 id）。
#[tauri::command]
pub fn get_track_by_path(
    state: State<'_, AppState>,
    path: String,
) -> Result<Option<TrackRow>, String> {
    let Ok(guard) = state.db.lock() else {
        return Ok(None);
    };
    let Some(db) = guard.as_ref() else {
        return Ok(None);
    };
    db.get_track_by_path(&path).map_err(|e| e.to_string())
}

/// Fan out a lyrics search to ALL sources concurrently.
/// Returns immediately; each source pushes its batch via `lyrics://batch`
/// ({trackId, source, items}) and a final `lyrics://done` ({trackId})
/// fires when all sources have reported. 库外文件 trackId 恒为 0。
#[tauri::command]
pub async fn lyrics_search(
    app: AppHandle,
    state: State<'_, AppState>,
    track_id: Option<i64>,
    path: Option<String>,
    artist: Option<String>,
    title: Option<String>,
) -> Result<(), String> {
    let (tid, _path) = resolve_lyrics_target(&state, track_id, path)?;
    let event_id = tid.unwrap_or(0);

    let (db_title, db_artist, db_album) = if let Some(id) = tid {
        let guard = require_db(&state)?;
        let db = db_ref(&guard)?;
        let track = db
            .get_track_by_id(id)
            .map_err(|e| e.to_string())?
            .ok_or("曲目不存在")?;
        (track.title, track.artist, track.album)
    } else {
        (String::new(), String::new(), String::new())
    };

    // 前端搜索面板允许用户改歌手/歌名；传入非空值时覆盖文件标签值
    let title = title
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(db_title);
    let artist = artist
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(db_artist);
    let album = db_album;
    if title.is_empty() && artist.is_empty() {
        return Err("歌手和歌名至少填一个".into());
    }
    let track_id = event_id;
    let enabled = {
        let guard = state.settings.lock().map_err(|e| e.to_string())?;
        guard.lyrics_sources.clone()
    };

    std::thread::spawn(move || {
        use std::sync::mpsc;
        let (tx, rx) = mpsc::channel::<(&'static str, Result<Vec<LyricsCandidate>, String>)>();
        let mut in_flight = 0usize;

        macro_rules! spawn_source {
            ($name:expr, $call:expr) => {{
                let tx = tx.clone();
                let (t, a, al) = (title.clone(), artist.clone(), album.clone());
                std::thread::spawn(move || {
                    let r = $call(&t, &a, &al).map_err(|e| e.to_string());
                    let _ = tx.send(($name, r));
                });
                in_flight += 1;
            }};
        }

        if enabled.lrclib {
            spawn_source!(lyrics::SOURCE_LRCLIB, lyrics::lrclib::search);
        }
        if enabled.netease {
            spawn_source!(lyrics::SOURCE_NETEASE, lyrics::netease::search);
        }
        if enabled.qq {
            spawn_source!(lyrics::SOURCE_QQ, lyrics::qqmusic::search);
        }
        drop(tx);

        for _ in 0..in_flight {
            if let Ok((source, result)) = rx.recv() {
                match result {
                    Ok(items) => {
                        let _ = app.emit(
                            "lyrics://batch",
                            serde_json::json!({
                                "trackId": track_id,
                                "source": source,
                                "items": items,
                            }),
                        );
                    }
                    Err(e) => {
                        let _ = app.emit(
                            "lyrics://batch",
                            serde_json::json!({
                                "trackId": track_id,
                                "source": source,
                                "items": [],
                                "error": e,
                            }),
                        );
                    }
                }
            }
        }
        let _ = app.emit("lyrics://done", serde_json::json!({ "trackId": track_id }));
    });

    Ok(())
}

/// Fetch full lyrics text for a candidate (source-prefixed id).
#[tauri::command]
pub async fn lyrics_fetch(id: String) -> Result<LyricsContent, String> {
    tauri::async_runtime::spawn_blocking(move || lyrics::fetch(&id).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

/// Re-read one track's file state into DB (shared by lyrics ops).
fn rescan_track_row(state: &State<'_, AppState>, path: &Path) {
    if let Ok(mut guard) = state.db.lock() {
        if let Some(db) = guard.as_mut() {
            if let Ok(mut row) = scanner::read_track(path) {
                let meta = std::fs::metadata(path).ok();
                let file_size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let mtime = meta
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                row.path = path.to_string_lossy().to_string();
                let _ = db.upsert_track(&row, file_size, mtime);
            }
        }
    }
}

/// Save lyrics for a track. mode: "sidecar"（默认，外挂 .lrc）| "embed"（内嵌到标签）.
/// 库外文件只写文件，不入库。
#[tauri::command]
pub async fn lyrics_save(
    app: AppHandle,
    state: State<'_, AppState>,
    track_id: Option<i64>,
    path: Option<String>,
    lrc_id: String,
    mode: String,
) -> Result<String, String> {
    let (tid, path_buf) = resolve_lyrics_target(&state, track_id, path)?;

    let content = lyrics_fetch(lrc_id).await?;
    let text = lyrics::best_text(&content)
        .ok_or("该候选没有歌词内容")?
        .to_string();

    let desc = if mode == "embed" {
        crate::tagger::write_track(
            &path_buf,
            &[crate::scraper::FieldChange {
                field: "lyrics".into(),
                old: String::new(),
                new: text,
            }],
            None,
            "",
            "",
        )
        .map_err(|e| e.to_string())?;
        "已内嵌到文件标签".to_string()
    } else {
        // 用户显式保存候选 = 替换更好的歌词，允许覆盖已有 .lrc
        let dest = lyrics::write_sidecar(&path_buf, &text, true).map_err(|e| e.to_string())?;
        format!("已写入外挂歌词 {}", dest.display())
    };

    if let Some(id) = tid {
        rescan_track_row(&state, &path_buf);
        let _ = app.emit("library://changed", id);
    }
    Ok(desc)
}

/// Export embedded lyrics → sidecar .lrc（内嵌转外挂）.
#[tauri::command]
pub fn lyrics_export_sidecar(
    app: AppHandle,
    state: State<'_, AppState>,
    track_id: Option<i64>,
    path: Option<String>,
    overwrite: bool,
) -> Result<String, String> {
    let (tid, path_buf) = resolve_lyrics_target(&state, track_id, path)?;
    let text = lyrics::read_embedded(&path_buf)
        .map_err(|e| e.to_string())?
        .ok_or("文件里没有内嵌歌词")?;
    let dest = lyrics::write_sidecar(&path_buf, &text, overwrite).map_err(|e| e.to_string())?;
    if let Some(id) = tid {
        rescan_track_row(&state, &path_buf);
        let _ = app.emit("library://changed", id);
    }
    Ok(format!("已导出到 {}", dest.display()))
}

/// Embed sidecar .lrc → file tag（外挂转内嵌）.
#[tauri::command]
pub fn lyrics_embed_sidecar(
    app: AppHandle,
    state: State<'_, AppState>,
    track_id: Option<i64>,
    path: Option<String>,
) -> Result<String, String> {
    let (tid, path_buf) = resolve_lyrics_target(&state, track_id, path)?;
    let text = lyrics::read_sidecar(&path_buf).map_err(|e| e.to_string())?;

    crate::tagger::write_track(
        &path_buf,
        &[crate::scraper::FieldChange {
            field: "lyrics".into(),
            old: String::new(),
            new: text,
        }],
        None,
        "",
        "",
    )
    .map_err(|e| e.to_string())?;

    if let Some(id) = tid {
        rescan_track_row(&state, &path_buf);
        let _ = app.emit("library://changed", id);
    }
    Ok("已内嵌到文件标签（外挂 .lrc 保留未删）".into())
}

/// Read current lyrics for preview: embedded first, else sidecar.
#[tauri::command]
pub fn lyrics_current(
    state: State<'_, AppState>,
    track_id: Option<i64>,
    path: Option<String>,
) -> Result<serde_json::Value, String> {
    let (_tid, path_buf) = resolve_lyrics_target(&state, track_id, path)?;
    let embedded = lyrics::read_embedded(&path_buf).ok().flatten();
    let sidecar = lyrics::read_sidecar(&path_buf).ok();
    Ok(serde_json::json!({
        "embedded": embedded,
        "sidecar": sidecar,
    }))
}

// ���� scrape (MusicBrainz / Cover Art Archive) ������������������������������������������

use crate::scraper::{self, ApplyPlan, CatalogTrackDraft, FieldChange, ScrapeCandidate, TrackPlan};

/// Album scrape: search MusicBrainz releases.
#[tauri::command]
pub async fn scrape_search_album(
    album: String,
    artist: String,
) -> Result<Vec<ScrapeCandidate>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        scraper::search_releases(&album, &artist).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Single-track scrape: search MusicBrainz recordings.
#[tauri::command]
pub async fn scrape_search_track(
    title: String,
    artist: String,
) -> Result<Vec<ScrapeCandidate>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        scraper::search_recordings(&title, &artist).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Build field-diff plan: local track_ids vs chosen release/recording.
/// `release_mbid` is ScrapeCandidate.release_id (for album) or recording id.
/// `mode` = "album" | "track"
#[tauri::command]
pub async fn scrape_build_plan(
    state: State<'_, AppState>,
    release_mbid: String,
    track_ids: Vec<i64>,
    mode: String,
) -> Result<ApplyPlan, String> {
    let mut locals = Vec::new();
    {
        let mut guard = state.db.lock().map_err(|e| e.to_string())?;
        let Some(db) = guard.as_mut() else {
            return Err("尚未初始化库目录".into());
        };
        for id in &track_ids {
            if let Some(t) = db.get_track_by_id(*id).map_err(|e| e.to_string())? {
                locals.push(t);
            }
        }
    }

        // blocking network on purpose (rate-limited)
    tauri::async_runtime::spawn_blocking(move || {
        build_plan_inner(release_mbid, locals, mode).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn build_plan_inner(
    release_mbid: String,
    locals: Vec<TrackRow>,
    mode: String,
) -> anyhow::Result<ApplyPlan> {
    use crate::scraper::musicbrainz::{fetch_recording, fetch_release, title_similarity};

    fn field(field: &str, old: &str, new: &str) -> FieldChange {
        FieldChange {
            field: field.into(),
            old: old.into(),
            new: new.into(),
        }
    }

    let mut unmatched = Vec::new();
    let mut tracks_plans = Vec::new();

    if mode == "track" {
        let rec_id = release_mbid.clone();
        let rec = fetch_recording(&rec_id)?;
        for t in locals {
            let changes = vec![
                field("title", &t.title, &rec.title),
                field("artist", &t.artist, &rec.artist),
                field("musicbrainz_recording", "", &rec.id),
            ];
            tracks_plans.push(TrackPlan {
                track_id: t.id,
                path: t.path.clone(),
                display: t.title.clone(),
                matched_title: rec.title.clone(),
                changes,
            });
        }
        return Ok(ApplyPlan {
            candidate_id: rec.id.clone(),
            release_id: rec.id.clone(),
            candidate_label: format!("{} — {}", rec.artist, rec.title),
            tracks: tracks_plans,
            catalog_tracks: vec![CatalogTrackDraft {
                mbid: rec.id.clone(),
                release_mbid: String::new(),
                title: rec.title.clone(),
                artist: rec.artist.clone(),
                album: String::new(),
                album_artist: String::new(),
                year: String::new(),
                track_no: None,
                release_type: String::new(),
            }],
            unmatched,
        });
    }

    // album mode
    let detail = fetch_release(&release_mbid)?;
    let mut used = vec![false; detail.tracks.len()];
    let mut pairs: Vec<(usize, usize)> = Vec::new();

    for (li, local) in locals.iter().enumerate() {
        let mut best: Option<(usize, f64)> = None;
        for (ri, remote) in detail.tracks.iter().enumerate() {
            if used.get(ri).copied().unwrap_or(true) {
                continue;
            }
            let mut score = title_similarity(&local.title, &remote.title);
            if let Some(no) = local.track_no {
                if no == remote.position {
                    score += 0.5;
                }
            }
            if best.map(|(_, s)| score > s).unwrap_or(true) {
                best = Some((ri, score));
            }
        }
        if let Some((ri, score)) = best {
            if score >= 0.35 {
                used[ri] = true;
                pairs.push((li, ri));
            } else {
                unmatched.push(local.title.clone());
            }
        } else {
            unmatched.push(local.title.clone());
        }
    }

    for (li, ri) in pairs {
        let local = &locals[li];
        let remote = &detail.tracks[ri];
        // Always list every field (old → new) so the user can review before apply.
        let changes = vec![
            field("title", &local.title, &remote.title),
            field("artist", &local.artist, &remote.artist),
            field("album", &local.album, &detail.title),
            field("album_artist", &local.album_artist, &detail.album_artist),
            field("year", &local.year, &detail.year),
            field("release_type", &local.release_type, &detail.release_type),
            field(
                "track_no",
                &local.track_no.map(|n| n.to_string()).unwrap_or_default(),
                &remote.position.to_string(),
            ),
            field("musicbrainz_recording", "", &remote.recording_id),
            field("musicbrainz_release", "", &detail.release_id),
        ];

        tracks_plans.push(TrackPlan {
            track_id: local.id,
            path: local.path.clone(),
            display: local.title.clone(),
            matched_title: remote.title.clone(),
            changes,
        });
    }

    // Subset backup: adopting an album stores the WHOLE release tracklist into
    // local catalog, so other local tracks can field-match against it later.
    let catalog_tracks: Vec<CatalogTrackDraft> = detail
        .tracks
        .iter()
        .map(|t| CatalogTrackDraft {
            mbid: t.recording_id.clone(),
            release_mbid: detail.release_id.clone(),
            title: t.title.clone(),
            artist: t.artist.clone(),
            album: detail.title.clone(),
            album_artist: detail.album_artist.clone(),
            year: detail.year.clone(),
            track_no: Some(t.position),
            release_type: detail.release_type.clone(),
        })
        .collect();

    Ok(ApplyPlan {
        candidate_id: detail.release_id.clone(),
        release_id: detail.release_id,
        candidate_label: format!("{} — {}", detail.artist, detail.title),
        tracks: tracks_plans,
        catalog_tracks,
        unmatched,
    })
}

/// Save scrape result into local catalog — does NOT touch audio files. Text only;
/// covers are fetched separately via `catalog_fetch_cover`.
/// Stores the WHOLE adopted release tracklist (subset backup); each locally matched
/// track is explicitly linked to its catalog row by recording MBID.
#[tauri::command]
pub async fn catalog_save(
    state: State<'_, AppState>,
    plan: ApplyPlan,
) -> Result<Vec<i64>, String> {
    let mut ids = Vec::new();
    {
        let guard = state.db.lock().map_err(|e| e.to_string())?;
        let Some(db) = guard.as_ref() else {
            return Err("尚未初始化库目录".into());
        };

        // 1. store every catalog track of the adopted release/recording
        let mut id_by_mbid: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();
        for ct in &plan.catalog_tracks {
            let row = crate::library::CatalogRow {
                id: 0,
                source: "musicbrainz".into(),
                kind: "track".into(),
                mbid: ct.mbid.clone(),
                release_mbid: ct.release_mbid.clone(),
                title: ct.title.clone(),
                artist: ct.artist.clone(),
                album: ct.album.clone(),
                album_artist: ct.album_artist.clone(),
                year: ct.year.clone(),
                track_no: ct.track_no,
                release_type: ct.release_type.clone(),
                cover_path: None,
                created_at: String::new(),
            };
            let id = db.insert_catalog(&row).map_err(|e| e.to_string())?;
            ids.push(id);
            if !ct.mbid.is_empty() {
                id_by_mbid.insert(ct.mbid.clone(), id);
            }
        }

        // 2. explicitly link the locally-matched tracks (recording MBID ↔ catalog row)
        for tp in &plan.tracks {
            let rec = tp
                .changes
                .iter()
                .find(|c| c.field == "musicbrainz_recording")
                .map(|c| c.new.clone())
                .unwrap_or_default();
            if let Some(cat_id) = id_by_mbid.get(&rec) {
                db.link_track_catalog(tp.track_id, *cat_id)
                    .map_err(|e| e.to_string())?;
            }
        }

        // 3. field-match the rest of the library against what's now in catalog
        let _ = db.auto_match_unlinked();
    }
    Ok(ids)
}

/// Compare local file tags vs linked catalog row (empty fields if no link).
#[tauri::command]
pub fn catalog_compare(
    state: State<'_, AppState>,
    track_id: i64,
) -> Result<serde_json::Value, String> {
    let guard = require_db(&state)?;
    let db = db_ref(&guard)?;
    let track = db
        .get_track_by_id(track_id)
        .map_err(|e| e.to_string())?
        .ok_or("曲目不存在")?;
    let catalog = db
        .find_catalog_for_track(track_id)
        .map_err(|e| e.to_string())?;

    let changes = if let Some(cat) = &catalog {
        vec![
            crate::scraper::FieldChange {
                field: "title".into(),
                old: track.title.clone(),
                new: cat.title.clone(),
            },
            crate::scraper::FieldChange {
                field: "artist".into(),
                old: track.artist.clone(),
                new: cat.artist.clone(),
            },
            crate::scraper::FieldChange {
                field: "album".into(),
                old: track.album.clone(),
                new: cat.album.clone(),
            },
            crate::scraper::FieldChange {
                field: "album_artist".into(),
                old: track.album_artist.clone(),
                new: cat.album_artist.clone(),
            },
            crate::scraper::FieldChange {
                field: "year".into(),
                old: track.year.clone(),
                new: cat.year.clone(),
            },
            crate::scraper::FieldChange {
                field: "release_type".into(),
                old: track.release_type.clone(),
                new: cat.release_type.clone(),
            },
            crate::scraper::FieldChange {
                field: "track_no".into(),
                old: track
                    .track_no
                    .map(|n| n.to_string())
                    .unwrap_or_default(),
                new: cat
                    .track_no
                    .map(|n| n.to_string())
                    .unwrap_or_default(),
            },
            crate::scraper::FieldChange {
                field: "musicbrainz_recording".into(),
                old: track.mb_recording_mbid.clone(),
                new: cat.mbid.clone(),
            },
        ]
    } else {
        Vec::new()
    };

    Ok(serde_json::json!({
        "track": track,
        "catalog": catalog,
        "changes": changes,
        "cover_data": cover_data_of(catalog.as_ref()),
    }))
}

/// catalog 缓存封面 → data URL（未刮取返回 None）。
fn cover_data_of(catalog: Option<&crate::library::CatalogRow>) -> Option<String> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    let path = catalog?.cover_path.as_ref()?;
    let bytes = std::fs::read(path).ok()?;
    Some(format!("data:image/jpeg;base64,{}", B64.encode(bytes)))
}

/// 多源搜索封面候选（CAA / iTunes / 网易云 / QQ音乐，并发），返回缩略图+大图 URL。
/// 查询词优先取关联 catalog 的专辑/歌手，退回文件标签。
#[tauri::command]
pub async fn cover_search(
    state: State<'_, AppState>,
    track_id: i64,
) -> Result<Vec<crate::scraper::CoverCandidate>, String> {
    let (release_mbid, album, artist) = {
        let guard = require_db(&state)?;
        let db = db_ref(&guard)?;
        let track = db
            .get_track_by_id(track_id)
            .map_err(|e| e.to_string())?
            .ok_or("曲目不存在")?;
        let cat = db.find_catalog_for_track(track_id).map_err(|e| e.to_string())?;
        match cat {
            Some(c) => {
                let album = if !c.album.is_empty() { c.album } else { track.album.clone() };
                let artist = if !c.artist.is_empty() {
                    c.artist
                } else if !track.artist.is_empty() {
                    track.artist.clone()
                } else {
                    track.album_artist.clone()
                };
                (c.release_mbid, album, artist)
            }
            None => (track.mb_release_mbid.clone(), track.album.clone(), track.artist.clone()),
        }
    };

    tauri::async_runtime::spawn_blocking(move || {
        crate::scraper::search_covers(&release_mbid, &album, &artist)
    })
    .await
    .map_err(|e| e.to_string())
}

/// 采纳封面候选：下载大图 → `<库>/covers/` → 更新 catalog 封面引用。不改音频文件。
/// 返回封面 data URL（对比面板直接显示）。
#[tauri::command]
pub async fn cover_apply(
    state: State<'_, AppState>,
    track_id: i64,
    url: String,
) -> Result<String, String> {
    let (catalog_id, release_mbid, library_root) = {
        let guard = require_db(&state)?;
        let db = db_ref(&guard)?;
        let cat = db.find_catalog_for_track(track_id).map_err(|e| e.to_string())?;
        let root = state
            .settings
            .lock()
            .map_err(|e| e.to_string())?
            .library_root
            .clone()
            .ok_or("尚未初始化库目录")?;
        (
            cat.as_ref().map(|c| c.id),
            cat.map(|c| c.release_mbid).unwrap_or_default(),
            PathBuf::from(root),
        )
    };

    let filename = if !release_mbid.is_empty() {
        format!("{release_mbid}.jpg")
    } else if let Some(id) = catalog_id {
        format!("cat-{id}.jpg")
    } else {
        format!("track-{track_id}.jpg")
    };

    let (bytes, path_str) = tauri::async_runtime::spawn_blocking(move || {
        let bytes = crate::scraper::download_image(&url).map_err(|e| e.to_string())?;
        let dir = library_root.join("covers");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(filename);
        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        Ok::<_, String>((bytes, path.to_string_lossy().to_string()))
    })
    .await
    .map_err(|e| e.to_string())??;

    {
        let guard = require_db(&state)?;
        let db = db_ref(&guard)?;
        if let Some(id) = catalog_id {
            db.set_catalog_cover_by_id(id, &path_str)
                .map_err(|e| e.to_string())?;
        }
        if !release_mbid.is_empty() {
            db.set_catalog_cover(&release_mbid, &path_str)
                .map_err(|e| e.to_string())?;
        }
    }

    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    Ok(format!("data:image/jpeg;base64,{}", B64.encode(bytes)))
}

/// Try to link a track to an existing catalog row (mbid / title+artist+album).
#[tauri::command]
pub fn catalog_match_one(
    state: State<'_, AppState>,
    track_id: i64,
) -> Result<Option<i64>, String> {
    let guard = require_db(&state)?;
    let db = db_ref(&guard)?;
    let track = db
        .get_track_by_id(track_id)
        .map_err(|e| e.to_string())?
        .ok_or("曲目不存在")?;
    let cat = db
        .find_catalog_fuzzy(&track)
        .map_err(|e| e.to_string())?;
    if let Some(c) = cat {
        db.link_track_catalog(track_id, c.id).map_err(|e| e.to_string())?;
        return Ok(Some(c.id));
    }
    Ok(None)
}

/// Batch field-match all unlinked tracks against local catalog.
#[tauri::command]
pub fn catalog_match_all(state: State<'_, AppState>) -> Result<usize, String> {
    let guard = require_db(&state)?;
    let db = db_ref(&guard)?;
    db.auto_match_unlinked().map_err(|e| e.to_string())
}

/// Write user-edited tag fields into the audio file, refresh the tracks row,
/// then try field-match against local catalog (fixing title/artist may unlock a link).
/// Empty values never overwrite tags.
#[tauri::command]
pub fn track_write_tags(
    state: State<'_, AppState>,
    track_id: i64,
    changes: Vec<FieldChange>,
) -> Result<i64, String> {
    let path = {
        let guard = require_db(&state)?;
        let db = db_ref(&guard)?;
        let track = db
            .get_track_by_id(track_id)
            .map_err(|e| e.to_string())?
            .ok_or("曲目不存在")?;
        track.path.clone()
    };

    let writable: Vec<FieldChange> = changes
        .into_iter()
        .filter(|ch| !ch.new.trim().is_empty())
        .collect();
    if writable.is_empty() {
        return Ok(track_id);
    }

    let path_buf = PathBuf::from(&path);
    crate::tagger::write_track(&path_buf, &writable, None, "", "").map_err(|e| e.to_string())?;

    {
        let mut guard = state.db.lock().map_err(|e| e.to_string())?;
        let Some(db) = guard.as_mut() else {
            return Err("尚未初始化库目录".into());
        };
        if let Ok(mut row) = scanner::read_track(Path::new(&path)) {
            let meta = std::fs::metadata(&path).ok();
            let file_size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let mtime = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            row.path = path.clone();
            let _ = db.upsert_track(&row, file_size, mtime);
        }
        // 修正字段后立刻尝试关联 catalog
        if let Ok(Some(row)) = db.get_track_by_id(track_id) {
            if row.catalog_id.is_none() || row.catalog_id == Some(0) {
                if let Ok(Some(c)) = db.find_catalog_fuzzy(&row) {
                    let _ = db.link_track_catalog(track_id, c.id);
                }
            }
        }
    }
    Ok(track_id)
}

/// Write selected catalog fields into the audio file (tags + optional cover from covers/).
/// Only fields listed in `fields` are written; empty catalog values never overwrite tags.
#[tauri::command]
pub fn catalog_apply_to_track(
    state: State<'_, AppState>,
    track_id: i64,
    fields: Vec<String>,
    write_cover: bool,
) -> Result<i64, String> {
    let (path, catalog, root) = {
        let guard = require_db(&state)?;
        let db = db_ref(&guard)?;
        let track = db
            .get_track_by_id(track_id)
            .map_err(|e| e.to_string())?
            .ok_or("曲目不存在")?;
        let cat = db
            .find_catalog_for_track(track_id)
            .map_err(|e| e.to_string())?
            .ok_or("未关联 catalog，请先刮削或自动匹配")?;
        let root = state
            .settings
            .lock()
            .map_err(|e| e.to_string())?
            .library_root
            .clone()
            .ok_or("尚未初始化库目录")?;
        (track.path.clone(), cat, root)
    };

    let wanted = |f: &str| fields.iter().any(|x| x == f);
    let mut changes: Vec<crate::scraper::FieldChange> = Vec::new();
    let mut push = |field: &str, new: &str| {
        // Only selected fields with a non-empty catalog value are written —
        // an empty value must never wipe an existing tag.
        if wanted(field) && !new.trim().is_empty() {
            changes.push(crate::scraper::FieldChange {
                field: field.into(),
                old: String::new(),
                new: new.into(),
            });
        }
    };

    push("title", &catalog.title);
    push("artist", &catalog.artist);
    push("album", &catalog.album);
    push("album_artist", &catalog.album_artist);
    push("year", &catalog.year);
    push("release_type", &catalog.release_type);
    push("musicbrainz_recording", &catalog.mbid);
    push("musicbrainz_release", &catalog.release_mbid);
    if let Some(n) = catalog.track_no {
        push("track_no", &n.to_string());
    }

    if changes.is_empty() && !write_cover {
        return Ok(track_id);
    }

    let path = PathBuf::from(&path);

    let cover: Option<Vec<u8>> = if write_cover {
        catalog
            .cover_path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .or_else(|| {
                let rel = catalog.release_mbid.as_str();
                if rel.is_empty() {
                    None
                } else {
                    std::fs::read(Path::new(&root).join("covers").join(format!("{rel}.jpg"))).ok()
                }
            })
    } else {
        None
    };

    crate::tagger::write_track(&path, &changes, cover.as_deref(), "", "")
        .map_err(|e| e.to_string())?;

    // refresh row
    {
        let mut guard = state.db.lock().map_err(|e| e.to_string())?;
        let Some(db) = guard.as_mut() else {
            return Err("尚未初始化库目录".into());
        };
        if let Ok(mut row) = scanner::read_track(Path::new(&path)) {
            let meta = std::fs::metadata(&path).ok();
            let file_size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let mtime = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            row.path = path.to_string_lossy().to_string();
            let _ = db.upsert_track(&row, file_size, mtime);
        }
    }
    Ok(track_id)
}

// ── playlists (m3u8) ────────────────────────────────────────────────

fn require_library_root(state: &State<'_, AppState>) -> Result<String, String> {
    state
        .settings
        .lock()
        .map_err(|e| e.to_string())?
        .library_root
        .clone()
        .ok_or_else(|| "尚未初始化库目录".to_string())
}

/// 歌单详情（带 DB 富化；无 DB 也能出列表，仅展示字段退化）。
fn playlist_detail_of(root: &str, name: &str, state: &State<'_, AppState>) -> Result<PlaylistDetail, String> {
    let parsed = playlists::read_playlist(Path::new(root), name)?;
    let guard = state.db.lock().map_err(|e| e.to_string())?;
    Ok(playlists::to_entries(Path::new(root), name, &parsed, guard.as_ref()))
}

/// 列出 `<库>/playlists/*.m3u8`。
#[tauri::command]
pub fn playlist_list(state: State<'_, AppState>) -> Result<Vec<PlaylistSummary>, String> {
    let root = require_library_root(&state)?;
    playlists::list_playlists(Path::new(&root))
}

/// 新建歌单并一次写入条目（items 可空 = 空歌单）。
#[tauri::command]
pub fn playlist_create(
    state: State<'_, AppState>,
    name: String,
    items: Vec<PlaylistAddItem>,
) -> Result<PlaylistDetail, String> {
    let root = require_library_root(&state)?;
    playlists::create(Path::new(&root), &name, &items)?;
    playlist_detail_of(&root, &playlists::validate_name(&name)?, &state)
}

#[tauri::command]
pub fn playlist_rename(
    state: State<'_, AppState>,
    name: String,
    new_name: String,
) -> Result<PlaylistSummary, String> {
    let root = require_library_root(&state)?;
    playlists::rename(Path::new(&root), &name, &new_name)?;
    let name = playlists::validate_name(&new_name)?;
    let parsed = playlists::read_playlist(Path::new(&root), &name)?;
    Ok(PlaylistSummary {
        name,
        track_count: parsed.len(),
        total_ms: parsed.iter().map(|x| x.duration_ms).sum(),
    })
}

#[tauri::command]
pub fn playlist_delete(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let root = require_library_root(&state)?;
    playlists::delete(Path::new(&root), &name)
}

#[tauri::command]
pub fn playlist_get(state: State<'_, AppState>, name: String) -> Result<PlaylistDetail, String> {
    let root = require_library_root(&state)?;
    playlist_detail_of(&root, &name, &state)
}

/// 追加条目（按绝对路径去重跳过）。
#[tauri::command]
pub fn playlist_add_tracks(
    state: State<'_, AppState>,
    name: String,
    items: Vec<PlaylistAddItem>,
) -> Result<PlaylistDetail, String> {
    let root = require_library_root(&state)?;
    playlists::add_tracks(Path::new(&root), &name, &items)?;
    playlist_detail_of(&root, &name, &state)
}

#[tauri::command]
pub fn playlist_remove_track(
    state: State<'_, AppState>,
    name: String,
    index: usize,
) -> Result<PlaylistDetail, String> {
    let root = require_library_root(&state)?;
    playlists::remove_track(Path::new(&root), &name, index)?;
    playlist_detail_of(&root, &name, &state)
}

#[tauri::command]
pub fn playlist_move_track(
    state: State<'_, AppState>,
    name: String,
    from_index: usize,
    to_index: usize,
) -> Result<PlaylistDetail, String> {
    let root = require_library_root(&state)?;
    playlists::move_track(Path::new(&root), &name, from_index, to_index)?;
    playlist_detail_of(&root, &name, &state)
}
