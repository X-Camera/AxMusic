//! Portable data-root resolution.
//!
//! Rules (see docs/技术架构.md §7):
//! - If `exe_dir/data/` exists OR `exe_dir/AxMusic-portable.ini` exists → `exe_dir/data`
//! - Walk up a few parents looking for the same markers (dev: target/debug → repo)
//! - Otherwise `%APPDATA%/AxMusic`
//!
//! Everything that writes app data must go through [`data_root`].

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};

const PORTABLE_INI: &str = "AxMusic-portable.ini";
const DATA_DIR: &str = "data";

static DATA_ROOT: OnceLock<PathBuf> = OnceLock::new();
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);
static LIBRARY_ROOT: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Resolve (once) and cache the portable data root.
pub fn data_root() -> &'static Path {
    DATA_ROOT.get_or_init(resolve_data_root)
}

/// Create `data/` (and subdirs) if missing. Returns the root.
pub fn ensure_data_root() -> &'static Path {
    let root = data_root();
    let _ = fs::create_dir_all(root);
    let _ = fs::create_dir_all(root.join("cache"));
    root
}

fn resolve_data_root() -> PathBuf {
    if let Some(dir) = portable_root() {
        return dir.join(DATA_DIR);
    }
    appdata_root()
}

fn portable_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.parent()?.to_path_buf();

    // Walk up a few levels: covers target/debug, target/release, and installed folder。
    // 只认明确的便携标记 AxMusic-portable.ini；祖先里恰好有 `data/` 目录太容易误判
    //（开发机、其它软件的 data 都可能撞名），不再据此判定便携。
    for _ in 0..5 {
        if dir.join(PORTABLE_INI).is_file() {
            return Some(dir);
        }
        // 安装目录旁的 data/ 仍认（exe 同级或上一级，且有 AxMusic.exe 或 ini 邻居）
        if dir.join(DATA_DIR).is_dir() {
            let looks_like_app = dir.join("AxMusic.exe").is_file()
                || dir.join("AxMusic-portable.ini").is_file()
                || dir
                    .file_name()
                    .map(|n| n.eq_ignore_ascii_case("AxMusic"))
                    .unwrap_or(false);
            if looks_like_app {
                return Some(dir);
            }
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

fn appdata_root() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return Path::new(&appdata).join("AxMusic");
    }
    // 不回落 temp_dir：临时目录重启即丢、权限也不适合长期存库指针/设置
    dirs::data_dir().unwrap_or_else(|| {
        // 最后兜底：用户主目录下隐藏目录（仍远好于 %TEMP%）
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".axmusic")
    })
}

/// Library working DB lives **inside the library root** (next to the audio).
pub fn library_db_path(library_root: &Path) -> PathBuf {
    library_root.join(DB_FILE_NAME)
}

/// 当前库目录（全局单例）。歌词路径解析等无 AppState 上下文的场景使用。
pub fn library_root() -> Option<PathBuf> {
    LIBRARY_ROOT.read().ok().and_then(|g| g.clone())
}

/// 设置/更新库目录全局值（启动加载 settings、init_library、settings 变更时调用）。
pub fn set_library_root(root: Option<PathBuf>) {
    if let Ok(mut g) = LIBRARY_ROOT.write() {
        *g = root;
    }
}

/// 库根目录/文件名单一事实来源（helper 与白名单共同引用，防止漂移）。
pub const ARCHIVED_DIR_NAME: &str = "archived";
pub const UNARCHIVED_DIR_NAME: &str = "Unarchived";
pub const LRC_DIR_NAME: &str = "lrc";
pub const COVERS_DIR_NAME: &str = "covers";
pub const PLAYLISTS_DIR_NAME: &str = "playlists";
pub const DB_FILE_NAME: &str = "axmusic.db";

/// 库内歌词目录：`<库根>/lrc/`
pub fn library_lrc_dir(library_root: &Path) -> PathBuf {
    library_root.join(LRC_DIR_NAME)
}

/// 歌曲归档区：`<库根>/archived/`
pub fn library_archived_dir(library_root: &Path) -> PathBuf {
    library_root.join(ARCHIVED_DIR_NAME)
}

/// 待整理区：`<库根>/Unarchived/`
pub fn library_unarchived_dir(library_root: &Path) -> PathBuf {
    library_root.join(UNARCHIVED_DIR_NAME)
}

/// 库根白名单目录/文件（见 docs/歌曲归档.md）。
/// 含 SQLite WAL/rollback journal 伴生文件——journal 在非 WAL 回退或崩溃恢复时会出现，
/// 被当成杂项挪走会导致库损坏。
pub const LIBRARY_ROOT_DIRS: &[&str] = &[
    ARCHIVED_DIR_NAME,
    UNARCHIVED_DIR_NAME,
    LRC_DIR_NAME,
    COVERS_DIR_NAME,
    PLAYLISTS_DIR_NAME,
];
pub const LIBRARY_ROOT_FILES: &[&str] = &[
    DB_FILE_NAME,
    "axmusic.db-wal",
    "axmusic.db-shm",
    "axmusic.db-journal",
];

