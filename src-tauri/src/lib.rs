//! AxMusic — local music player with library management.

mod commands;
mod folder_meta;
mod import;
mod library;
mod listen_history;
mod lyrics;
mod net_util;
mod paths;
mod archive;
mod play_session;
mod play_ui;
mod player;
mod playlists;
mod scanner;
mod scraper;
mod settings;
mod shell_menu;
mod tagger;
mod taskbar;
mod tray;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 必须最先注册：二次启动时唤醒已有实例并退出，禁止多开
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // 右键菜单拉起：按动作处理文件，仅「播放」时唤醒主窗口
            if let Some((action, paths)) = shell_menu::parse_ctx_args(&argv) {
                shell_menu::dispatch_ctx(app, action, paths);
                if action == shell_menu::CtxAction::Play {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.unminimize();
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
                return;
            }
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_root = paths::ensure_data_root();
            eprintln!("[AxMusic] data_root = {}", data_root.display());

            let app_settings = settings::load();
            // 全局 library_root：歌词路径解析等无 AppState 场景使用
            paths::set_library_root(
                app_settings
                    .library_root
                    .as_deref()
                    .filter(|r| Path::new(r).is_dir())
                    .map(PathBuf::from),
            );
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
            player.set_shuffle(app_settings.shuffle);
            player.set_replaygain_mode(app_settings.replaygain_mode);
            player.set_repeat(match app_settings.repeat {
                settings::RepeatMode::Off => player::RepeatMode::Off,
                settings::RepeatMode::All => player::RepeatMode::All,
                settings::RepeatMode::One => player::RepeatMode::One,
            });

            // 恢复上次播放列表（队列 + 当前曲），从头暂停不自动播
            if let Some(session) = play_session::load() {
                if !session.items.is_empty() {
                    let start = session
                        .queue_index
                        .unwrap_or(0)
                        .min(session.items.len().saturating_sub(1));
                    let _ = player.restore_session(session.items, start, 0);
                } else if let Some(track) = session.track {
                    let items = vec![track];
                    let _ = player.restore_session(items, 0, 0);
                }
            }

            // 频谱动效数据管线：消费 PCM 分接 → ~30Hz 推 viz://spectrum（定向主窗口）
            if let Some(win) = app.get_webview_window("main") {
                if let Some(cons) = player.engine.take_viz_consumer() {
                    player::viz::spawn_viz_thread(
                        win,
                        cons,
                        player.engine.viz_active_handle(),
                        player.engine.viz_gen_handle(),
                        player.engine.output_sample_rate(),
                    );
                }
            }

            let listen_hub = match listen_history::ListenHub::open(
                app_settings
                    .library_root
                    .as_deref()
                    .map(Path::new)
                    .filter(|r| r.is_dir()),
            ) {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("[AxMusic] 听歌历史库打开失败（不影响播放）: {e}");
                    // 听歌统计失败不拦启动；用内存空库占位
                    listen_history::ListenHub::open_in_memory()?
                }
            };
            let state = commands::AppState {
                db: Mutex::new(db),
                player: Mutex::new(player),
                scanning: Mutex::new(false),
                volume_persist: Mutex::new(commands::VolumePersist {
                    saved: app_settings.volume,
                    pending: app_settings.volume,
                    timer_running: false,
                }),
                settings: Mutex::new(app_settings),
                listen: Mutex::new(listen_hub),
                listen_tracker: Mutex::new(listen_history::ListenTracker::new()),
            };
            app.manage(state);
            tray::init(app)?;
            taskbar::init(app)?;
            // 首次启动可能带 --ctx-play / --ctx-enqueue（右键菜单冷启动）
            // args_os：Windows 路径可能非严格 UTF-8，避免 args() 直接 panic
            let handle = app.handle().clone();
            let argv: Vec<String> = std::env::args_os()
                .map(|s| s.to_string_lossy().into_owned())
                .collect();
            shell_menu::dispatch_from_args(&handle, &argv);
            shell_menu::flush_pending(&handle);
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
            commands::open_url,
            commands::resolve_window_close,
            commands::enter_true_fullscreen,
            commands::exit_true_fullscreen,
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
            commands::viz_set_active,
            commands::play_file,
            commands::play_queue,
            commands::player_enqueue,
            commands::player_remove_at,
            commands::player_play,
            commands::player_pause,
            commands::player_toggle,
            commands::player_next,
            commands::player_prev,
            commands::player_seek,
            commands::player_set_volume,
            commands::player_set_shuffle,
            commands::player_set_repeat,
            commands::list_dir_audio,
            commands::list_dir_tree,
            commands::list_dir_audio_recursive,
            commands::resolve_drop_paths,
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
            commands::replaygain_analyze,
            commands::replaygain_tags,
            commands::replaygain_write,
            commands::lyrics_search,
            commands::lyrics_fetch,
            commands::set_pending_lyrics_target,
            commands::take_pending_lyrics_target,
            commands::lyrics_save,
            commands::lyrics_export_sidecar,
            commands::lyrics_embed_sidecar,
            commands::lyrics_current,
            commands::archive_check_batch,
            commands::archive_normalize,
            commands::archive_normalize_issue,
            commands::library_root_scan,
            commands::library_root_organize,
            commands::library_import_preview,
            commands::library_import_run,
            commands::playlist_list,
            commands::playlist_create,
            commands::playlist_rename,
            commands::playlist_delete,
            commands::playlist_get,
            commands::playlist_add_tracks,
            commands::playlist_remove_track,
            commands::playlist_move_track,
            commands::playlist_clean_missing,
            commands::favorite_paths,
            commands::favorite_toggle,
            commands::listen_summary,
            commands::listen_top,
            commands::listen_recent,
            commands::listen_daily,
            commands::listen_hour_hist,
            commands::shell_menu_status,
            commands::shell_menu_register,
            commands::shell_menu_unregister,
        ])
        .build(tauri::generate_context!())
        .expect("error while building AxMusic")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<commands::AppState>() {
                    commands::persist_play_session(&state);
                    commands::flush_listen_on_exit(&state);
                }
            }
        });
}
