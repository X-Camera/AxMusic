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
    /// 外挂 .lrc（与音频同目录同名）
    pub has_lrc: bool,
    pub has_year: bool,
    pub has_mb_id: bool,
    pub tag_status: String,
    pub missing: String,
    pub release_type: String,
    pub mb_recording_mbid: String,
    pub mb_release_mbid: String,
    /// Linked catalog row (metadata match), None = 待刮削.
    pub catalog_id: Option<i64>,
    #[serde(default)]
    pub mtime: i64,
    #[serde(default)]
    pub file_size: i64,
    /// catalog 关联行字段（TRACK_COLS 关联子查询填充），管理表「与 catalog 匹配」高亮用；
    /// 未关联 catalog 时为 None。
    #[serde(default)]
    pub catalog_title: Option<String>,
    #[serde(default)]
    pub catalog_artist: Option<String>,
    #[serde(default)]
    pub catalog_album: Option<String>,
    #[serde(default)]
    pub catalog_year: Option<String>,
    #[serde(default)]
    pub catalog_track_no: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumCard {
    pub album: String,
    pub album_artist: String,
    pub year: String,
    pub has_cover: bool,
    pub track_count: i64,
    pub cover_path: Option<String>,
    /// 封面懒加载样例曲目（组内优先有封面的）
    pub cover_track_path: Option<String>,
    pub cover_track_mtime: i64,
}

