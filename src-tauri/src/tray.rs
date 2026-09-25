//! 系统托盘：左键显示主界面；右键菜单播放控制（上一首 / 播放暂停 / 下一首）+「显示 / 退出」。

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn build_menu(app: &AppHandle, playing: bool) -> tauri::Result<Menu<tauri::Wry>> {
    let play_label = if playing { "暂停" } else { "播放" };
    let prev_i = MenuItem::with_id(app, "prev", "上一首", true, None::<&str>)?;
    let play_i = MenuItem::with_id(app, "play_pause", play_label, true, None::<&str>)?;
    let next_i = MenuItem::with_id(app, "next", "下一首", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let show_i = MenuItem::with_id(app, "show", "显示主界面", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    Menu::with_items(app, &[&prev_i, &play_i, &next_i, &sep, &show_i, &quit_i])
}

/// 托盘菜单上次展示的「是否在播」；状态没变就不重建菜单。
static LAST_PLAYING: AtomicBool = AtomicBool::new(false);

/// 播放状态变化时刷新托盘「播放 / 暂停」文案。
pub fn sync_play_pause(app: &AppHandle, playing: bool) {
    if LAST_PLAYING.swap(playing, Ordering::SeqCst) == playing {
        return;
    }
    let Some(tray) = app.tray_by_id("main-tray") else {
        return;
    };
    if let Ok(menu) = build_menu(app, playing) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// 托盘播放控制：与 IPC player_toggle / next / prev 同源，并广播状态。
fn control(app: &AppHandle, action: &str) {
    crate::commands::control_player(app, action);
}

pub fn init(app: &tauri::App) -> tauri::Result<()> {
    let menu = build_menu(app.app_handle(), false)?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .tooltip("AxMusic")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "prev" | "play_pause" | "next" => control(app, event.id().as_ref()),
            "show" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            let show = matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            );
            if show {
                show_main(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    LAST_PLAYING.store(false, Ordering::SeqCst);
    Ok(())
}
