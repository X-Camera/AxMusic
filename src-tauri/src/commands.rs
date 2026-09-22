//! Tauri IPC commands.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::library::{AlbumCard, LibraryDb, LibraryRoot, TrackFilter, TrackRow};
use crate::player::{Player, PlayerSnapshot, QueueItem, TrackInfo};
use crate::scanner;

pub struct AppState {
    pub db: Mutex<LibraryDb>,
    pub player: Mutex<Player>,
    pub scanning: Mutex<bool>,
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
    PathsInfo {
        data_root: crate::paths::data_root().to_string_lossy().to_string(),
        portable: crate::paths::is_portable(),
        db_path: crate::paths::db_path().to_string_lossy().to_string(),
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
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_library_root().map_err(|e| e.to_string())
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

    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.set_library_root(&dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_tracks(
    state: State<'_, AppState>,
    filter: Option<TrackFilter>,
) -> Result<Vec<TrackRow>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_tracks(&filter.unwrap_or_default())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_albums(state: State<'_, AppState>) -> Result<Vec<AlbumCard>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_albums().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_album_tracks(
    state: State<'_, AppState>,
    album: String,
    album_artist: String,
) -> Result<Vec<TrackRow>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.tracks_of_album(&album, &album_artist)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_track_count(state: State<'_, AppState>) -> Result<i64, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.track_count().map_err(|e| e.to_string())
}

/// Incremental scan. Progress events: `scan://progress`, `scan://done`.
#[tauri::command]
pub async fn refresh_scan(app: AppHandle, state: State<'_, AppState>) -> Result<ScanResult, String> {
    let root = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.get_library_root()
            .map_err(|e| e.to_string())?
            .ok_or("尚未初始化库目录")?
            .path
    };

    {
        let mut flag = state.scanning.lock().map_err(|e| e.to_string())?;
        if *flag {
            return Err("扫描已在进行中".into());
        }
        *flag = true;
    }

    let root_path = PathBuf::from(&root);
    let result = std::thread::spawn({
        let db_path = crate::paths::db_path();
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
    let root = db
        .get_library_root()
        .map_err(|e| e.to_string())?
        .ok_or("尚未初始化库目录")?
        .path;
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
