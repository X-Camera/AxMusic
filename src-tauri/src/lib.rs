//! AxMusic — local music player with library management.

mod commands;
mod library;
mod paths;
mod player;
mod scanner;

use std::sync::Mutex;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|_app| {
            let data_root = paths::ensure_data_root();
            eprintln!("[AxMusic] data_root = {}", data_root.display());

            let db = library::LibraryDb::open_default()?;
            let player = player::Player::new()?;

            let state = commands::AppState {
                db: Mutex::new(db),
                player: Mutex::new(player),
                scanning: Mutex::new(false),
            };
            _app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_paths,
            commands::get_app_info,
            commands::get_library_root,
            commands::init_library,
            commands::get_tracks,
            commands::get_albums,
            commands::get_album_tracks,
            commands::get_track_count,
            commands::refresh_scan,
            commands::include_in_library,
            commands::get_player_state,
            commands::play_file,
            commands::play_queue,
            commands::player_play,
            commands::player_pause,
            commands::player_toggle,
            commands::player_next,
            commands::player_prev,
            commands::player_seek,
            commands::player_set_volume,
            commands::list_dir_audio,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AxMusic");
}
