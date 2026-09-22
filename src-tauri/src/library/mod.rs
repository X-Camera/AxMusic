//! SQLite working library for the management workspace.
//! Playback does NOT require this database.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub struct LibraryDb {
    conn: Connection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryRoot {
    pub id: i64,
    pub path: String,
    pub initialized_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackRow {
    pub id: i64,
    pub path: String,
    pub filename: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: String,
    pub track_no: Option<i64>,
    pub duration_ms: i64,
    pub format: String,
    pub sample_rate: Option<i64>,
    pub bit_rate: Option<i64>,
    pub has_cover: bool,
    pub has_lyrics: bool,
    pub has_year: bool,
    pub has_mb_id: bool,
    pub tag_status: String,
    pub missing: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumCard {
    pub album: String,
    pub album_artist: String,
    pub year: String,
    pub has_cover: bool,
    pub track_count: i64,
    pub cover_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrackFilter {
    pub query: Option<String>,
    /// Only rows missing at least one of: cover / lyrics / year / type / mb
    pub missing_only: bool,
    pub limit: Option<i64>,
}

pub fn now_iso() -> String {
    // Local ISO-ish timestamp without extra chrono dependency
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

impl LibraryDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)
            .with_context(|| format!("open database {}", path.display()))?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_default() -> Result<Self> {
        crate::paths::ensure_data_root();
        Self::open(&crate::paths::db_path())
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS library_roots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                initialized_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tracks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                filename TEXT NOT NULL DEFAULT '',
                title TEXT NOT NULL DEFAULT '',
                artist TEXT NOT NULL DEFAULT '',
                album TEXT NOT NULL DEFAULT '',
                album_artist TEXT NOT NULL DEFAULT '',
                year TEXT NOT NULL DEFAULT '',
                track_no INTEGER,
                duration_ms INTEGER NOT NULL DEFAULT 0,
                format TEXT NOT NULL DEFAULT '',
                sample_rate INTEGER,
                bit_rate INTEGER,
                has_cover INTEGER NOT NULL DEFAULT 0,
                has_lyrics INTEGER NOT NULL DEFAULT 0,
                has_year INTEGER NOT NULL DEFAULT 0,
                has_mb_id INTEGER NOT NULL DEFAULT 0,
                tag_status TEXT NOT NULL DEFAULT 'unmatched',
                missing TEXT NOT NULL DEFAULT '',
                file_size INTEGER NOT NULL DEFAULT 0,
                mtime INTEGER NOT NULL DEFAULT 0,
                is_deleted INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL DEFAULT ''
            );

            CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album);
            CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
            CREATE INDEX IF NOT EXISTS idx_tracks_status ON tracks(tag_status);
            CREATE INDEX IF NOT EXISTS idx_tracks_deleted ON tracks(is_deleted);

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    // ── library roots ──────────────────────────────────────────────

    pub fn get_library_root(&self) -> Result<Option<LibraryRoot>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, path, initialized_at FROM library_roots ORDER BY id LIMIT 1",
                [],
                |r| {
                    Ok(LibraryRoot {
                        id: r.get(0)?,
                        path: r.get(1)?,
                        initialized_at: r.get(2)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    pub fn set_library_root(&self, path: &Path) -> Result<LibraryRoot> {
        let path_str = path.to_string_lossy().to_string();
        let ts = now_iso();
        self.conn.execute(
            "INSERT INTO library_roots (path, initialized_at) VALUES (?1, ?2)
             ON CONFLICT(path) DO UPDATE SET initialized_at = excluded.initialized_at",
            params![path_str, ts],
        )?;
        // MVP: single root — drop others
        self.conn.execute(
            "DELETE FROM library_roots WHERE path != ?1",
            params![path_str],
        )?;
        self.get_library_root()?
            .context("library root missing after write")
    }

    // ── tracks ─────────────────────────────────────────────────────

    pub fn upsert_track(&self, t: &TrackRow, file_size: u64, mtime: u64) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO tracks (
                path, filename, title, artist, album, album_artist, year, track_no,
                duration_ms, format, sample_rate, bit_rate,
                has_cover, has_lyrics, has_year, has_mb_id, tag_status, missing,
                file_size, mtime, is_deleted, updated_at
            ) VALUES (
                ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,0,?21
            )
            ON CONFLICT(path) DO UPDATE SET
                filename=excluded.filename,
                title=excluded.title,
                artist=excluded.artist,
                album=excluded.album,
                album_artist=excluded.album_artist,
                year=excluded.year,
                track_no=excluded.track_no,
                duration_ms=excluded.duration_ms,
                format=excluded.format,
                sample_rate=excluded.sample_rate,
                bit_rate=excluded.bit_rate,
                has_cover=excluded.has_cover,
                has_lyrics=excluded.has_lyrics,
                has_year=excluded.has_year,
                has_mb_id=excluded.has_mb_id,
                tag_status=excluded.tag_status,
                missing=excluded.missing,
                file_size=excluded.file_size,
                mtime=excluded.mtime,
                is_deleted=0,
                updated_at=excluded.updated_at
            "#,
            params![
                t.path,
                t.filename,
                t.title,
                t.artist,
                t.album,
                t.album_artist,
                t.year,
                t.track_no,
                t.duration_ms,
                t.format,
                t.sample_rate,
                t.bit_rate,
                t.has_cover as i64,
                t.has_lyrics as i64,
                t.has_year as i64,
                t.has_mb_id as i64,
                t.tag_status,
                t.missing,
                file_size as i64,
                mtime as i64,
                now_iso(),
            ],
        )?;
        Ok(())
    }

    pub fn mark_missing_paths(&self, present: &[String]) -> Result<usize> {
        // Mark rows not in `present` as deleted (incremental scan).
        // For large sets, chunk; MVP uses a temporary approach via SQL.
        self.conn.execute("UPDATE tracks SET is_deleted = 1", [])?;
        for chunk in present.chunks(500) {
            let mut sql = String::from("UPDATE tracks SET is_deleted = 0 WHERE path IN (");
            let mut args: Vec<&dyn rusqlite::ToSql> = Vec::new();
            for (i, p) in chunk.iter().enumerate() {
                if i > 0 {
                    sql.push(',');
                }
                sql.push('?');
                args.push(p);
            }
            sql.push(')');
            self.conn.execute(&sql, args.as_slice())?;
        }
        Ok(0)
    }

    pub fn list_tracks(&self, filter: &TrackFilter) -> Result<Vec<TrackRow>> {
        let limit = filter.limit.unwrap_or(2000);
        let rows = if filter.missing_only {
            self.conn
                .prepare(
                    "SELECT id, path, filename, title, artist, album, album_artist, year, track_no,
                            duration_ms, format, sample_rate, bit_rate,
                            has_cover, has_lyrics, has_year, has_mb_id, tag_status, missing
                     FROM tracks
                     WHERE is_deleted = 0
                       AND (has_cover = 0 OR has_lyrics = 0 OR has_year = 0 OR has_mb_id = 0 OR tag_status != 'complete')
                     ORDER BY album_artist, album, track_no, filename
                     LIMIT ?1",
                )?
                .query_map(params![limit], map_track)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            self.conn
                .prepare(
                    "SELECT id, path, filename, title, artist, album, album_artist, year, track_no,
                            duration_ms, format, sample_rate, bit_rate,
                            has_cover, has_lyrics, has_year, has_mb_id, tag_status, missing
                     FROM tracks
                     WHERE is_deleted = 0
                     ORDER BY album_artist, album, track_no, filename
                     LIMIT ?1",
                )?
                .query_map(params![limit], map_track)?
                .collect::<Result<Vec<_>, _>>()?
        };
        Ok(rows)
    }

    pub fn track_count(&self) -> Result<i64> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM tracks WHERE is_deleted = 0",
            [],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    pub fn list_albums(&self) -> Result<Vec<AlbumCard>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                CASE WHEN album = '' THEN 'Unknown Album' ELSE album END AS album,
                CASE WHEN album_artist = '' THEN
                    (CASE WHEN artist = '' THEN 'Unknown Artist' ELSE artist END)
                ELSE album_artist END AS album_artist,
                MAX(year) AS year,
                MAX(has_cover) AS has_cover,
                COUNT(*) AS track_count
             FROM tracks
             WHERE is_deleted = 0
             GROUP BY album, album_artist
             ORDER BY album_artist, album",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(AlbumCard {
                    album: r.get(0)?,
                    album_artist: r.get(1)?,
                    year: r.get::<_, String>(2).unwrap_or_default(),
                    has_cover: r.get::<_, i64>(3)? != 0,
                    track_count: r.get(4)?,
                    cover_path: None,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn tracks_of_album(&self, album: &str, album_artist: &str) -> Result<Vec<TrackRow>> {
        let rows = self
            .conn
            .prepare(
                "SELECT id, path, filename, title, artist, album, album_artist, year, track_no,
                        duration_ms, format, sample_rate, bit_rate,
                        has_cover, has_lyrics, has_year, has_mb_id, tag_status, missing
                 FROM tracks
                 WHERE is_deleted = 0
                   AND CASE WHEN ?1 = 'Unknown Album' THEN album = '' ELSE album = ?1 END
                   AND (
                     CASE WHEN ?2 = 'Unknown Artist' THEN (album_artist = '' AND (artist = '' OR artist = ?2))
                     WHEN ?2 = 'Unknown Album Artist' THEN album_artist = ''
                     ELSE album_artist = ?2 OR (album_artist = '' AND artist = ?2) END
                   )
                 ORDER BY track_no, filename",
            )?
            .query_map(params![album, album_artist], map_track)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Browse tracks for album wall playback when library has no rows yet: fall back empty.
    pub fn get_track_by_path(&self, path: &str) -> Result<Option<TrackRow>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, path, filename, title, artist, album, album_artist, year, track_no,
                        duration_ms, format, sample_rate, bit_rate,
                        has_cover, has_lyrics, has_year, has_mb_id, tag_status, missing
                 FROM tracks WHERE path = ?1",
                params![path],
                map_track,
            )
            .optional()?;
        Ok(row)
    }
}

fn map_track(r: &rusqlite::Row<'_>) -> rusqlite::Result<TrackRow> {
    Ok(TrackRow {
        id: r.get(0)?,
        path: r.get(1)?,
        filename: r.get(2)?,
        title: r.get(3)?,
        artist: r.get(4)?,
        album: r.get(5)?,
        album_artist: r.get(6)?,
        year: r.get(7)?,
        track_no: r.get(8)?,
        duration_ms: r.get(9)?,
        format: r.get(10)?,
        sample_rate: r.get(11)?,
        bit_rate: r.get(12)?,
        has_cover: r.get::<_, i64>(13)? != 0,
        has_lyrics: r.get::<_, i64>(14)? != 0,
        has_year: r.get::<_, i64>(15)? != 0,
        has_mb_id: r.get::<_, i64>(16)? != 0,
        tag_status: r.get(17)?,
        missing: r.get(18)?,
    })
}

/// Create parent dirs + a new directory (library wizard).
pub fn create_library_dir(parent: &Path, name: &str) -> Result<PathBuf> {
    let name = name.trim();
    anyhow::ensure!(!name.is_empty(), "文件夹名不能为空");
    anyhow::ensure!(
        !name.contains(['<', '>', ':', '"', '/', '\\', '|', '?', '*']),
        "文件夹名含非法字符"
    );
    let dir = parent.join(name);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("无法创建目录 {}", dir.display()))?;
    Ok(dir)
}
