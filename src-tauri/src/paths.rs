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
use std::sync::OnceLock;

const PORTABLE_INI: &str = "AxMusic-portable.ini";
const DATA_DIR: &str = "data";

static DATA_ROOT: OnceLock<PathBuf> = OnceLock::new();

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

    // Walk up a few levels: covers target/debug, target/release, and installed folder.
    for _ in 0..5 {
        if dir.join(PORTABLE_INI).is_file() || dir.join(DATA_DIR).is_dir() {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

fn appdata_root() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return Path::new(&appdata).join("AxMusic");
    }
    // Fallback: platform data dir
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("AxMusic")
}

/// Working SQLite database path (`axmusic.db`).
pub fn db_path() -> PathBuf {
    data_root().join("axmusic.db")
}

/// True when running in portable mode (data next to exe / marked by ini).
pub fn is_portable() -> bool {
    portable_root().is_some()
}

/// Force-create portable marker next to the executable (used by package script tests).
pub fn write_portable_ini(exe_dir: &Path) -> std::io::Result<()> {
    fs::write(
        exe_dir.join(PORTABLE_INI),
        "# AxMusic portable mode — keep data/ next to AxMusic.exe\nportable=1\n",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_path_under_data_root() {
        let p = db_path();
        assert!(p.ends_with("axmusic.db"));
        assert!(p.parent().is_some());
    }
}
