//! AxMusic — local music player with library management.

mod commands;
mod folder_meta;
mod library;
mod lyrics;
mod paths;
mod player;
mod playlists;
mod scanner;
mod scraper;
mod settings;
mod tagger;
mod tray;

use std::path::Path;
use std::sync::Mutex;

use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_root = paths::ensure_data_root();
            eprintln!("[AxMusic] data_root = {}", data_root.display());

            let app_settings = settings::load();
            let db = match app_settings.library_root.as_deref() {
                Some(root) if Path::new(root).is_dir() => {
                    let db_path = paths::library_db_path(Path::new(root));
                    eprintln!("[AxMusic] library_db = {}", db_path.display());
                    Some(library::LibraryDb::open(&db_path)?)
                }
                _ => None,
            };

            let mut player = player::Player::new()?;
            let boot_vol = if app_settings.restore_volume {
                app_settings.volume.clamp(0.0, 1.0)
            } else {
                0.8
            };
            player.engine.set_volume_f32(boot_vol);
            player.set_play_mode(match app_settings.play_mode {
                settings::PlayMode::Sequential => player::PlayMode::Sequential,
                settings::PlayMode::Shuffle => player::PlayMode::Shuffle,
                settings::PlayMode::RepeatOne => player::PlayMode::RepeatOne,
            });

            let state = commands::AppState {
                db: Mutex::new(db),
                player: Mutex::new(player),
                scanning: Mutex::new(false),
                settings: Mutex::new(app_settings),
            };
            app.manage(state);
            tray::init(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let behavior = window
                    .app_handle()
                    .try_state::<commands::AppState>()
                    .and_then(|s| s.settings.lock().ok().map(|g| g.close_behavior))
                    .unwrap_or_default();
                match behavior {
                    settings::CloseBehavior::Exit => {
                        // 允许关闭；窗口销毁后应用退出
                    }
                    settings::CloseBehavior::Tray => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    settings::CloseBehavior::Ask => {
                        api.prevent_close();
                        let _ = window.emit("app://close-requested", ());
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_paths,
            commands::get_app_info,
            commands::get_settings,
            commands::update_settings,
            commands::open_path,
            commands::resolve_window_close,
            commands::get_library_root,
            commands::init_library,
            commands::get_tracks,
            commands::get_albums,
            commands::get_album_tracks,
            commands::get_artists,
            commands::get_artist_albums,
            commands::get_artist_tracks,
            commands::get_track_count,
            commands::library_stats,
            commands::track_cover_thumb,
            commands::track_media_info,
            commands::refresh_scan,
            commands::is_in_library,
            commands::get_track_by_path,
            commands::include_in_library,
            commands::get_player_state,
            commands::play_file,
            commands::play_queue,
            commands::player_enqueue,
            commands::player_play,
            commands::player_pause,
            commands::player_toggle,
            commands::player_next,
            commands::player_prev,
            commands::player_seek,
            commands::player_set_volume,
            commands::player_set_play_mode,
            commands::list_dir_audio,
            commands::list_dir_tree,
            commands::list_dir_audio_recursive,
            commands::folder_meta_lookup,
            commands::folder_meta_read,
            commands::scrape_search_album,
            commands::scrape_search_track,
            commands::scrape_build_plan,
            commands::catalog_save,
            commands::catalog_compare,
            commands::cover_search,
            commands::cover_apply,
            commands::catalog_match_one,
            commands::catalog_match_all,
            commands::catalog_apply_to_track,
            commands::track_write_tags,
            commands::lyrics_search,
            commands::lyrics_fetch,
            commands::lyrics_save,
            commands::lyrics_export_sidecar,
            commands::lyrics_embed_sidecar,
            commands::lyrics_current,
            commands::playlist_list,
            commands::playlist_create,
            commands::playlist_rename,
            commands::playlist_delete,
            commands::playlist_get,
            commands::playlist_add_tracks,
            commands::playlist_remove_track,
            commands::playlist_move_track,
            commands::favorite_paths,
            commands::favorite_toggle,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AxMusic");
}
