//! 资源管理器右键菜单（Windows）：音频文件 →「使用 AxMusic 播放」/「添加到 AxMusic 播放队列」。
//!
//! 写 HKCU（无需管理员）。动词挂在 `SystemFileAssociations\.ext\shell\`，
//! 用户改过默认打开方式也不影响。Win11 经典菜单在「显示更多选项」里。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::scanner::AUDIO_EXTS;

// 注册表子键按名字母序展示：Play 必须排在入队动词前
// （旧键 AxMusic.Enqueue 的 E < P 会跑到「播放」上面，卸载时一并清掉）
const VERB_PLAY: &str = "AxMusic.Play";
const VERB_ENQUEUE: &str = "AxMusic.Queue";
const VERB_ENQUEUE_LEGACY: &str = "AxMusic.Enqueue";
const LABEL_PLAY: &str = "使用 AxMusic 播放";
const LABEL_ENQUEUE: &str = "添加到 AxMusic 播放队列";
/// 多选 play 归并窗口（毫秒）。Windows MultiSelectModel=Player 逐文件拉起，
/// 同批多选几乎同时到达；窗口过长会把「连点两首想切歌」误并成入队。
const PLAY_BURST_WINDOW_MS: u64 = 600;

/// 右键动作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtxAction {
    Play,
    Enqueue,
}

/// 设置页查询结果
#[derive(Debug, Clone, Serialize)]
pub struct ShellMenuStatus {
    /// 当前平台是否支持注册
    pub supported: bool,
    /// 注册表里是否已有本应用菜单项
    pub registered: bool,
}

fn cls_base(ext: &str) -> String {
    format!("Software\\Classes\\SystemFileAssociations\\.{ext}\\shell")
}

/// 去掉 `\\?\` 前缀，避免写进注册表后命令行拉不起 exe
pub fn strip_extended_prefix(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        p.to_path_buf()
    }
}

/// 解析启动参数中的右键动作。`args[0]` 为 exe 路径。
/// 形如：`AxMusic.exe --ctx-play "D:\a.flac"` / `--ctx-enqueue ...`
pub fn parse_ctx_args(args: &[String]) -> Option<(CtxAction, Vec<PathBuf>)> {
    let mut action: Option<CtxAction> = None;
    let mut paths = Vec::new();
    for a in args.iter().skip(1) {
        match a.as_str() {
            "--ctx-play" => action = Some(CtxAction::Play),
            "--ctx-enqueue" => action = Some(CtxAction::Enqueue),
            s if s.starts_with("--") => {}
            s if !s.is_empty() => paths.push(PathBuf::from(s)),
            _ => {}
        }
    }
    let action = action?;
    if paths.is_empty() {
        return None;
    }
    // 只收音频文件，目录/杂项丢掉
    paths.retain(|p| {
        p.extension()
            .and_then(|e| e.to_str())
            .map(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
            .unwrap_or(false)
    });
    if paths.is_empty() {
        return None;
    }
    Some((action, paths))
}

/// 查询是否在「多选 play」归并窗口内（只读，不推进时间戳）
pub fn play_burst_active() -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        let g = play_burst_last().lock().unwrap_or_else(|e| e.into_inner());
        return g
            .map(|t| now.duration_since(t) < Duration::from_millis(PLAY_BURST_WINDOW_MS))
            .unwrap_or(false);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = PLAY_BURST_WINDOW_MS;
        false
    }
}

/// play 成功落地后调用，推进归并窗口（同批后续文件才会被识别）
pub fn mark_play_burst() {
    #[cfg(target_os = "windows")]
    {
        use std::time::Instant;
        let mut g = play_burst_last().lock().unwrap_or_else(|e| e.into_inner());
        *g = Some(Instant::now());
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = PLAY_BURST_WINDOW_MS;
    }
}

#[cfg(target_os = "windows")]
fn play_burst_last() -> &'static Mutex<Option<std::time::Instant>> {
    use std::sync::OnceLock;
    static LAST: OnceLock<Mutex<Option<std::time::Instant>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(None))
}

/// 把路径转成队列条目（优先标签标题，失败退回文件名）
pub fn queue_item_from_path(path: &Path) -> crate::player::QueueItem {
    let filename = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    match crate::scanner::read_track(path) {
        Ok(row) => crate::player::QueueItem {
            path: row.path,
            title: if row.title.is_empty() {
                filename
            } else {
                row.title
            },
            duration_ms: row.duration_ms.max(0) as u64,
        },
        Err(_) => crate::player::QueueItem {
            path: path.to_string_lossy().to_string(),
            title: filename,
            duration_ms: 0,
        },
    }
}

