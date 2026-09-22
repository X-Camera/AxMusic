//! Tauri IPC commands.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::library::{AlbumCard, LibraryDb, LibraryRoot, TrackFilter, TrackRow};
use crate::player::{Player, PlayerSnapshot, QueueItem, TrackInfo};
use crate::settings::AppSettings;
use crate::{scanner, settings};

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
pub fn get_track_count(state: State<'_, AppState>) -> Result<i64, String> {
    let guard = require_db(&state)?;
    let db = db_ref(&guard)?;
    db.track_count().map_err(|e| e.to_string())
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
    let _ = info;
    let snap = player.snapshot();
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
    player
        .play_queue(items, start)
        .map_err(|e| e.to_string())?;
    let snap = player.snapshot();
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

/// Fan out a lyrics search to ALL sources concurrently.
/// Returns immediately; each source pushes its batch via `lyrics://batch`
/// ({track_id, source, items}) and a final `lyrics://done` ({track_id})
/// fires when all sources have reported.
#[tauri::command]
pub async fn lyrics_search(
    app: AppHandle,
    state: State<'_, AppState>,
    track_id: i64,
) -> Result<(), String> {
    let track = {
        let guard = require_db(&state)?;
        let db = db_ref(&guard)?;
        db.get_track_by_id(track_id)
            .map_err(|e| e.to_string())?
            .ok_or("曲目不存在")?
    };
    let title = track.title.clone();
    let artist = track.artist.clone();
    let album = track.album.clone();

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

        spawn_source!(lyrics::SOURCE_LRCLIB, lyrics::lrclib::search);
        spawn_source!(lyrics::SOURCE_NETEASE, lyrics::netease::search);
        spawn_source!(lyrics::SOURCE_QQ, lyrics::qqmusic::search);
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
#[tauri::command]
pub async fn lyrics_save(
    app: AppHandle,
    state: State<'_, AppState>,
    track_id: i64,
    lrc_id: String,
    mode: String,
) -> Result<String, String> {
    let path = {
        let guard = require_db(&state)?;
        let db = db_ref(&guard)?;
        db.get_track_by_id(track_id)
            .map_err(|e| e.to_string())?
            .ok_or("曲目不存在")?
            .path
    };
    let path_buf = PathBuf::from(&path);

    let content = lyrics_fetch(lrc_id).await?;
    let text = lyrics::best_text(&content)
        .ok_or("该候选没有歌词内容")?
        .to_string();

    let desc = if mode == "embed" {
        let backup_root = crate::paths::data_root().to_path_buf();
        crate::tagger::backup_file(&path_buf, &backup_root).map_err(|e| e.to_string())?;
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
        let dest = lyrics::write_sidecar(&path_buf, &text, false).map_err(|e| e.to_string())?;
        format!("已写入外挂歌词 {}", dest.display())
    };

    rescan_track_row(&state, &path_buf);
    let _ = app.emit("library://changed", track_id);
    Ok(desc)
}

/// Export embedded lyrics → sidecar .lrc（内嵌转外挂）.
#[tauri::command]
pub fn lyrics_export_sidecar(
    app: AppHandle,
    state: State<'_, AppState>,
    track_id: i64,
    overwrite: bool,
) -> Result<String, String> {
    let path = {
        let guard = require_db(&state)?;
        let db = db_ref(&guard)?;
        db.get_track_by_id(track_id)
            .map_err(|e| e.to_string())?
            .ok_or("曲目不存在")?
            .path
    };
    let path_buf = PathBuf::from(&path);
    let text = lyrics::read_embedded(&path_buf)
        .map_err(|e| e.to_string())?
        .ok_or("文件里没有内嵌歌词")?;
    let dest = lyrics::write_sidecar(&path_buf, &text, overwrite).map_err(|e| e.to_string())?;
    rescan_track_row(&state, &path_buf);
    let _ = app.emit("library://changed", track_id);
    Ok(format!("已导出到 {}", dest.display()))
}

/// Embed sidecar .lrc → file tag（外挂转内嵌）.
#[tauri::command]
pub fn lyrics_embed_sidecar(
    app: AppHandle,
    state: State<'_, AppState>,
    track_id: i64,
) -> Result<String, String> {
    let path = {
        let guard = require_db(&state)?;
        let db = db_ref(&guard)?;
        db.get_track_by_id(track_id)
            .map_err(|e| e.to_string())?
            .ok_or("曲目不存在")?
            .path
    };
    let path_buf = PathBuf::from(&path);
    let text = lyrics::read_sidecar(&path_buf).map_err(|e| e.to_string())?;

    let backup_root = crate::paths::data_root().to_path_buf();
    crate::tagger::backup_file(&path_buf, &backup_root).map_err(|e| e.to_string())?;
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

    rescan_track_row(&state, &path_buf);
    let _ = app.emit("library://changed", track_id);
    Ok("已内嵌到文件标签（外挂 .lrc 保留未删）".into())
}

/// Read current lyrics for preview: embedded first, else sidecar.
#[tauri::command]
pub fn lyrics_current(
    state: State<'_, AppState>,
    track_id: i64,
) -> Result<serde_json::Value, String> {
    let path = {
        let guard = require_db(&state)?;
        let db = db_ref(&guard)?;
        db.get_track_by_id(track_id)
            .map_err(|e| e.to_string())?
            .ok_or("曲目不存在")?
            .path
    };
    let path_buf = PathBuf::from(&path);
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
    write_cover: bool,
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
        build_plan_inner(release_mbid, locals, mode, write_cover).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn build_plan_inner(
    release_mbid: String,
    locals: Vec<TrackRow>,
    mode: String,
    write_cover: bool,
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
            cover_will_write: false,
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
        cover_will_write: write_cover,
        unmatched,
    })
}

/// Save scrape result into local catalog + `<library>/covers/` — does NOT touch audio files.
/// Stores the WHOLE adopted release tracklist (subset backup); each locally matched
/// track is explicitly linked to its catalog row by recording MBID.
#[tauri::command]
pub async fn catalog_save(
    state: State<'_, AppState>,
    plan: ApplyPlan,
    fetch_cover: bool,
) -> Result<Vec<i64>, String> {
    let root = state
        .settings
        .lock()
        .map_err(|e| e.to_string())?
        .library_root
        .clone()
        .ok_or("尚未初始化库目录")?;
    let library_root = PathBuf::from(root);
    let release_id = plan.release_id.clone();

    let cover = if fetch_cover {
        tauri::async_runtime::spawn_blocking({
            let release_id = release_id.clone();
            let library_root = library_root.clone();
            move || -> Option<String> {
                let bytes = crate::scraper::fetch_front_cover(&release_id).ok()?;
                let dir = library_root.join("covers");
                std::fs::create_dir_all(&dir).ok()?;
                let path = dir.join(format!("{release_id}.jpg"));
                std::fs::write(&path, &bytes).ok()?;
                Some(path.to_string_lossy().to_string())
            }
        })
        .await
        .unwrap_or(None)
    } else {
        None
    };

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
                cover_path: cover.clone(),
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
    }))
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

    let backup_root = crate::paths::data_root().to_path_buf();
    let path = PathBuf::from(&path);
    crate::tagger::backup_file(&path, &backup_root).map_err(|e| e.to_string())?;

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