/// 管理页右栏「库统计」面板的聚合数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryStats {
    pub total_tracks: i64,
    /// 已关联 catalog 的曲目数（已刮削）
    pub linked_tracks: i64,
    pub with_cover: i64,
    /// 内嵌歌词
    pub with_lyrics: i64,
    /// 外挂 .lrc
    pub with_lrc: i64,
    pub catalog_tracks: i64,
    /// catalog 中按 release MBID 去重的专辑数
    pub catalog_albums: i64,
    /// catalog 中按专辑艺人（空则艺人）去重的歌手数
    pub catalog_artists: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrackFilter {
    #[serde(default)]
    pub query: Option<String>,
    /// Only rows missing at least one of: cover / lyrics / year / type / mb
    #[serde(default)]
    pub missing_only: bool,
    /// Only rows not yet linked to a catalog record (待刮削).
    #[serde(default)]
    pub unlinked_only: bool,
    #[serde(default)]
    pub limit: Option<i64>,
    /// "album"（默认）| "title" | "artist"
    #[serde(default)]
    pub sort: Option<String>,
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

/// Shared column list for SELECTs mapped by [`map_track`].
/// 列顺序即 map_track 的读取下标顺序。catalog_* 为关联子查询列（tracks.catalog_id 命中时
/// 非 NULL），不改 FROM 即可被全部查询复用。
const TRACK_COLS: &str = "id, path, filename, title, artist, album, album_artist, year, track_no,
        duration_ms, format, sample_rate, bit_rate,
        has_cover, has_lyrics, has_lrc, has_year, has_mb_id, tag_status, missing,
        release_type, mb_recording_mbid, mb_release_mbid, catalog_id, mtime, file_size,
        (SELECT c.title FROM catalog c WHERE c.id = tracks.catalog_id) AS catalog_title,
        (SELECT c.artist FROM catalog c WHERE c.id = tracks.catalog_id) AS catalog_artist,
        (SELECT c.album FROM catalog c WHERE c.id = tracks.catalog_id) AS catalog_album,
        (SELECT c.year FROM catalog c WHERE c.id = tracks.catalog_id) AS catalog_year,
        (SELECT c.track_no FROM catalog c WHERE c.id = tracks.catalog_id) AS catalog_track_no";

impl LibraryDb {
    /// Open the working DB at an explicit path (usually `<library>/axmusic.db`).
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

            CREATE TABLE IF NOT EXISTS catalog (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source TEXT NOT NULL DEFAULT 'musicbrainz',
                kind TEXT NOT NULL DEFAULT 'track',
                mbid TEXT,
                release_mbid TEXT,
                title TEXT NOT NULL DEFAULT '',
                artist TEXT NOT NULL DEFAULT '',
                album TEXT NOT NULL DEFAULT '',
                album_artist TEXT NOT NULL DEFAULT '',
                year TEXT NOT NULL DEFAULT '',
                track_no INTEGER,
                release_type TEXT NOT NULL DEFAULT '',
                cover_path TEXT,
                created_at TEXT NOT NULL DEFAULT ''
            );

            CREATE TABLE IF NOT EXISTS path_map (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_path TEXT NOT NULL,
                dest_path TEXT NOT NULL,
                operated_at TEXT NOT NULL DEFAULT ''
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
                has_lrc INTEGER NOT NULL DEFAULT 0,
                has_year INTEGER NOT NULL DEFAULT 0,
                has_mb_id INTEGER NOT NULL DEFAULT 0,
                tag_status TEXT NOT NULL DEFAULT 'unmatched',
                missing TEXT NOT NULL DEFAULT '',
                file_size INTEGER NOT NULL DEFAULT 0,
                mtime INTEGER NOT NULL DEFAULT 0,
                is_deleted INTEGER NOT NULL DEFAULT 0,
                catalog_id INTEGER,
                release_type TEXT NOT NULL DEFAULT '',
                mb_recording_mbid TEXT NOT NULL DEFAULT '',
                mb_release_mbid TEXT NOT NULL DEFAULT '',
                updated_at TEXT NOT NULL DEFAULT ''
            );

            CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album);
            CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
            CREATE INDEX IF NOT EXISTS idx_tracks_status ON tracks(tag_status);
            CREATE INDEX IF NOT EXISTS idx_tracks_deleted ON tracks(is_deleted);
            CREATE INDEX IF NOT EXISTS idx_tracks_catalog ON tracks(catalog_id);
            CREATE INDEX IF NOT EXISTS idx_tracks_mb_rec ON tracks(mb_recording_mbid);

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;
        // Migrate DBs created before the mbid columns existed.
        for ddl in [
            "ALTER TABLE tracks ADD COLUMN release_type TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE tracks ADD COLUMN mb_recording_mbid TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE tracks ADD COLUMN mb_release_mbid TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE tracks ADD COLUMN has_lrc INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE catalog ADD COLUMN release_type TEXT NOT NULL DEFAULT ''",
        ] {
            let _ = self.conn.execute(ddl, []);
        }
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
                has_cover, has_lyrics, has_lrc, has_year, has_mb_id, tag_status, missing,
                file_size, mtime, is_deleted,
                release_type, mb_recording_mbid, mb_release_mbid, updated_at
            ) VALUES (
                ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,0,?22,?23,?24,?25
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
                has_lrc=excluded.has_lrc,
                has_year=excluded.has_year,
                has_mb_id=excluded.has_mb_id,
                tag_status=excluded.tag_status,
                missing=excluded.missing,
                file_size=excluded.file_size,
                mtime=excluded.mtime,
                is_deleted=0,
                release_type=excluded.release_type,
                mb_recording_mbid=excluded.mb_recording_mbid,
                mb_release_mbid=excluded.mb_release_mbid,
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
                t.has_lrc as i64,
                t.has_year as i64,
                t.has_mb_id as i64,
                t.tag_status,
                t.missing,
                file_size as i64,
                mtime as i64,
                t.release_type,
                t.mb_recording_mbid,
                t.mb_release_mbid,
                now_iso(),
            ],
        )?;
        // Re-apply any known catalog link (upsert keeps catalog_id on conflict).
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
        let mut where_conds: Vec<&str> = vec!["is_deleted = 0"];
        if filter.missing_only {
            where_conds.push(
                "(has_cover = 0 OR has_lyrics = 0 OR has_year = 0 OR has_mb_id = 0 OR tag_status != 'complete')",
            );
        }
        if filter.unlinked_only {
            where_conds.push("(catalog_id IS NULL OR catalog_id = 0)");
        }
        let order = match filter.sort.as_deref() {
            Some("title") => "title, album_artist, album, track_no, filename",
            Some("artist") => "artist, album, track_no, filename",
            _ => "album_artist, album, track_no, filename",
        };
        let sql = format!(
            "SELECT {TRACK_COLS}
             FROM tracks
             WHERE {}
             ORDER BY {order}
             LIMIT ?1",
            where_conds.join(" AND ")
        );
        let rows = self
            .conn
            .prepare(&sql)?
            .query_map(params![limit], map_track)?
            .collect::<Result<Vec<_>, _>>()?;
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

    /// 库统计：tracks 侧一次聚合 + catalog 侧一次聚合。
    pub fn library_stats(&self) -> Result<LibraryStats> {
        let (total_tracks, linked_tracks, with_cover, with_lyrics, with_lrc) =
            self.conn.query_row(
                "SELECT COUNT(*),
                        COALESCE(SUM(CASE WHEN catalog_id IS NOT NULL AND catalog_id > 0 THEN 1 ELSE 0 END), 0),
                        COALESCE(SUM(has_cover), 0),
                        COALESCE(SUM(has_lyrics), 0),
                        COALESCE(SUM(has_lrc), 0)
                 FROM tracks WHERE is_deleted = 0",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?;
        let (catalog_tracks, catalog_albums, catalog_artists) = self.conn.query_row(
            "SELECT COUNT(*),
                    COUNT(DISTINCT NULLIF(release_mbid, '')),
                    COUNT(DISTINCT COALESCE(NULLIF(album_artist, ''), NULLIF(artist, '')))
             FROM catalog",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        Ok(LibraryStats {
            total_tracks,
            linked_tracks,
            with_cover,
            with_lyrics,
            with_lrc,
            catalog_tracks,
            catalog_albums,
            catalog_artists,
        })
    }

    pub fn list_albums(&self) -> Result<Vec<AlbumCard>> {
        // cover_track_*：组内优先有封面的样例曲目，前端 coverCache 懒加载封面
        let mut stmt = self.conn.prepare(
            "SELECT
                CASE WHEN t.album = '' THEN 'Unknown Album' ELSE t.album END AS album,
                CASE WHEN t.album_artist = '' THEN
                    (CASE WHEN t.artist = '' THEN 'Unknown Artist' ELSE t.artist END)
                ELSE t.album_artist END AS album_artist,
                MAX(t.year) AS year,
                MAX(t.has_cover) AS has_cover,
                COUNT(*) AS track_count,
                (SELECT t2.path FROM tracks t2
                  WHERE t2.is_deleted = 0
                    AND t2.album = t.album
                    AND t2.album_artist = t.album_artist
                  ORDER BY t2.has_cover DESC, t2.track_no, t2.filename
                  LIMIT 1) AS cover_track_path,
                (SELECT t2.mtime FROM tracks t2
                  WHERE t2.is_deleted = 0
                    AND t2.album = t.album
                    AND t2.album_artist = t.album_artist
                  ORDER BY t2.has_cover DESC, t2.track_no, t2.filename
                  LIMIT 1) AS cover_track_mtime
             FROM tracks t
             WHERE t.is_deleted = 0
             GROUP BY t.album, t.album_artist
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
                    cover_track_path: r.get(5)?,
                    cover_track_mtime: r.get::<_, Option<i64>>(6)?.unwrap_or(0),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn tracks_of_album(&self, album: &str, album_artist: &str) -> Result<Vec<TrackRow>> {
        let rows = self
            .conn
            .prepare(&format!(
                "SELECT {TRACK_COLS}
                 FROM tracks
                 WHERE is_deleted = 0
                   AND CASE WHEN ?1 = 'Unknown Album' THEN album = '' ELSE album = ?1 END
                   AND (
                     CASE WHEN ?2 = 'Unknown Artist' THEN (album_artist = '' AND (artist = '' OR artist = ?2))
                     WHEN ?2 = 'Unknown Album Artist' THEN album_artist = ''
                     ELSE album_artist = ?2 OR (album_artist = '' AND artist = ?2) END
                   )
                 ORDER BY track_no, filename"
            ))?
            .query_map(params![album, album_artist], map_track)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Browse tracks for album wall playback when library has no rows yet: fall back empty.
    pub fn get_track_by_path(&self, path: &str) -> Result<Option<TrackRow>> {
        let row = self
            .conn
            .query_row(
                &format!("SELECT {TRACK_COLS} FROM tracks WHERE path = ?1"),
                params![path],
                map_track,
            )
            .optional()?;
        Ok(row)
    }

    pub fn get_track_by_id(&self, id: i64) -> Result<Option<TrackRow>> {
        let row = self
            .conn
            .query_row(
                &format!("SELECT {TRACK_COLS} FROM tracks WHERE id = ?1"),
                params![id],
                map_track,
            )
            .optional()?;
        Ok(row)
    }

    // ── catalog (online metadata subset; covers are files) ─────────

    /// Insert catalog row (online-metadata subset). Re-scraping the same recording
    /// updates the existing row instead of duplicating it.
    pub fn insert_catalog(&self, c: &CatalogRow) -> Result<i64> {
        if !c.mbid.is_empty() {
            let existing: Option<i64> = self
                .conn
                .query_row(
                    "SELECT id FROM catalog WHERE mbid = ?1 ORDER BY id LIMIT 1",
                    params![c.mbid],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(id) = existing {
                self.conn.execute(
                    "UPDATE catalog SET
                        source=?1, kind=?2, release_mbid=?3, title=?4, artist=?5,
                        album=?6, album_artist=?7, year=?8, track_no=?9,
                        release_type=?10, cover_path=COALESCE(?11, cover_path)
                     WHERE id=?12",
                    params![
                        c.source,
                        c.kind,
                        c.release_mbid,
                        c.title,
                        c.artist,
                        c.album,
                        c.album_artist,
                        c.year,
                        c.track_no,
                        c.release_type,
                        c.cover_path,
                        id,
                    ],
                )?;
                return Ok(id);
            }
        }
        self.conn.execute(
            "INSERT INTO catalog (
                source, kind, mbid, release_mbid, title, artist, album,
                album_artist, year, track_no, release_type, cover_path, created_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                c.source,
                c.kind,
                c.mbid,
                c.release_mbid,
                c.title,
                c.artist,
                c.album,
                c.album_artist,
                c.year,
                c.track_no,
                c.release_type,
                c.cover_path,
                now_iso(),
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn link_track_catalog(&self, track_id: i64, catalog_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET catalog_id = ?1 WHERE id = ?2",
            params![catalog_id, track_id],
        )?;
        Ok(())
    }

    /// 更新同一发行下全部 catalog 行的封面引用（单独刮封面后调用）。
    pub fn set_catalog_cover(&self, release_mbid: &str, cover_path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE catalog SET cover_path = ?1 WHERE release_mbid = ?2",
            params![cover_path, release_mbid],
        )?;
        Ok(())
    }

    /// 按 catalog 行 id 更新封面引用（无发行 MBID 的记录用）。
    pub fn set_catalog_cover_by_id(&self, catalog_id: i64, cover_path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE catalog SET cover_path = ?1 WHERE id = ?2",
            params![cover_path, catalog_id],
        )?;
        Ok(())
    }

    pub fn get_catalog(&self, id: i64) -> Result<Option<CatalogRow>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, source, kind, mbid, release_mbid, title, artist, album,
                        album_artist, year, track_no, release_type, cover_path, created_at
                 FROM catalog WHERE id = ?1",
                params![id],
                map_catalog,
            )
            .optional()?;
        Ok(row)
    }

    /// Match local track → catalog by fields: MBID → title+artist+album → title+artist.
    pub fn find_catalog_fuzzy(&self, t: &TrackRow) -> Result<Option<CatalogRow>> {
        const COLS: &str = "id, source, kind, mbid, release_mbid, title, artist, album,
                        album_artist, year, track_no, release_type, cover_path, created_at";
        // 1. MBID (recording, else release) — strongest key.
        if !t.mb_recording_mbid.is_empty() {
            let row = self
                .conn
                .query_row(
                    &format!(
                        "SELECT {COLS} FROM catalog
                         WHERE mbid = ?1 ORDER BY id DESC LIMIT 1"
                    ),
                    params![t.mb_recording_mbid],
                    map_catalog,
                )
                .optional()?;
            if row.is_some() {
                return Ok(row);
            }
        }
        if !t.mb_release_mbid.is_empty() && t.track_no.is_some() {
            let row = self
                .conn
                .query_row(
                    &format!(
                        "SELECT {COLS} FROM catalog
                         WHERE release_mbid = ?1 AND track_no = ?2
                         ORDER BY id DESC LIMIT 1"
                    ),
                    params![t.mb_release_mbid, t.track_no],
                    map_catalog,
                )
                .optional()?;
            if row.is_some() {
                return Ok(row);
            }
        }
        // 2. title + artist + album (exact).
        if !t.title.is_empty() && !t.artist.is_empty() {
            let row = self
                .conn
                .query_row(
                    &format!(
                        "SELECT {COLS} FROM catalog
                         WHERE title = ?1 AND artist = ?2
                           AND ((?3 != '' AND album = ?3) OR album = '')
                         ORDER BY (album = ?3) DESC, id DESC LIMIT 1"
                    ),
                    params![t.title, t.artist, t.album],
                    map_catalog,
                )
                .optional()?;
            if row.is_some() {
                return Ok(row);
            }
            // 3. looser: title + artist
            let row = self
                .conn
                .query_row(
                    &format!(
                        "SELECT {COLS} FROM catalog
                         WHERE title = ?1 AND artist = ?2
                         ORDER BY id DESC LIMIT 1"
                    ),
                    params![t.title, t.artist],
                    map_catalog,
                )
                .optional()?;
            return Ok(row);
        }
        Ok(None)
    }

    /// Find catalog entry for a local track: linked id → match fields → link on hit.
    pub fn find_catalog_for_track(&self, track_id: i64) -> Result<Option<CatalogRow>> {
        let Some(t) = self.get_track_by_id(track_id)? else {
            return Ok(None);
        };
        if let Some(cid) = t.catalog_id {
            if cid > 0 {
                return self.get_catalog(cid);
            }
        }
        // Try field match and persist the link so it survives rescans.
        if let Some(c) = self.find_catalog_fuzzy(&t)? {
            self.link_track_catalog(track_id, c.id)?;
            return Ok(Some(c));
        }
        Ok(None)
    }

    /// Batch field-match for all unlinked tracks (after scan / after catalog save).
    pub fn auto_match_unlinked(&self) -> Result<usize> {
        let rows = self
            .conn
            .prepare(&format!(
                "SELECT {TRACK_COLS} FROM tracks
                 WHERE is_deleted = 0 AND (catalog_id IS NULL OR catalog_id = 0)"
            ))?
            .query_map([], map_track)?
            .collect::<Result<Vec<_>, _>>()?;
        let mut n = 0;
        for t in rows {
            if let Some(c) = self.find_catalog_fuzzy(&t)? {
                self.link_track_catalog(t.id, c.id)?;
                n += 1;
            }
        }
        Ok(n)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogRow {
    pub id: i64,
    pub source: String,
    pub kind: String,
    pub mbid: String,
    pub release_mbid: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: String,
    pub track_no: Option<i64>,
    pub release_type: String,
    pub cover_path: Option<String>,
    pub created_at: String,
}

fn map_catalog(r: &rusqlite::Row<'_>) -> rusqlite::Result<CatalogRow> {
    Ok(CatalogRow {
        id: r.get(0)?,
        source: r.get(1)?,
        kind: r.get(2)?,
        mbid: r.get(3)?,
        release_mbid: r.get(4)?,
        title: r.get(5)?,
        artist: r.get(6)?,
        album: r.get(7)?,
        album_artist: r.get(8)?,
        year: r.get(9)?,
        track_no: r.get(10)?,
        release_type: r.get(11)?,
        cover_path: r.get(12)?,
        created_at: r.get(13)?,
    })
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
        has_lrc: r.get::<_, i64>(15)? != 0,
        has_year: r.get::<_, i64>(16)? != 0,
        has_mb_id: r.get::<_, i64>(17)? != 0,
        tag_status: r.get(18)?,
        missing: r.get(19)?,
        release_type: r.get(20)?,
        mb_recording_mbid: r.get(21)?,
        mb_release_mbid: r.get(22)?,
        catalog_id: r.get(23)?,
        mtime: r.get(24)?,
        file_size: r.get(25)?,
        catalog_title: r.get(26)?,
        catalog_artist: r.get(27)?,
        catalog_album: r.get(28)?,
        catalog_year: r.get(29)?,
        catalog_track_no: r.get(30)?,
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
