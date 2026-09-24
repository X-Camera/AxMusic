//! 文件夹浏览标签缓存（`data_root/cache/folder_tags.db`）。
//! 不绑库根：任意路径浏览都可用。`path + mtime + size` 失效重读。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderMeta {
    pub path: String,
    pub name: String,
    pub title: String,
    pub artist: String,
    pub duration_ms: u64,
}

fn db_path() -> PathBuf {
    paths::ensure_data_root().join("cache").join("folder_tags.db")
}

fn file_sig(p: &Path) -> (i64, i64) {
    let Ok(m) = std::fs::metadata(p) else {
        return (0, 0);
    };
    let mtime = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    (mtime, m.len() as i64)
}

fn open() -> rusqlite::Result<Connection> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(path)?;
    // 每次调用都新开连接，并发命令会撞锁：留等待窗口避免立刻 SQLITE_BUSY
    conn.busy_timeout(std::time::Duration::from_millis(3_000))?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         CREATE TABLE IF NOT EXISTS folder_tags (
            path TEXT PRIMARY KEY,
            mtime INTEGER NOT NULL,
            file_size INTEGER NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            artist TEXT NOT NULL DEFAULT '',
            duration_ms INTEGER NOT NULL DEFAULT 0,
            cached_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_folder_tags_mtime ON folder_tags(path);",
    )?;
    Ok(conn)
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn row_meta(path: &str, title: String, artist: String, duration_ms: i64) -> FolderMeta {
    let name = Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());
    FolderMeta {
        path: path.to_string(),
        name,
        title,
        artist,
        duration_ms: duration_ms.max(0) as u64,
    }
}

/// 只读缓存：命中且 mtime/size 一致才返回。
pub fn lookup(paths: &[String]) -> anyhow::Result<Vec<FolderMeta>> {
    let conn = open()?;
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        let (mtime, size) = file_sig(Path::new(path));
        if mtime == 0 && size == 0 {
            continue;
        }
        let hit: Option<(String, String, i64, i64, i64)> = conn
            .query_row(
                "SELECT title, artist, duration_ms, mtime, file_size FROM folder_tags WHERE path = ?1",
                params![path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        if let Some((title, artist, duration_ms, c_mtime, c_size)) = hit {
            if c_mtime == mtime && c_size == size {
                out.push(row_meta(path, title, artist, duration_ms));
            }
        }
    }
    Ok(out)
}

/// 读标签并写缓存（已有效则直接返回缓存）。失败的路径跳过。
pub fn read_and_cache(paths: &[String]) -> anyhow::Result<Vec<FolderMeta>> {
    let conn = open()?;
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        let p = Path::new(path);
        let (mtime, size) = file_sig(p);
        if !p.is_file() {
            continue;
        }

        // 缓存有效则复用
        if mtime != 0 || size != 0 {
            let hit: Option<(String, String, i64, i64, i64)> = conn
                .query_row(
                    "SELECT title, artist, duration_ms, mtime, file_size FROM folder_tags WHERE path = ?1",
                    params![path],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()?;
            if let Some((title, artist, duration_ms, c_mtime, c_size)) = hit {
                if c_mtime == mtime && c_size == size {
                    out.push(row_meta(path, title, artist, duration_ms));
                    continue;
                }
            }
        }

        match read_brief_tags(p) {
            Some((title, artist, duration_ms)) => {
                // 单条缓存写失败（BUSY/磁盘满）只跳过该条缓存，不丢已收集结果
                if let Err(e) = conn.execute(
                    "INSERT INTO folder_tags (path, mtime, file_size, title, artist, duration_ms, cached_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(path) DO UPDATE SET
                       mtime = excluded.mtime,
                       file_size = excluded.file_size,
                       title = excluded.title,
                       artist = excluded.artist,
                       duration_ms = excluded.duration_ms,
                       cached_at = excluded.cached_at",
                    params![path, mtime, size, title, artist, duration_ms as i64, now_secs()],
                ) {
                    eprintln!("[AxMusic] 标签缓存写入跳过 {path}: {e}");
                }
                out.push(row_meta(path, title, artist, duration_ms as i64));
            }
            None => {
                // 探测失败（文件占用/损坏）：文件名兜底展示但不写缓存，下次打开重试
                let stem = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                out.push(row_meta(path, stem, String::new(), 0));
            }
        }
    }
    Ok(out)
}

/// 轻量读标签：探测失败返回 None（调用方不得缓存）；无标签时按文件名兜底。
fn read_brief_tags(p: &Path) -> Option<(String, String, u64)> {
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::prelude::Accessor;
    use lofty::probe::Probe;

    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut title = stem;
    let mut artist = String::new();

    let tagged = Probe::open(p).and_then(|x| x.read()).ok()?;
    let duration_ms = tagged.properties().duration().as_millis() as u64;
    if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
        if let Some(t) = tag.title() {
            let t = t.to_string();
            if !t.trim().is_empty() {
                title = t;
            }
        }
        if let Some(a) = tag.artist() {
            artist = a.to_string();
        }
    }
    Some((title, artist, duration_ms))
}
