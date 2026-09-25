//! 播放状态 UI 同步：托盘菜单文案 + 任务栏缩略图按钮（各平台无操作则跳过）。

use tauri::AppHandle;

pub fn sync_play_ui(app: &AppHandle, playing: bool) {
    crate::tray::sync_play_pause(app, playing);
    crate::taskbar::sync_play_pause(app, playing);
}
