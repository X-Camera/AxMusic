//! 任务栏缩略图工具栏（ITaskbarList3）：悬浮任务栏图标时的上一首 / 播放暂停 / 下一首。
//! foobar2000 同款；仅 Windows。非 Windows 为空实现。

#[cfg(not(windows))]
mod imp {
    pub fn init(_app: &tauri::App) -> tauri::Result<()> {
        Ok(())
    }

    pub fn sync_play_pause(_app: &tauri::AppHandle, _playing: bool) {}
}

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::{Mutex, OnceLock};

    use tauri::{AppHandle, Manager};
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        CreateBitmap, CreateDIBSection, DeleteObject, GetDC, HBITMAP, ReleaseDC, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        ITaskbarList3, TaskbarList, THB_FLAGS, THB_ICON, THB_TOOLTIP, THBF_ENABLED, THUMBBUTTON,
        THBN_CLICKED,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, CreateIconIndirect, LoadIconW, RegisterWindowMessageW,
        SetWindowLongPtrW, GWLP_WNDPROC, HICON, ICONINFO, IDI_APPLICATION, WM_COMMAND, WNDPROC,
    };

    const BTN_PREV: u32 = 101;
    const BTN_PLAY: u32 = 102;
    const BTN_NEXT: u32 = 103;

    struct ThumbState {
        hwnd: HWND,
        taskbar: ITaskbarList3,
        icon_prev: HICON,
        icon_play: HICON,
        icon_pause: HICON,
        icon_next: HICON,
        playing: bool,
        added: bool,
    }

    // HWND/COM 句柄只在 UI 线程用；Mutex 保证跨线程 sync_play_pause 安全
    unsafe impl Send for ThumbState {}

    static APP: OnceLock<AppHandle> = OnceLock::new();
    static STATE: OnceLock<Mutex<Option<ThumbState>>> = OnceLock::new();
    static PLAYING: AtomicBool = AtomicBool::new(false);
    static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
    static OLD_WNDPROC: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

    fn app() -> Option<&'static AppHandle> {
        APP.get()
    }

    fn state() -> &'static Mutex<Option<ThumbState>> {
        STATE.get_or_init(|| Mutex::new(None))
    }

    /// 32bpp ARGB 简单几何图标：灰色单色主体，无描边。
    unsafe fn make_icon(draw: fn(&mut [u32])) -> Option<HICON> {
        const S: usize = 32;
        let mut pixels = vec![0u32; S * S];
        draw(&mut pixels);

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: S as i32,
                biHeight: -(S as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            },
            bmiColors: [Default::default(); 1],
        };

        let hdc = GetDC(None);
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let hbm: HBITMAP =
            match CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(b) => b,
                Err(_) => {
                    ReleaseDC(None, hdc);
                    return None;
                }
            };
        if bits.is_null() {
            let _ = DeleteObject(hbm);
            ReleaseDC(None, hdc);
            return None;
        }
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u32, S * S);

        // 单色 AND 掩码全 0：整图可见，透明度靠颜色通道 alpha
        let hmask = CreateBitmap(S as i32, S as i32, 1, 1, None);
        if hmask.is_invalid() {
            let _ = DeleteObject(hbm);
            ReleaseDC(None, hdc);
            return None;
        }
        ReleaseDC(None, hdc);

        let icon = CreateIconIndirect(&ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: hmask,
            hbmColor: hbm,
        })
        .ok();

        let _ = DeleteObject(hbm);
        let _ = DeleteObject(hmask);
        icon
    }

    /// 中灰主体（BGRA），透明背景
    const GRAY: u32 = 0xFF9A9A9A;

    fn put(px: &mut [u32], x: i32, y: i32) {
        if x < 0 || y < 0 || x >= 32 || y >= 32 {
            return;
        }
        let i = (y * 32 + x) as usize;
        if i < px.len() {
            px[i] = GRAY;
        }
    }

    fn fill_rect(px: &mut [u32], x0: i32, y0: i32, x1: i32, y1: i32) {
        for y in y0..y1 {
            for x in x0..x1 {
                put(px, x, y);
            }
        }
    }

    /// 实心右三角：底边在 x0，顶点朝右
    fn tri_right(px: &mut [u32], x0: i32, y0: i32, h: i32, fw: i32) {
        let mid = y0 + h / 2;
        for y in y0..y0 + h {
            let dy = if y >= mid { y - mid } else { mid - y };
            let w = ((fw * (h / 2 - dy)) / (h / 2).max(1)).max(2);
            for x in x0..x0 + w {
                put(px, x, y);
            }
        }
    }

    /// 实心左三角：底边在 x1，顶点朝左
    fn tri_left(px: &mut [u32], x1: i32, y0: i32, h: i32, fw: i32) {
        let mid = y0 + h / 2;
        for y in y0..y0 + h {
            let dy = if y >= mid { y - mid } else { mid - y };
            let w = ((fw * (h / 2 - dy)) / (h / 2).max(1)).max(2);
            for x in x1 - w..x1 {
                put(px, x, y);
            }
        }
    }

    fn draw_prev(px: &mut [u32]) {
        // |◀
        fill_rect(px, 7, 8, 11, 24);
        tri_left(px, 25, 8, 16, 14);
    }

    fn draw_play(px: &mut [u32]) {
        // ▶
        tri_right(px, 11, 7, 18, 14);
    }

    fn draw_pause(px: &mut [u32]) {
        // ‖
        fill_rect(px, 11, 8, 15, 24);
        fill_rect(px, 18, 8, 22, 24);
    }

    fn draw_next(px: &mut [u32]) {
        // ▶|
        tri_right(px, 7, 8, 16, 14);
        fill_rect(px, 22, 8, 26, 24);
    }

    fn tip_wide(text: &str) -> [u16; 260] {
        let mut buf = [0u16; 260];
        for (i, u) in text.encode_utf16().take(259).enumerate() {
            buf[i] = u;
        }
        buf
    }

    fn build_buttons(st: &ThumbState) -> [THUMBBUTTON; 3] {
        let mut prev = THUMBBUTTON::default();
        prev.dwMask = THB_ICON | THB_TOOLTIP | THB_FLAGS;
        prev.iId = BTN_PREV;
        prev.hIcon = st.icon_prev;
        prev.szTip = tip_wide("上一首");
        prev.dwFlags = THBF_ENABLED;

        let mut play = THUMBBUTTON::default();
        play.dwMask = THB_ICON | THB_TOOLTIP | THB_FLAGS;
        play.iId = BTN_PLAY;
        play.hIcon = if st.playing {
            st.icon_pause
        } else {
            st.icon_play
        };
        play.szTip = tip_wide(if st.playing { "暂停" } else { "播放" });
        play.dwFlags = THBF_ENABLED;

        let mut next = THUMBBUTTON::default();
        next.dwMask = THB_ICON | THB_TOOLTIP | THB_FLAGS;
        next.iId = BTN_NEXT;
        next.hIcon = st.icon_next;
        next.szTip = tip_wide("下一首");
        next.dwFlags = THBF_ENABLED;

        [prev, play, next]
    }

    unsafe fn try_add_buttons(st: &mut ThumbState) {
        if st.added {
            return;
        }
        let buttons = build_buttons(st);
        if st.taskbar.ThumbBarAddButtons(st.hwnd, &buttons).is_ok() {
            st.added = true;
        }
    }

    unsafe fn update_play_button(st: &mut ThumbState) {
        if !st.added {
            return;
        }
        let buttons = build_buttons(st);
        let _ = st.taskbar.ThumbBarUpdateButtons(st.hwnd, &buttons[1..2]);
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let created = TASKBAR_CREATED.load(Ordering::SeqCst);
        if created != 0 && msg == created {
            if let Ok(mut guard) = state().lock() {
                if let Some(st) = guard.as_mut() {
                    try_add_buttons(st);
                }
            }
        } else if msg == WM_COMMAND {
            let code = ((wparam.0 >> 16) & 0xffff) as u32;
            let id = (wparam.0 & 0xffff) as u32;
            if code == THBN_CLICKED as u32 {
                let action = match id {
                    BTN_PREV => "prev",
                    BTN_PLAY => "play_pause",
                    BTN_NEXT => "next",
                    _ => "",
                };
                if !action.is_empty() {
                    if let Some(app) = app() {
                        crate::commands::control_player(app, action);
                    }
                }
                return LRESULT(0);
            }
        }

        let old = OLD_WNDPROC.load(Ordering::SeqCst);
        if old == 0 {
            return LRESULT(0);
        }
        let prev: WNDPROC = unsafe { std::mem::transmute(old) };
        CallWindowProcW(prev, hwnd, msg, wparam, lparam)
    }

    pub fn init(app: &tauri::App) -> tauri::Result<()> {
        let _ = APP.set(app.app_handle().clone());

        let Some(win) = app.get_webview_window("main") else {
            return Ok(());
        };

        let raw = win.hwnd().map_err(|e| {
            eprintln!("[taskbar] hwnd: {e}");
            e
        })?;
        let hwnd = HWND(raw.0 as *mut std::ffi::c_void);

        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }

        let taskbar: ITaskbarList3 = unsafe {
            match CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("[taskbar] CoCreateInstance: {e}");
                    return Ok(());
                }
            }
        };
        if unsafe { taskbar.HrInit() }.is_err() {
            return Ok(());
        }

        unsafe fn icon_or_default(draw: fn(&mut [u32])) -> HICON {
            make_icon(draw).unwrap_or_else(|| LoadIconW(None, IDI_APPLICATION).unwrap_or_default())
        }

        let icons = unsafe {
            (
                icon_or_default(draw_prev),
                icon_or_default(draw_play),
                icon_or_default(draw_pause),
                icon_or_default(draw_next),
            )
        };

        {
            let mut guard = state().lock().unwrap_or_else(|e| e.into_inner());
            *guard = Some(ThumbState {
                hwnd,
                taskbar,
                icon_prev: icons.0,
                icon_play: icons.1,
                icon_pause: icons.2,
                icon_next: icons.3,
                playing: false,
                added: false,
            });
        }
        PLAYING.store(false, Ordering::SeqCst);

        unsafe {
            let msg = RegisterWindowMessageW(w!("TaskbarButtonCreated"));
            TASKBAR_CREATED.store(msg, Ordering::SeqCst);
        }

        let prev = unsafe {
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wnd_proc as *const () as isize)
        };
        if prev == 0 {
            eprintln!("[taskbar] SetWindowLongPtrW failed");
            return Ok(());
        }
        OLD_WNDPROC.store(prev, Ordering::SeqCst);

        // 窗口已显示时任务栏按钮多半已建，立刻挂一次；否则等 TaskbarButtonCreated
        unsafe {
            if let Ok(mut guard) = state().lock() {
                if let Some(st) = guard.as_mut() {
                    try_add_buttons(st);
                }
            }
        }
        Ok(())
    }

    pub fn sync_play_pause(app: &AppHandle, playing: bool) {
        if PLAYING.swap(playing, Ordering::SeqCst) == playing {
            return;
        }
        // ThumbBarUpdateButtons 是 COM，必须回到创建线程；get_player_state 等可能在工作线程
        let app = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Ok(mut guard) = state().lock() {
                if let Some(st) = guard.as_mut() {
                    st.playing = playing;
                    unsafe {
                        update_play_button(st);
                    }
                }
            }
        });
    }
}

pub use imp::{init, sync_play_pause};