/// 创建库目录骨架（archived / Unarchived / lrc / covers / playlists）。
pub fn ensure_library_dirs(library_root: &Path) -> std::io::Result<()> {
    for name in LIBRARY_ROOT_DIRS {
        fs::create_dir_all(library_root.join(name)).map_err(|e| {
            std::io::Error::new(e.kind(), format!("创建库目录 {name} 失败: {e}"))
        })?;
    }
    Ok(())
}

/// 同目录唯一临时文件路径（pid + 毫秒 + 自增计数），供「写临时文件 + rename」用。
/// 点前缀 + `.axtmp-*` 后缀：scanner 只认音频扩展名、歌单只认 .m3u8，不会被误扫。
pub fn temp_path_for(target: &Path) -> PathBuf {
    let pid = std::process::id();
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let name = target
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "tmp".into());
    target.with_file_name(format!(".{name}.axtmp-{pid}-{millis}-{seq}"))
}

/// 原子写文件：先写同目录唯一临时文件（flush + sync），再 rename 覆盖目标
///（Windows 下 `fs::rename` 即 MoveFileEx MOVEFILE_REPLACE_EXISTING，可覆盖已存在目标）。
/// 任何一步失败都清理临时文件，目标保持原样。
pub fn write_atomic(target: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = temp_path_for(target);
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// 字节 → 文本：UTF-8（容忍 BOM）优先，失败回退 GB18030
///（GBK 超集；Windows 中文环境的 .lrc/.m3u8 大量是 ANSI/GBK 编码）。
pub fn decode_text(bytes: &[u8]) -> String {
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::GB18030.decode(b).0.into_owned(),
    }
}

/// 读文本文件（编码容错版 `read_to_string`）；IO 错误原样上抛（调用方可区分 NotFound）。
pub fn read_text_lossy(path: &Path) -> std::io::Result<String> {
    Ok(decode_text(&fs::read(path)?))
}

/// 路径段级「是否在 root 下」：`Music` 不吞 `Music2`，Windows 大小写不敏感。
/// canonicalize 成功时用组件比较；失败时回退归一化字符串 + 段边界。
pub fn path_under_root(path: &str, root: &str) -> bool {
    let (Ok(p), Ok(r)) = (
        PathBuf::from(path).canonicalize(),
        PathBuf::from(root).canonicalize(),
    ) else {
        return path_under_root_norm(path, root);
    };
    p.starts_with(&r)
}

fn path_under_root_norm(path: &str, root: &str) -> bool {
    let norm = |s: &str| {
        let mut s = s.replace('/', "\\").to_lowercase();
        while s.len() > 3 && s.ends_with('\\') {
            s.pop();
        }
        s
    };
    let p = norm(path);
    let r = norm(root);
    if r.is_empty() || p.is_empty() {
        return false;
    }
    if p == r {
        return true;
    }
    p.len() > r.len() && p.starts_with(&r) && p.as_bytes().get(r.len()) == Some(&b'\\')
}

/// True when running in portable mode (data next to exe / marked by ini).
pub fn is_portable() -> bool {
    portable_root().is_some()
}

/// Force-create portable marker next to the executable (used by package script tests).
/// 目标必须是已存在目录，且写入固定文件名——拒绝任意覆盖。
#[allow(dead_code)] // 预留：便携标记写入（打包脚本/测试用）
pub fn write_portable_ini(exe_dir: &Path) -> std::io::Result<()> {
    if !exe_dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "便携标记只能写在已存在的程序目录",
        ));
    }
    fs::write(
        exe_dir.join(PORTABLE_INI),
        "# AxMusic portable mode — keep data/ next to AxMusic.exe\nportable=1\n",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_db_under_root() {
        let p = library_db_path(Path::new(r"D:\Lib"));
        assert!(p.ends_with("axmusic.db"));
    }

    #[test]
    fn path_under_root_segment_boundary() {
        // 字符串前缀会把 Music2 误判进 Music；组件边界必须拦住
        assert!(path_under_root_norm(
            r"D:\Music\a.flac",
            r"D:\Music"
        ));
        assert!(path_under_root_norm(
            r"D:\music\a.flac",
            r"D:\Music"
        ));
        assert!(!path_under_root_norm(
            r"D:\Music2\a.flac",
            r"D:\Music"
        ));
        assert!(!path_under_root_norm(
            r"D:\Music-old\a.flac",
            r"D:\Music"
        ));
        assert!(path_under_root_norm(r"D:\Music", r"D:\Music"));
        assert!(!path_under_root_norm(r"D:\Music\a.flac", r""));
    }
}