// ── 注册表 ────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod reg {
    use super::*;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    fn command_line(exe: &Path, flag: &str) -> String {
        // 路径可能含空格，整体加引号；%1 由资源管理器替换为选中文件
        format!("\"{}\" {} \"%1\"", exe.display(), flag)
    }

    fn write_verb(exe: &Path, base: &str, verb: &str, label: &str, flag: &str) -> Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu
            .create_subkey(format!("{base}\\{verb}"))
            .with_context(|| format!("写注册表失败：{base}\\{verb}"))?;
        key.set_value("", &label)?;
        // Icon 值是裸路径 + 可选资源索引，不要包引号
        key.set_value("Icon", &format!("{},0", exe.display()))?;
        // 多选时每个文件各拉起一次命令，由应用侧归并
        key.set_value("MultiSelectModel", &"Player")?;
        let (cmd, _) = key.create_subkey("command")?;
        cmd.set_value("", &command_line(exe, flag))?;
        Ok(())
    }

    fn delete_verb(base: &str, verb: &str) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let _ = hkcu.delete_subkey_all(format!("{base}\\{verb}"));
    }

    fn verb_exists(base: &str, verb: &str) -> bool {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        hkcu.open_subkey(format!("{base}\\{verb}")).is_ok()
    }

    pub fn register(exe: &Path) -> Result<()> {
        let exe = strip_extended_prefix(exe);
        if !exe.is_file() {
            anyhow::bail!("找不到 AxMusic 可执行文件：{}", exe.display());
        }
        for ext in AUDIO_EXTS {
            let base = cls_base(ext);
            // 先清旧键，避免 AxMusic.Enqueue 残留把「入队」顶到「播放」上面
            delete_verb(&base, VERB_ENQUEUE_LEGACY);
            write_verb(&exe, &base, VERB_PLAY, LABEL_PLAY, "--ctx-play")?;
            write_verb(&exe, &base, VERB_ENQUEUE, LABEL_ENQUEUE, "--ctx-enqueue")?;
        }
        Ok(())
    }

    pub fn unregister() -> Result<()> {
        for ext in AUDIO_EXTS {
            let base = cls_base(ext);
            delete_verb(&base, VERB_PLAY);
            delete_verb(&base, VERB_ENQUEUE);
            delete_verb(&base, VERB_ENQUEUE_LEGACY);
        }
        Ok(())
    }

    pub fn is_registered() -> bool {
        // 任一扩展挂上即视为已注册（含旧键，避免旧版残留显示「未注册」）
        AUDIO_EXTS.iter().any(|ext| {
            let base = cls_base(ext);
            verb_exists(&base, VERB_PLAY)
                || verb_exists(&base, VERB_ENQUEUE)
                || verb_exists(&base, VERB_ENQUEUE_LEGACY)
        })
    }
}

#[cfg(target_os = "windows")]
pub fn register(exe: &Path) -> Result<()> {
    reg::register(exe)
}

#[cfg(target_os = "windows")]
pub fn unregister() -> Result<()> {
    reg::unregister()
}

#[cfg(target_os = "windows")]
pub fn is_registered() -> bool {
    reg::is_registered()
}

#[cfg(not(target_os = "windows"))]
pub fn register(_exe: &Path) -> Result<()> {
    anyhow::bail!("右键菜单仅支持 Windows")
}

#[cfg(not(target_os = "windows"))]
pub fn unregister() -> Result<()> {
    anyhow::bail!("右键菜单仅支持 Windows")
}

#[cfg(not(target_os = "windows"))]
pub fn is_registered() -> bool {
    false
}

pub fn status() -> ShellMenuStatus {
    ShellMenuStatus {
        supported: cfg!(target_os = "windows"),
        registered: is_registered(),
    }
}

// ── 启动/单实例参数入口 ─────────────────────────────────────────
// 二次实例回调可能早于 AppState 就绪（插件 init 在 setup 之前），先挂起再冲刷。

static PENDING: Mutex<Vec<(CtxAction, Vec<PathBuf>)>> = Mutex::new(Vec::new());

/// 处理一次右键动作；AppState 未就绪则挂起，`flush_pending` 稍后补。
pub fn dispatch_ctx(app: &tauri::AppHandle, action: CtxAction, paths: Vec<PathBuf>) {
    use tauri::Manager;
    if app.try_state::<crate::commands::AppState>().is_some() {
        crate::commands::apply_ctx_action(app, action, paths);
    } else {
        PENDING
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((action, paths));
    }
}

/// 启动完成、AppState 可用后冲刷挂起的右键动作（保持到达顺序）。
pub fn flush_pending(app: &tauri::AppHandle) {
    let batch: Vec<(CtxAction, Vec<PathBuf>)> = {
        let mut g = PENDING.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *g)
    };
    for (action, paths) in batch {
        crate::commands::apply_ctx_action(app, action, paths);
    }
}

/// 从任意 argv（含 exe）解析并分发右键动作。返回是否命中。
pub fn dispatch_from_args(app: &tauri::AppHandle, args: &[String]) -> bool {
    match parse_ctx_args(args) {
        Some((action, paths)) => {
            dispatch_ctx(app, action, paths);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_play_and_enqueue() {
        let (a, p) = parse_ctx_args(&args(&[
            "AxMusic.exe",
            "--ctx-play",
            r"D:\music\a.flac",
        ]))
        .unwrap();
        assert_eq!(a, CtxAction::Play);
        assert_eq!(p, vec![PathBuf::from(r"D:\music\a.flac")]);

        let (a, p) = parse_ctx_args(&args(&[
            "AxMusic.exe",
            "--ctx-enqueue",
            r"D:\music\a.mp3",
        ]))
        .unwrap();
        assert_eq!(a, CtxAction::Enqueue);
        assert_eq!(p.len(), 1);
    }

    #[test]
    fn parse_skips_non_audio_and_flags() {
        // 非音频、纯 flag、无动作 → None
        assert!(parse_ctx_args(&args(&["AxMusic.exe", "--ctx-play", r"D:\x.txt"])).is_none());
        assert!(parse_ctx_args(&args(&["AxMusic.exe", r"D:\x.flac"])).is_none());
        assert!(parse_ctx_args(&args(&["AxMusic.exe", "--ctx-play"])).is_none());
    }

    #[test]
    fn verb_keys_sort_play_before_queue() {
        // 资源管理器按键名字母序展示菜单
        assert!(VERB_PLAY < VERB_ENQUEUE);
    }

    #[test]
    fn strip_unc_prefix() {
        assert_eq!(
            strip_extended_prefix(Path::new(r"\\?\C:\Apps\AxMusic.exe")),
            PathBuf::from(r"C:\Apps\AxMusic.exe")
        );
        assert_eq!(
            strip_extended_prefix(Path::new(r"C:\Apps\AxMusic.exe")),
            PathBuf::from(r"C:\Apps\AxMusic.exe")
        );
    }
}
