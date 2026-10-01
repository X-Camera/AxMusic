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
    /// 碟号（多碟发行；无碟号标签为 None，按 1 处理）
    #[serde(default)]
    pub disc_no: Option<i64>,
    pub duration_ms: i64,
    pub format: String,
    pub sample_rate: Option<i64>,
    pub bit_rate: Option<i64>,
    pub has_cover: bool,
    pub has_lyrics: bool,
    /// 外挂 .lrc（库 lrc/ 或音频旁）
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
    /// 专辑聚合稳定键（`id:{组内最小曲目id}`），取曲目用它，避免展示字段变化导致对不上
    pub group_key: String,
}

/// 歌手浏览卡片：按 album_artist（空则 artist）聚合。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistCard {
    pub name: String,
    pub track_count: i64,
    pub album_count: i64,
    pub has_cover: bool,
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
    /// 排序字段：title（默认）| artist | album | album_artist | year | track_no | format | duration | filename
    #[serde(default)]
    pub sort: Option<String>,
    /// 升/降序："asc"（默认）| "desc"
    #[serde(default)]
    pub sort_dir: Option<String>,
}

/// Unix 秒时间戳（存 TEXT 列，省 chrono 依赖）。名实相符：不是 ISO 串。
pub fn now_unix_secs() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

/// Shared column list for SELECTs mapped by [`map_track`].
/// 列顺序即 map_track 的读取下标顺序；新增列一律**追加在末尾**（catalog_* 关联子查询列之后），
/// 中间的顺序改动会让所有下标错位。catalog_* 为关联子查询列（tracks.catalog_id 命中时
/// 非 NULL），不改 FROM 即可被全部查询复用。
const TRACK_COLS: &str = "id, path, filename, title, artist, album, album_artist, year, track_no,
        duration_ms, format, sample_rate, bit_rate,
        has_cover, has_lyrics, has_lrc, has_year, has_mb_id, tag_status, missing,
        release_type, mb_recording_mbid, mb_release_mbid, catalog_id, mtime, file_size,
        (SELECT c.title FROM catalog c WHERE c.id = tracks.catalog_id) AS catalog_title,
        (SELECT c.artist FROM catalog c WHERE c.id = tracks.catalog_id) AS catalog_artist,
        (SELECT c.album FROM catalog c WHERE c.id = tracks.catalog_id) AS catalog_album,
        (SELECT c.year FROM catalog c WHERE c.id = tracks.catalog_id) AS catalog_year,
        (SELECT c.track_no FROM catalog c WHERE c.id = tracks.catalog_id) AS catalog_track_no,
        disc_no";

impl LibraryDb {
    /// Open the working DB at an explicit path (usually `<library>/axmusic.db`).
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)
            .with_context(|| format!("open database {}", path.display()))?;
        // 扫描线程持有第二条连接：写锁竞争时留等待窗口，避免立刻 SQLITE_BUSY
        conn.busy_timeout(std::time::Duration::from_millis(3_000))?;
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
                updated_at TEXT NOT NULL DEFAULT '',
                py_title TEXT,
                py_artist TEXT,
                py_album TEXT,
                py_album_artist TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album);
            CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
            CREATE INDEX IF NOT EXISTS idx_tracks_status ON tracks(tag_status);
            CREATE INDEX IF NOT EXISTS idx_tracks_deleted ON tracks(is_deleted);
            CREATE INDEX IF NOT EXISTS idx_tracks_catalog ON tracks(catalog_id);
            CREATE INDEX IF NOT EXISTS idx_tracks_mb_rec ON tracks(mb_recording_mbid);
            CREATE INDEX IF NOT EXISTS idx_tracks_mb_rel ON tracks(mb_release_mbid);
            CREATE INDEX IF NOT EXISTS idx_tracks_title_artist ON tracks(title, artist);

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
            // 多碟发行的碟号（P1：多碟曲号配对/排序用；NULL = 无碟号信息，按 1 处理）
            "ALTER TABLE tracks ADD COLUMN disc_no INTEGER",
            "ALTER TABLE catalog ADD COLUMN disc_no INTEGER",
            // 汉字拼音排序键（管理表按曲名等排序）；NULL=未算，''=算过（纯符号标题）
            "ALTER TABLE tracks ADD COLUMN py_title TEXT",
            "ALTER TABLE tracks ADD COLUMN py_artist TEXT",
            "ALTER TABLE tracks ADD COLUMN py_album TEXT",
            "ALTER TABLE tracks ADD COLUMN py_album_artist TEXT",
        ] {
            let _ = self.conn.execute(ddl, []);
        }
        // 历史库补拼音排序键（新库空表秒回）
        if let Err(e) = self.backfill_pinyin_keys() {
            eprintln!("[AxMusic] 拼音排序键回填跳过: {e}");
        }
        // catalog.mbid 部分唯一索引（空 mbid 不参与）。先收敛历史重复行：
        // 每组保留最小 id，tracks.catalog_id 重指后删多余行；任何一步失败只记录不致命。
        if let Err(e) = self.dedupe_catalog_mbid() {
            eprintln!("[AxMusic] catalog 去重跳过: {e}");
        }
        if let Err(e) = self.conn.execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_catalog_mbid ON catalog(mbid) WHERE mbid != '';
             CREATE INDEX IF NOT EXISTS idx_catalog_title_artist ON catalog(title, artist);
             CREATE INDEX IF NOT EXISTS idx_catalog_rel_track ON catalog(release_mbid, track_no);",
        ) {
            eprintln!("[AxMusic] catalog 索引未完全建立: {e}");
        }
        Ok(())
    }

    /// 合并 catalog 里 mbid 重复的行（保留最小 id，重指 tracks.catalog_id）。
    fn dedupe_catalog_mbid(&self) -> Result<()> {
        let dupes: Vec<(String, i64)> = self
            .conn
            .prepare(
                "SELECT mbid, MIN(id) FROM catalog WHERE mbid != '' GROUP BY mbid HAVING COUNT(*) > 1",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        for (mbid, keep) in dupes {
            self.conn.execute(
                "UPDATE tracks SET catalog_id = ?1
                 WHERE catalog_id IN (SELECT id FROM catalog WHERE mbid = ?2 AND id != ?1)",
                params![keep, mbid],
            )?;
            self.conn.execute(
                "DELETE FROM catalog WHERE mbid = ?1 AND id != ?2",
                params![mbid, keep],
            )?;
        }
        Ok(())
    }

    /// 事务边界（`unchecked_transaction`：`&self` 即可，同连接的 `&self` 方法都在事务内）。
    /// 注意不可嵌套；Drop 未 commit 自动回滚。
    pub fn transaction(&self) -> Result<rusqlite::Transaction<'_>> {
        Ok(self.conn.unchecked_transaction()?)
    }

    /// 底层连接（只读查询/对比用；写入走具名方法）。
    pub fn raw_conn(&self) -> &Connection {
        &self.conn
    }

    // ── library roots ──────────────────────────────────────────────

    #[allow(dead_code)] // 预留：读取当前库根（初始化页/调试）
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
        let ts = now_unix_secs();
        // 两条语句进同一事务：崩溃不会留下多行 root 的半状态
        let tx = self.transaction()?;
        tx.execute(
            "INSERT INTO library_roots (path, initialized_at) VALUES (?1, ?2)
             ON CONFLICT(path) DO UPDATE SET initialized_at = excluded.initialized_at",
            params![path_str, ts],
        )?;
        // MVP: single root — drop others
        tx.execute(
            "DELETE FROM library_roots WHERE path != ?1",
            params![path_str],
        )?;
        // 直接读回本次写入的行（而非按 id 升序猜第一行）
        let row = tx.query_row(
            "SELECT id, path, initialized_at FROM library_roots WHERE path = ?1",
            params![path_str],
            |r| {
                Ok(LibraryRoot {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    initialized_at: r.get(2)?,
                })
            },
        )?;
        tx.commit()?;
        Ok(row)
    }

    // ── tracks ─────────────────────────────────────────────────────

    pub fn upsert_track(&self, t: &TrackRow, file_size: u64, mtime: u64) -> Result<()> {
        let py_title = crate::text_norm::pinyin_sort_key(&t.title);
        let py_artist = crate::text_norm::pinyin_sort_key(&t.artist);
        let py_album = crate::text_norm::pinyin_sort_key(&t.album);
        let py_album_artist = crate::text_norm::pinyin_sort_key(&t.album_artist);
        self.conn.execute(
            r#"
            INSERT INTO tracks (
                path, filename, title, artist, album, album_artist, year, track_no,
                duration_ms, format, sample_rate, bit_rate,
                has_cover, has_lyrics, has_lrc, has_year, has_mb_id, tag_status, missing,
                file_size, mtime, is_deleted,
                release_type, mb_recording_mbid, mb_release_mbid, updated_at, disc_no,
                py_title, py_artist, py_album, py_album_artist
            ) VALUES (
                ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,0,?22,?23,?24,?25,?26,?27,?28,?29,?30
            )
            ON CONFLICT(path) DO UPDATE SET
                filename=excluded.filename,
                title=excluded.title,
                artist=excluded.artist,
                album=excluded.album,
                album_artist=excluded.album_artist,
                year=excluded.year,
                track_no=excluded.track_no,
                disc_no=excluded.disc_no,
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
                updated_at=excluded.updated_at,
                py_title=excluded.py_title,
                py_artist=excluded.py_artist,
                py_album=excluded.py_album,
                py_album_artist=excluded.py_album_artist
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
                now_unix_secs(),
                t.disc_no,
                py_title,
                py_artist,
                py_album,
                py_album_artist,
            ],
        )?;
        // Re-apply any known catalog link (upsert keeps catalog_id on conflict).
        Ok(())
    }

    /// 为尚未计算拼音排序键的行补键（入库早于拼音功能的库）。
    /// NULL=未算（含历史 ALTER 后的旧行），''=算过但结果为空（纯符号标题），不重写。
    /// 返回补写行数。扫描/写回走 upsert 会顺带维护，这里兜底历史数据。
    pub fn backfill_pinyin_keys(&self) -> Result<usize> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, artist, album, album_artist FROM tracks
             WHERE py_title IS NULL
                OR py_artist IS NULL
                OR py_album IS NULL
                OR py_album_artist IS NULL",
        )?;
        let rows: Vec<(i64, String, String, String, String)> = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        if rows.is_empty() {
            return Ok(0);
        }
        let tx = self.transaction()?;
        let mut n = 0;
        for (id, title, artist, album, album_artist) in rows {
            tx.execute(
                "UPDATE tracks SET
                    py_title = ?1, py_artist = ?2, py_album = ?3, py_album_artist = ?4
                 WHERE id = ?5",
                params![
                    crate::text_norm::pinyin_sort_key(&title),
                    crate::text_norm::pinyin_sort_key(&artist),
                    crate::text_norm::pinyin_sort_key(&album),
                    crate::text_norm::pinyin_sort_key(&album_artist),
                    id,
                ],
            )?;
            n += 1;
        }
        tx.commit()?;
        Ok(n)
    }

    /// 归档移动后改写 tracks.path / filename（按 id 定位，避免 upsert 插新行）。
    pub fn update_track_path(&self, id: i64, new_path: &str, new_filename: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET path = ?1, filename = ?2, updated_at = ?3 WHERE id = ?4",
            params![new_path, new_filename, now_unix_secs(), id],
        )?;
        Ok(())
    }

    /// 找出「同一首歌换路径」的旧行 id（搬迁继承）。
    /// 身份优先级：recording MBID → release MBID+轨号 → title+artist+album+时长 → title+artist+时长。
    /// 仅接受旧路径文件已不在磁盘（或已标删）的行；优先有 catalog_id、已标删者。
    pub fn find_moved_track_candidate(
        &self,
        new_path: &str,
        probe: &TrackRow,
    ) -> Result<Option<i64>> {
        const CAND_COLS: &str = "id, path, catalog_id, is_deleted";
        type Cand = (i64, String, Option<i64>, i64);

        fn map_cand(r: &rusqlite::Row<'_>) -> rusqlite::Result<Cand> {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        }

        // 旧路径文件仍在 → 不是搬迁（复制/双文件），不继承；优先有 catalog_id、已标删者
        fn better(best: &mut Option<(i64, (bool, bool))>, c: Cand) {
            let (id, path, cid, del) = c;
            if del == 0 && std::path::Path::new(&path).exists() {
                return;
            }
            let s = (cid.is_some_and(|v| v > 0), del != 0);
            if best.as_ref().map(|(_, b)| *b < s).unwrap_or(true) {
                *best = Some((id, s));
            }
        }

        let mut best: Option<(i64, (bool, bool))> = None;

        // 1. recording MBID
        if !probe.mb_recording_mbid.is_empty() {
            let mut stmt = self.conn.prepare(&format!(
                "SELECT {CAND_COLS} FROM tracks
                 WHERE path != ?1 AND mb_recording_mbid = ?2 AND mb_recording_mbid != ''"
            ))?;
            for row in stmt.query_map(params![new_path, probe.mb_recording_mbid], map_cand)? {
                better(&mut best, row?);
            }
            if best.is_some() {
                return Ok(best.map(|(id, _)| id));
            }
        }
        // 2. release MBID + 轨号（碟号尽量对齐）
        if !probe.mb_release_mbid.is_empty() && probe.track_no.is_some() {
            let mut stmt = self.conn.prepare(&format!(
                "SELECT {CAND_COLS} FROM tracks
                 WHERE path != ?1 AND mb_release_mbid = ?2 AND mb_release_mbid != ''
                   AND track_no = ?3
                   AND (disc_no IS NULL OR ?4 IS NULL OR disc_no = ?4)"
            ))?;
            for row in stmt.query_map(
                params![new_path, probe.mb_release_mbid, probe.track_no, probe.disc_no],
                map_cand,
            )? {
                better(&mut best, row?);
            }
            if best.is_some() {
                return Ok(best.map(|(id, _)| id));
            }
        }
        // 3/4. 标签字段 + 时长（时长须有效，降低误配）
        if !probe.title.is_empty() && !probe.artist.is_empty() && probe.duration_ms > 0 {
            let mut stmt = self.conn.prepare(&format!(
                "SELECT {CAND_COLS} FROM tracks
                 WHERE path != ?1 AND title = ?2 AND artist = ?3
                   AND ((?4 != '' AND album = ?4) OR album = '' OR ?4 = '')
                   AND duration_ms = ?5"
            ))?;
            for row in stmt.query_map(
                params![
                    new_path,
                    probe.title,
                    probe.artist,
                    probe.album,
                    probe.duration_ms
                ],
                map_cand,
            )? {
                better(&mut best, row?);
            }
            if best.is_some() {
                return Ok(best.map(|(id, _)| id));
            }

            let mut stmt = self.conn.prepare(&format!(
                "SELECT {CAND_COLS} FROM tracks
                 WHERE path != ?1 AND title = ?2 AND artist = ?3 AND duration_ms = ?4"
            ))?;
            for row in stmt.query_map(
                params![new_path, probe.title, probe.artist, probe.duration_ms],
                map_cand,
            )? {
                better(&mut best, row?);
            }
        }
        Ok(best.map(|(id, _)| id))
    }

    /// 歌单失效条目兜底重匹配：路径变更（归档/移动）后按线索找回库内仍存活曲目。
    /// 线索分层：EXTINF artist+title → 文件名主名（原名 / `artist - title` / title）→ title+时长。
    /// 仅接受磁盘上仍存在的行；无法唯一定位时返回 None（宁缺勿滥，避免误愈合歌单/喜爱）。
    pub fn find_track_for_playlist_rematch(
        &self,
        old_path: &str,
        artist: &str,
        title: &str,
        duration_ms: u64,
    ) -> Result<Option<TrackRow>> {
        let artist = artist.trim();
        let title = title.trim();
        let old = Path::new(old_path);
        let old_stem = old
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let old_ext = old
            .extension()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        // A. artist + title（归档改名后 EXTINF 仍带这两项）— 简繁等价
        if !artist.is_empty() && !title.is_empty() {
            let want_t = crate::text_norm::match_key(title);
            let want_a = crate::text_norm::match_key(artist);
            let mut stmt = self.conn.prepare(&format!(
                "SELECT {TRACK_COLS} FROM tracks WHERE is_deleted = 0"
            ))?;
            let cands: Vec<TrackRow> = stmt
                .query_map([], map_track)?
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .filter(|r| {
                    crate::text_norm::match_key(&r.title) == want_t
                        && crate::text_norm::match_key(&r.artist) == want_a
                })
                .collect();
            if let Some(r) = pick_unique_rematch(cands, duration_ms, &old_ext) {
                return Ok(Some(r));
            }
        }

        // B. 文件名主名：原主名 / `artist - title` / title
        let mut stems: Vec<String> = Vec::new();
        if !old_stem.is_empty() {
            stems.push(old_stem.clone());
        }
        if !title.is_empty() {
            if !artist.is_empty() {
                stems.push(format!("{artist} - {title}").to_lowercase());
            }
            stems.push(title.to_lowercase());
        }
        for stem in stems {
            // LIKE 通配符转义：% _ \ —— 否则「100%」之类会误匹配
            let stem_like = stem
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            let mut stmt = self.conn.prepare(&format!(
                "SELECT {TRACK_COLS} FROM tracks
                 WHERE is_deleted = 0
                   AND (lower(filename) = lower(?1)
                        OR lower(filename) LIKE lower(?2) || '.%' ESCAPE '\\')"
            ))?;
            let with_ext = if old_ext.is_empty() {
                stem.clone()
            } else {
                format!("{stem}.{old_ext}")
            };
            let cands = stmt
                .query_map(params![with_ext, stem_like], map_track)?
                .collect::<Result<Vec<_>, _>>()?;
            if let Some(r) = pick_unique_rematch(cands, duration_ms, &old_ext) {
                return Ok(Some(r));
            }
        }

        // C. 仅 title + 时长（无歌手时的最后手段）— 简繁等价
        if !title.is_empty() && duration_ms > 0 {
            let want_t = crate::text_norm::match_key(title);
            let mut stmt = self.conn.prepare(&format!(
                "SELECT {TRACK_COLS} FROM tracks
                 WHERE is_deleted = 0
                   AND duration_ms > 0
                   AND abs(duration_ms - ?1) <= 3000"
            ))?;
            let cands: Vec<TrackRow> = stmt
                .query_map(params![duration_ms as i64], map_track)?
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .filter(|r| crate::text_norm::match_key(&r.title) == want_t)
                .collect();
            if let Some(r) = pick_unique_rematch(cands, duration_ms, &old_ext) {
                return Ok(Some(r));
            }
        }

        Ok(None)
    }

    /// 扫描收尾兜底：把「已删旧行」上的 catalog_id 继承给「仍存活但未关联」的新行。
    /// 覆盖扫描中途未能搬迁接管的场景（旧文件当时仍在、后被挪走等）。返回继承条数。
    pub fn inherit_catalog_from_moved(&self) -> Result<usize> {
        let unlinked: Vec<TrackRow> = self
            .conn
            .prepare(&format!(
                "SELECT {TRACK_COLS} FROM tracks
                 WHERE is_deleted = 0 AND (catalog_id IS NULL OR catalog_id = 0)"
            ))?
            .query_map([], map_track)?
            .collect::<Result<Vec<_>, _>>()?;
        let mut n = 0usize;
        for t in unlinked {
            let Some(old_id) = self.find_moved_track_candidate(&t.path, &t)? else {
                continue;
            };
            let cid: Option<i64> = self
                .conn
                .query_row(
                    "SELECT catalog_id FROM tracks WHERE id = ?1",
                    params![old_id],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            if let Some(cid) = cid {
                if cid > 0 {
                    self.link_track_catalog(t.id, cid)?;
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    /// 全量置删 + 分块回置进同一事务：中途崩溃/失败不会把全库残留在 is_deleted=1。
    /// 返回扫描后仍标记为缺失的行数。
    pub fn mark_missing_paths(&self, present: &[String]) -> Result<usize> {
        let tx = self.transaction()?;
        tx.execute("UPDATE tracks SET is_deleted = 1", [])?;
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
            tx.execute(&sql, args.as_slice())?;
        }
        let n: i64 = tx.query_row(
            "SELECT COUNT(*) FROM tracks WHERE is_deleted = 1",
            [],
            |r| r.get(0),
        )?;
        tx.commit()?;
        Ok(n as usize)
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
        let order = {
            let primary = match filter.sort.as_deref() {
                Some("artist") => "artist",
                Some("album") => "album",
                Some("album_artist") => "album_artist",
                Some("year") => "year",
                Some("track_no") => "track_no",
                Some("format") => "format",
                Some("duration") => "duration_ms",
                Some("filename") => "filename",
                // 默认曲名：写标签后不易整表跳动（不依赖 album_artist）
                _ => "title",
            };
            let dir = match filter.sort_dir.as_deref() {
                Some("desc") => "DESC",
                _ => "ASC",
            };
            // 汉字按拼音键排序（upsert/回填维护，NULL 回退原文防未回填时乱序）
            let primary = match primary {
                "title" => "COALESCE(py_title, title)",
                "artist" => "COALESCE(py_artist, artist)",
                "album" => "COALESCE(py_album, album)",
                "album_artist" => "COALESCE(py_album_artist, album_artist)",
                other => other,
            };
            // 次级：专辑 → 碟号 → 曲序 → 文件名 → 专辑艺人（表里不显示，垫底）
            format!(
                "{primary} {dir}, COALESCE(py_album, album), COALESCE(disc_no, 1), track_no, filename, COALESCE(py_album_artist, album_artist)"
            )
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

    /// 专辑墙/歌手页专辑列表。同一张专辑按「标签归一 / release MBID / catalog 身份」
    /// 任一命中合并，避免整理写入 album_artist 后裂成两张。
    pub fn list_albums(&self) -> Result<Vec<AlbumCard>> {
        let seeds = self.load_album_seeds(None)?;
        let mut cards: Vec<AlbumCard> = group_album_seeds(seeds)
            .into_iter()
            .map(|g| g.into_card())
            .collect();
        cards.sort_by_key(|c| {
            (
                crate::text_norm::pinyin_sort_key(&c.album_artist),
                crate::text_norm::pinyin_sort_key(&c.album),
            )
        });
        Ok(cards)
    }

    /// 按聚合键取整张专辑的曲目（键来自 [`AlbumCard::group_key`]）。
    pub fn tracks_of_album(&self, group_key: &str) -> Result<Vec<TrackRow>> {
        let seeds = self.load_album_seeds(None)?;
        let Some(group) = group_album_seeds(seeds).into_iter().find(|g| g.group_key == group_key)
        else {
            return Err(anyhow::anyhow!(
                "专辑分组键 {group_key} 已失效（曲目删除或标签变更导致重组），请刷新专辑列表"
            ));
        };
        if group.member_ids.is_empty() {
            return Ok(Vec::new());
        }
        // 一次 IN 取回，避免逐条点查
        let placeholders = vec!["?"; group.member_ids.len()].join(",");
        let sql = format!("SELECT {TRACK_COLS} FROM tracks WHERE id IN ({placeholders})");
        let params: Vec<&dyn rusqlite::types::ToSql> = group
            .member_ids
            .iter()
            .map(|id| id as &dyn rusqlite::types::ToSql)
            .collect();
        let mut rows = self
            .conn
            .prepare(&sql)?
            .query_map(params.as_slice(), map_track)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.sort_by(|a, b| {
            a.disc_no
                .unwrap_or(1)
                .cmp(&b.disc_no.unwrap_or(1))
                .then(a.track_no.cmp(&b.track_no))
                .then(a.filename.cmp(&b.filename))
        });
        Ok(rows)
    }

    /// 歌手名：album_artist 优先，空则 artist，再空则 Unknown Artist。
    pub fn list_artists(&self) -> Result<Vec<ArtistCard>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                artist_name,
                COUNT(*) AS track_count,
                COUNT(DISTINCT album_key) AS album_count,
                MAX(has_cover) AS has_cover,
                (SELECT t2.path FROM tracks t2
                  WHERE t2.is_deleted = 0
                    AND CASE WHEN t2.album_artist = '' THEN
                          (CASE WHEN t2.artist = '' THEN 'Unknown Artist' ELSE t2.artist END)
                        ELSE t2.album_artist END = artist_name
                  ORDER BY t2.has_cover DESC, t2.track_no, t2.filename
                  LIMIT 1) AS cover_track_path,
                (SELECT t2.mtime FROM tracks t2
                  WHERE t2.is_deleted = 0
                    AND CASE WHEN t2.album_artist = '' THEN
                          (CASE WHEN t2.artist = '' THEN 'Unknown Artist' ELSE t2.artist END)
                        ELSE t2.album_artist END = artist_name
                  ORDER BY t2.has_cover DESC, t2.track_no, t2.filename
                  LIMIT 1) AS cover_track_mtime
             FROM (
               SELECT
                 t.has_cover,
                 CASE WHEN t.album = '' THEN 'Unknown Album' ELSE t.album END AS album_key,
                 CASE WHEN t.album_artist = '' THEN
                   (CASE WHEN t.artist = '' THEN 'Unknown Artist' ELSE t.artist END)
                 ELSE t.album_artist END AS artist_name,
                 -- 排序键与名字同口径派生（py_* NULL=未回填，回退原文）；
                 -- 'Unknown Artist' 手写其 pinyin_sort_key 结果，避免大写 U 漂到拼音键前面
                 CASE WHEN t.album_artist = '' THEN
                   (CASE WHEN t.artist = '' THEN 'unknown artist' ELSE COALESCE(t.py_artist, t.artist) END)
                 ELSE COALESCE(t.py_album_artist, t.album_artist) END AS artist_py
               FROM tracks t
               WHERE t.is_deleted = 0
             )
             GROUP BY artist_name
             ORDER BY MIN(artist_py), artist_name",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ArtistCard {
                    name: r.get(0)?,
                    track_count: r.get(1)?,
                    album_count: r.get(2)?,
                    has_cover: r.get::<_, i64>(3)? != 0,
                    cover_track_path: r.get(4)?,
                    cover_track_mtime: r.get::<_, Option<i64>>(5)?.unwrap_or(0),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 歌手名匹配：与 list_artists 聚合口径一致（album_artist 优先，空则 artist）。
    /// `q` 是调用方 FROM 里 tracks 的可见名（load_album_seeds 联了 catalog，
    /// 不带表前缀的 artist/album_artist 是歧义列）。
    fn artist_match_sql(q: &str) -> String {
        format!(
            "CASE WHEN ?1 = 'Unknown Artist' THEN ({q}.album_artist = '' AND ({q}.artist = '' OR {q}.artist = ?1))
                  ELSE {q}.album_artist = ?1 OR ({q}.album_artist = '' AND {q}.artist = ?1) END"
        )
    }

    /// 曲目列表的歌手匹配：比聚合口径宽——专辑艺人或演唱者任一命中即算该歌手的歌。
    /// 听歌史/统计按原始 artist 标签聚合，跳进来的名字（合辑里的演唱者、合唱名）
    /// 在窄口径下会整页空，故曲目放宽；专辑区仍用窄口径，避免列出别人的半张专辑。
    fn artist_track_match_sql(q: &str) -> String {
        format!(
            "CASE WHEN ?1 = 'Unknown Artist' THEN ({q}.album_artist = '' AND ({q}.artist = '' OR {q}.artist = ?1))
                  ELSE {q}.album_artist = ?1 OR {q}.artist = ?1 END"
        )
    }

    /// 读取专辑聚合用轻量行（含 catalog 佐证字段）。`artist_filter` 与歌手列表口径一致。
    fn load_album_seeds(&self, artist_filter: Option<&str>) -> Result<Vec<AlbumSeed>> {
        let mut sql = String::from(
            "SELECT t.id, t.path, t.filename, t.artist, t.album, t.album_artist, t.year,
                    t.track_no, t.disc_no, t.has_cover, t.mtime, t.mb_release_mbid, t.catalog_id,
                    COALESCE(c.album, ''),
                    COALESCE(c.album_artist, ''),
                    COALESCE(c.artist, ''),
                    COALESCE(c.release_mbid, ''),
                    COALESCE(c.year, '')
             FROM tracks t
             LEFT JOIN catalog c ON c.id = t.catalog_id
             WHERE t.is_deleted = 0",
        );
        if artist_filter.is_some() {
            sql.push_str(&format!(" AND ({})", Self::artist_match_sql("t")));
        }
        sql.push_str(" ORDER BY t.id");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = match artist_filter {
            Some(artist) => stmt.query_map(params![artist], map_album_seed)?,
            None => stmt.query_map([], map_album_seed)?,
        };
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn albums_of_artist(&self, artist: &str) -> Result<Vec<AlbumCard>> {
        let seeds = self.load_album_seeds(Some(artist))?;
        let mut cards: Vec<AlbumCard> = group_album_seeds(seeds)
            .into_iter()
            .map(|g| g.into_card())
            .collect();
        cards.sort_by_key(|c| crate::text_norm::pinyin_sort_key(&c.album));
        Ok(cards)
    }

    pub fn tracks_of_artist(&self, artist: &str) -> Result<Vec<TrackRow>> {
        let sql = format!(
            "SELECT {TRACK_COLS}
             FROM tracks
             WHERE is_deleted = 0 AND ({})
             ORDER BY COALESCE(py_album, album), album, COALESCE(disc_no, 1), track_no, filename",
            Self::artist_track_match_sql("tracks")
        );
        let rows = self
            .conn
            .prepare(&sql)?
            .query_map(params![artist], map_track)?
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
    /// 去重键：recording MBID；无 MBID 时按 (release_mbid, track_no) 去重，避免重刮重行。
    /// 事务边界由调用方持有（catalog_save 批量落库）；唯一索引 idx_catalog_mbid 兜底并发。
    pub fn insert_catalog(&self, c: &CatalogRow) -> Result<i64> {
        let existing: Option<i64> = if !c.mbid.is_empty() {
            self.conn
                .query_row(
                    "SELECT id FROM catalog WHERE mbid = ?1 ORDER BY id LIMIT 1",
                    params![c.mbid],
                    |r| r.get(0),
                )
                .optional()?
        } else if !c.release_mbid.is_empty() {
            // 无 recording MBID 的行按 (source, release_mbid, disc, track_no) 去重：
            // 各源 id 命名空间独立，必须带 source 谓词避免跨源误判重行；
            // 多碟发行同一 track_no 在不同碟各出现一次，disc_no 必须进键
            self.conn
                .query_row(
                    "SELECT id FROM catalog WHERE mbid = '' AND source = ?1 AND release_mbid = ?2
                       AND track_no IS ?3 AND disc_no IS ?4
                     ORDER BY id LIMIT 1",
                    params![c.source, c.release_mbid, c.track_no, c.disc_no],
                    |r| r.get(0),
                )
                .optional()?
        } else {
            None
        };
        if let Some(id) = existing {
            self.conn.execute(
                "UPDATE catalog SET
                    source=?1, kind=?2, release_mbid=?3, title=?4, artist=?5,
                    album=?6, album_artist=?7, year=?8, track_no=?9,
                    release_type=?10, cover_path=COALESCE(?11, cover_path), disc_no=?13
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
                    c.disc_no,
                ],
            )?;
            return Ok(id);
        }
        self.conn.execute(
            "INSERT INTO catalog (
                source, kind, mbid, release_mbid, title, artist, album,
                album_artist, year, track_no, release_type, cover_path, created_at, disc_no
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
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
                now_unix_secs(),
                c.disc_no,
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
    /// 按 (source, release_mbid) 定位：各源 id 命名空间独立，可能撞值。
    pub fn set_catalog_cover(&self, source: &str, release_mbid: &str, cover_path: &str) -> Result<()> {
        // 空串会命中所有无发行 id 的行，批量覆盖封面引用——直接拒绝
        anyhow::ensure!(!release_mbid.is_empty(), "release_mbid 不能为空");
        anyhow::ensure!(!cover_path.is_empty(), "cover_path 不能为空");
        self.conn.execute(
            "UPDATE catalog SET cover_path = ?1 WHERE release_mbid = ?2 AND source = ?3",
            params![cover_path, release_mbid, source],
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
                        album_artist, year, track_no, release_type, cover_path, created_at, disc_no
                 FROM catalog WHERE id = ?1",
                params![id],
                map_catalog,
            )
            .optional()?;
        Ok(row)
    }

    /// Match local track → catalog by fields: MBID → title+artist+album → title+artist.
    pub fn find_catalog_fuzzy(&self, t: &TrackRow) -> Result<Option<CatalogRow>> {
        Ok(self.find_catalog_candidates(t)?.into_iter().next())
    }

    /// 全部候选（MBID 直配优先，再字段匹配），供对比面板切换不同匹配。
    pub fn find_catalog_candidates(&self, t: &TrackRow) -> Result<Vec<CatalogRow>> {
        let mut out: Vec<CatalogRow> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        if let Some(row) = self.find_catalog_by_mbid(t)? {
            seen.insert(row.id);
            out.push(row);
        }
        for c in self.find_catalog_matches(t)? {
            if seen.insert(c.id) {
                out.push(c);
            }
        }
        Ok(out)
    }

    /// MBID 直配：录音 MBID → 发行 MBID + 轨号（碟号一致优先）。
    fn find_catalog_by_mbid(&self, t: &TrackRow) -> Result<Option<CatalogRow>> {
        const COLS: &str = "id, source, kind, mbid, release_mbid, title, artist, album,
                        album_artist, year, track_no, release_type, cover_path, created_at, disc_no";
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
            // 多碟发行同一 track_no 每碟一行：本地有碟号时要求碟号一致（catalog 缺碟号则放行）
            let row = self
                .conn
                .query_row(
                    &format!(
                        "SELECT {COLS} FROM catalog
                         WHERE release_mbid = ?1 AND track_no = ?2
                           AND (?3 IS NULL OR disc_no IS NULL OR disc_no = ?3)
                         ORDER BY id DESC LIMIT 1"
                    ),
                    params![t.mb_release_mbid, t.track_no, t.disc_no],
                    map_catalog,
                )
                .optional()?;
            return Ok(row);
        }
        Ok(None)
    }

    /// 按字段找出**全部**可能的 catalog 候选（去重，优先级从高到低）。
    /// 供对比面板切换：同一首歌可能同时命中录音室版 / Live / 不同专辑发行。
    pub fn find_catalog_matches(&self, t: &TrackRow) -> Result<Vec<CatalogRow>> {
        const COLS: &str = "id, source, kind, mbid, release_mbid, title, artist, album,
                        album_artist, year, track_no, release_type, cover_path, created_at, disc_no";
        let t_title = crate::text_norm::match_key(&t.title);
        let t_artist = crate::text_norm::match_key(&t.artist);
        if t_title.is_empty() || t_artist.is_empty() {
            return Ok(Vec::new());
        }
        let t_album = crate::text_norm::match_key(&t.album);
        let t_title_fuzzy = crate::text_norm::title_match_key(&t.title);
        let t_title_plain = crate::text_norm::match_key(&t.title) == t_title_fuzzy;

        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {COLS} FROM catalog ORDER BY id DESC"))?;
        let rows = stmt.query_map([], map_catalog)?;

        // 精确优先于模糊；同级里 album 命中优先。每档保留多条供切换。
        let mut exact_album: Vec<CatalogRow> = Vec::new();
        let mut exact_title_artist: Vec<CatalogRow> = Vec::new();
        let mut fuzzy_album: Vec<CatalogRow> = Vec::new();
        let mut fuzzy_title_artist: Vec<CatalogRow> = Vec::new();
        for row in rows {
            let c = row?;
            if crate::text_norm::match_key(&c.artist) != t_artist {
                continue;
            }
            let c_title = crate::text_norm::match_key(&c.title);
            let album_hit = !t_album.is_empty() && crate::text_norm::match_key(&c.album) == t_album;
            if c_title == t_title {
                if album_hit {
                    exact_album.push(c);
                } else {
                    exact_title_artist.push(c);
                }
                continue;
            }
            // 版本后缀模糊：仅在「一侧无版本后缀」时允许（Live↔Remix 等双侧不同后缀不互串）
            if !t_title_fuzzy.is_empty() {
                let c_fuzzy = crate::text_norm::title_match_key(&c.title);
                let c_plain = crate::text_norm::match_key(&c.title) == c_fuzzy;
                let suffix_ok = t_title_plain || c_plain;
                if suffix_ok && c_fuzzy == t_title_fuzzy {
                    if album_hit {
                        fuzzy_album.push(c);
                    } else {
                        fuzzy_title_artist.push(c);
                    }
                }
            }
        }
        let mut out: Vec<CatalogRow> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for c in exact_album
            .into_iter()
            .chain(exact_title_artist)
            .chain(fuzzy_album)
            .chain(fuzzy_title_artist)
        {
            if seen.insert(c.id) {
                out.push(c);
            }
        }
        Ok(out)
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
    /// catalog 侧只扫一遍建 match_key 索引，避免每条 track 全表重扫。
    pub fn auto_match_unlinked(&self) -> Result<usize> {
        let rows = self
            .conn
            .prepare(&format!(
                "SELECT {TRACK_COLS} FROM tracks
                 WHERE is_deleted = 0 AND (catalog_id IS NULL OR catalog_id = 0)"
            ))?
            .query_map([], map_track)?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.is_empty() {
            return Ok(0);
        }
        crate::text_norm::warm();

        const COLS: &str = "id, source, kind, mbid, release_mbid, title, artist, album,
                        album_artist, year, track_no, release_type, cover_path, created_at, disc_no";
        // (title_key, artist_key) → (album_key, id, is_plain_title)；
        // fuzzy 用 title_match_key 作键，收录全部候选，匹配时再按「一侧无版本后缀」过滤
        type Cand = (String, i64, bool);
        let mut index: std::collections::HashMap<(String, String), Vec<Cand>> =
            std::collections::HashMap::new();
        let mut fuzzy_index: std::collections::HashMap<(String, String), Vec<Cand>> =
            std::collections::HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare(&format!("SELECT {COLS} FROM catalog ORDER BY id DESC"))?;
            let cat_rows = stmt.query_map([], map_catalog)?;
            for row in cat_rows {
                let c = row?;
                let artist_key = crate::text_norm::match_key(&c.artist);
                let title_key = crate::text_norm::match_key(&c.title);
                if title_key.is_empty() || artist_key.is_empty() {
                    continue;
                }
                let album_key = crate::text_norm::match_key(&c.album);
                let ftitle = crate::text_norm::title_match_key(&c.title);
                let c_plain = title_key == ftitle;
                // 精确索引必进；模糊键为空只跳过 fuzzy（如标题整体是「(Live)」）
                if !ftitle.is_empty() {
                    fuzzy_index
                        .entry((ftitle, artist_key.clone()))
                        .or_default()
                        .push((album_key.clone(), c.id, c_plain));
                }
                index
                    .entry((title_key, artist_key))
                    .or_default()
                    .push((album_key, c.id, c_plain));
            }
        }

        let pick = |cands: &[Cand], t_album: &str| -> Option<i64> {
            let hit_album = |ak: &str| !t_album.is_empty() && ak == t_album;
            // 优先 album 命中，再取第一条
            cands
                .iter()
                .find(|(ak, _, _)| hit_album(ak))
                .or_else(|| cands.first())
                .map(|(_, id, _)| *id)
        };

        let mut n = 0;
        for t in rows {
            // MBID 优先（与 find_catalog_fuzzy 一致）：译名/简繁不一致时仍可挂上
            if let Some(c) = self.find_catalog_by_mbid(&t)? {
                self.link_track_catalog(t.id, c.id)?;
                n += 1;
                continue;
            }
            let t_title = crate::text_norm::match_key(&t.title);
            let t_artist = crate::text_norm::match_key(&t.artist);
            if t_title.is_empty() || t_artist.is_empty() {
                continue;
            }
            let t_album = crate::text_norm::match_key(&t.album);
            let t_title_fuzzy = crate::text_norm::title_match_key(&t.title);
            let t_plain = t_title == t_title_fuzzy;
            // 精确曲名优先；未命中再试剥版本后缀的模糊键（仅一侧带后缀，Live↔Remix 不互串）
            let hit = index
                .get(&(t_title, t_artist.clone()))
                .and_then(|cands| pick(cands, &t_album))
                .or_else(|| {
                    if t_title_fuzzy.is_empty() {
                        return None;
                    }
                    let cands = fuzzy_index.get(&(t_title_fuzzy, t_artist))?;
                    let filtered: Vec<Cand> = cands
                        .iter()
                        .filter(|(_, _, c_plain)| t_plain || *c_plain)
                        .cloned()
                        .collect();
                    pick(&filtered, &t_album)
                });
            if let Some(cid) = hit {
                self.link_track_catalog(t.id, cid)?;
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
    /// 碟号（多碟发行；单碟/无信息为 None）
    #[serde(default)]
    pub disc_no: Option<i64>,
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
        disc_no: r.get(14)?,
    })
}

/// 专辑聚合用轻量曲目（含 catalog 佐证字段，不进 TrackRow）。
#[derive(Clone)]
struct AlbumSeed {
    id: i64,
    path: String,
    filename: String,
    artist: String,
    album: String,
    album_artist: String,
    year: String,
    track_no: Option<i64>,
    disc_no: Option<i64>,
    has_cover: bool,
    mtime: i64,
    mb_release_mbid: String,
    catalog_id: Option<i64>,
    catalog_album: String,
    catalog_album_artist: String,
    catalog_artist: String,
    catalog_release_mbid: String,
    catalog_year: String,
}

fn map_album_seed(r: &rusqlite::Row<'_>) -> rusqlite::Result<AlbumSeed> {
    Ok(AlbumSeed {
        id: r.get(0)?,
        path: r.get(1)?,
        filename: r.get(2)?,
        artist: r.get(3)?,
        album: r.get(4)?,
        album_artist: r.get(5)?,
        year: r.get(6)?,
        track_no: r.get(7)?,
        disc_no: r.get(8)?,
        has_cover: r.get::<_, i64>(9)? != 0,
        mtime: r.get(10)?,
        mb_release_mbid: r.get(11)?,
        catalog_id: r.get(12)?,
        catalog_album: r.get(13)?,
        catalog_album_artist: r.get(14)?,
        catalog_artist: r.get(15)?,
        catalog_release_mbid: r.get(16)?,
        catalog_year: r.get(17)?,
    })
}

impl AlbumSeed {
    /// 文件标签上的有效专辑艺人：album_artist 优先，空则 artist。
    fn tag_album_artist(&self) -> &str {
        if self.album_artist.trim().is_empty() {
            self.artist.as_str()
        } else {
            self.album_artist.as_str()
        }
    }

    /// 合并身份键：标签归一、release MBID、catalog 专辑身份，任一相同即同一张专辑。
    /// mbid 不全时靠标签键兜底；catalog 作佐证把整理前后标签不一致的曲目并回来。
    fn identity_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        let album_k = crate::text_norm::match_key(&self.album);
        let artist_k = crate::text_norm::match_key(self.tag_album_artist());
        if !album_k.is_empty() || !artist_k.is_empty() {
            keys.push(format!("tag:{album_k}|{artist_k}"));
        }
        let rel = if !self.mb_release_mbid.trim().is_empty() {
            self.mb_release_mbid.trim()
        } else {
            self.catalog_release_mbid.trim()
        };
        if !rel.is_empty() {
            keys.push(format!("rel:{}", rel.to_ascii_lowercase()));
        }
        if self.catalog_id.is_some() {
            let ca = crate::text_norm::match_key(&self.catalog_album);
            let caa = if self.catalog_album_artist.trim().is_empty() {
                crate::text_norm::match_key(&self.catalog_artist)
            } else {
                crate::text_norm::match_key(&self.catalog_album_artist)
            };
            if !ca.is_empty() {
                keys.push(format!("cat:{ca}|{caa}"));
            }
        }
        keys
    }
}

/// 合并后的专辑组。
struct AlbumGroup {
    group_key: String,
    member_ids: Vec<i64>,
    seeds: Vec<AlbumSeed>,
}

impl AlbumGroup {
    fn into_card(self) -> AlbumCard {
        let album = display_album(&self.seeds);
        let album_artist = display_album_artist(&self.seeds);
        let mut year = String::new();
        let mut has_cover = false;
        for s in &self.seeds {
            let y = if !s.catalog_year.trim().is_empty() {
                s.catalog_year.trim()
            } else {
                s.year.trim()
            };
            if !y.is_empty() && (year.is_empty() || y > year.as_str()) {
                year = y.to_string();
            }
            if s.has_cover {
                has_cover = true;
            }
        }
        // 组内优先有封面的样例曲目，其次曲序更前
        let mut cover_sorted: Vec<&AlbumSeed> = self.seeds.iter().collect();
        cover_sorted.sort_by(|a, b| {
            b.has_cover
                .cmp(&a.has_cover)
                .then(a.disc_no.unwrap_or(1).cmp(&b.disc_no.unwrap_or(1)))
                .then(a.track_no.cmp(&b.track_no))
                .then(a.filename.cmp(&b.filename))
        });
        let cover_sample = cover_sorted.first().map(|s| (s.path.clone(), s.mtime));

        AlbumCard {
            album,
            album_artist,
            year,
            has_cover,
            track_count: self.seeds.len() as i64,
            cover_path: None,
            cover_track_path: cover_sample.as_ref().map(|c| c.0.clone()),
            cover_track_mtime: cover_sample.map(|c| c.1).unwrap_or(0),
            group_key: self.group_key,
        }
    }
}

fn display_album(seeds: &[AlbumSeed]) -> String {
    if let Some(s) = seeds.iter().find(|m| !m.catalog_album.trim().is_empty()) {
        return s.catalog_album.trim().to_string();
    }
    if let Some(s) = seeds.iter().find(|m| !m.album.trim().is_empty()) {
        return s.album.trim().to_string();
    }
    "Unknown Album".into()
}

fn display_album_artist(seeds: &[AlbumSeed]) -> String {
    if let Some(s) = seeds
        .iter()
        .find(|m| !m.catalog_album_artist.trim().is_empty())
    {
        return s.catalog_album_artist.trim().to_string();
    }
    if let Some(s) = seeds.iter().find(|m| !m.catalog_artist.trim().is_empty()) {
        return s.catalog_artist.trim().to_string();
    }
    if let Some(s) = seeds.iter().find(|m| !m.album_artist.trim().is_empty()) {
        return s.album_artist.trim().to_string();
    }
    if let Some(s) = seeds.iter().find(|m| !m.artist.trim().is_empty()) {
        return s.artist.trim().to_string();
    }
    "Unknown Artist".into()
}

/// union-find：任一身份键相同即合并（标签归一 / mbid / catalog 佐证）。
fn group_album_seeds(seeds: Vec<AlbumSeed>) -> Vec<AlbumGroup> {
    let n = seeds.len();
    if n == 0 {
        return Vec::new();
    }
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], i: usize) -> usize {
        if parent[i] != i {
            parent[i] = find(parent, parent[i]);
        }
        parent[i]
    }
    let mut first: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (i, seed) in seeds.iter().enumerate() {
        for key in seed.identity_keys() {
            if let Some(&j) = first.get(&key) {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    if ri < rj {
                        parent[rj] = ri;
                    } else {
                        parent[ri] = rj;
                    }
                }
            } else {
                first.insert(key, i);
            }
        }
    }
    let mut buckets: std::collections::BTreeMap<usize, Vec<usize>> = std::collections::BTreeMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        buckets.entry(r).or_default().push(i);
    }
    buckets
        .into_values()
        .map(|idxs| {
            let mut member_ids: Vec<i64> = idxs.iter().map(|&i| seeds[i].id).collect();
            member_ids.sort_unstable();
            let group_key = format!("id:{}", member_ids.first().copied().unwrap_or(0));
            let mut group_seeds: Vec<AlbumSeed> = idxs.into_iter().map(|i| seeds[i].clone()).collect();
            group_seeds.sort_by_key(|s| s.id);
            AlbumGroup {
                group_key,
                member_ids,
                seeds: group_seeds,
            }
        })
        .collect()
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
        disc_no: r.get(31)?,
    })
}

/// 重匹配候选收敛：只认磁盘仍存在的行；时长/扩展名择优后若仍不唯一，
/// 仅当身份（title+artist）完全一致时取时长最近、id 最小的一条，否则放弃（防误配）。
fn pick_unique_rematch(
    mut cands: Vec<TrackRow>,
    entry_duration_ms: u64,
    old_ext: &str,
) -> Option<TrackRow> {
    cands.retain(|r| Path::new(&r.path).is_file());
    if cands.is_empty() {
        return None;
    }
    if entry_duration_ms > 0 {
        let near: Vec<TrackRow> = cands
            .iter()
            .filter(|r| r.duration_ms > 0 && (r.duration_ms - entry_duration_ms as i64).abs() <= 3000)
            .cloned()
            .collect();
        if !near.is_empty() {
            cands = near;
        }
    }
    if !old_ext.is_empty() {
        let same_ext: Vec<TrackRow> = cands
            .iter()
            .filter(|r| {
                Path::new(&r.path)
                    .extension()
                    .map(|e| e.to_string_lossy().eq_ignore_ascii_case(old_ext))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        if !same_ext.is_empty() {
            cands = same_ext;
        }
    }
    if cands.len() == 1 {
        return cands.into_iter().next();
    }
    let ident = |r: &TrackRow| {
        (
            crate::text_norm::match_key(&r.title),
            crate::text_norm::match_key(&r.artist),
        )
    };
    let k0 = ident(&cands[0]);
    if cands.iter().all(|r| ident(r) == k0) {
        cands.sort_by_key(|r| {
            let d = if entry_duration_ms > 0 && r.duration_ms > 0 {
                (r.duration_ms - entry_duration_ms as i64).unsigned_abs()
            } else {
                0
            };
            (d, r.id)
        });
        return cands.into_iter().next();
    }
    None
}

/// Create parent dirs + a new directory (library wizard).
pub fn create_library_dir(parent: &Path, name: &str) -> Result<PathBuf> {
    let name = name.trim();
    anyhow::ensure!(!name.is_empty(), "文件夹名不能为空");
    anyhow::ensure!(name != "." && name != "..", "文件夹名不合法");
    anyhow::ensure!(
        !name.contains(['<', '>', ':', '"', '/', '\\', '|', '?', '*']),
        "文件夹名含非法字符"
    );
    // 保留设备名（含扩展名变体）
    let stem = name.split('.').next().unwrap_or(name);
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
        "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let upper_stem = stem.to_ascii_uppercase();
    anyhow::ensure!(
        !RESERVED.iter().any(|r| *r == upper_stem.as_str()),
        "文件夹名是 Windows 保留名"
    );
    let dir = parent.join(name);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("无法创建目录 {}", dir.display()))?;
    crate::paths::ensure_library_dirs(&dir)
        .with_context(|| format!("无法创建库子目录 {}", dir.display()))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_track(path: &str) -> TrackRow {
        TrackRow {
            id: 0,
            path: path.into(),
            filename: Path::new(path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
            title: "晴天".into(),
            artist: "周杰伦".into(),
            album: "叶惠美".into(),
            album_artist: "周杰伦".into(),
            year: "2003".into(),
            track_no: Some(1),
            disc_no: None,
            duration_ms: 269_000,
            format: "flac".into(),
            sample_rate: Some(44100),
            bit_rate: Some(900),
            has_cover: false,
            has_lyrics: false,
            has_lrc: false,
            has_year: true,
            has_mb_id: false,
            tag_status: "partial".into(),
            missing: String::new(),
            release_type: String::new(),
            mb_recording_mbid: String::new(),
            mb_release_mbid: String::new(),
            catalog_id: None,
            mtime: 0,
            file_size: 1000,
            catalog_title: None,
            catalog_artist: None,
            catalog_album: None,
            catalog_year: None,
            catalog_track_no: None,
        }
    }

    #[test]
    fn moved_track_inherits_catalog_id() {
        let dir = std::env::temp_dir().join(format!("axmusic-lib-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("t.db");
        let _ = std::fs::remove_file(&db_path);
        let db = LibraryDb::open(&db_path).unwrap();

        // 旧路径行：已关联 catalog，文件已不在
        let old_path = dir.join("old_gone.flac").to_string_lossy().to_string();
        let old = sample_track(&old_path);
        db.upsert_track(&old, 1000, 1).unwrap();
        let old_id = db.get_track_by_path(&old_path).unwrap().unwrap().id;
        let cat_id = db
            .insert_catalog(&CatalogRow {
                id: 0,
                source: "musicbrainz".into(),
                kind: "recording".into(),
                mbid: "mb-1".into(),
                release_mbid: String::new(),
                title: "晴天".into(),
                artist: "周杰伦".into(),
                album: "叶惠美".into(),
                album_artist: "周杰伦".into(),
                year: "2003".into(),
                track_no: Some(1),
                disc_no: None,
                release_type: String::new(),
                cover_path: None,
                created_at: String::new(),
            })
            .unwrap();
        db.link_track_catalog(old_id, cat_id).unwrap();

        // 新路径文件存在，内容是「同一首歌」但尚未入库关联
        let new_path = dir.join("Unarchived").join("new.flac");
        std::fs::create_dir_all(new_path.parent().unwrap()).unwrap();
        std::fs::write(&new_path, b"x").unwrap();
        let mut probe = sample_track(&new_path.to_string_lossy());
        probe.catalog_id = None;

        // 1) 扫描中搬迁接管：应命中旧行
        let cand = db
            .find_moved_track_candidate(&probe.path, &probe)
            .unwrap()
            .expect("应命中搬迁候选");
        assert_eq!(cand, old_id);

        // 2) 兜底 inherit：旧行已标删、新行未关联
        db.mark_missing_paths(&[]).unwrap(); // 全部标删
        db.upsert_track(&probe, 1000, 2).unwrap(); // 新路径活行，catalog_id 仍空
        let n = db.inherit_catalog_from_moved().unwrap();
        assert!(n >= 1, "inherit 应至少补上一条");
        let after = db.get_track_by_path(&probe.path).unwrap().unwrap();
        assert_eq!(after.catalog_id, Some(cat_id));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn catalog_fuzzy_matches_version_suffix_title() {
        let dir = std::env::temp_dir().join(format!("axmusic-fuzzy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("t.db");
        let _ = std::fs::remove_file(&db_path);
        let db = LibraryDb::open(&db_path).unwrap();

        // catalog：录音室版「晴天」
        let cat_id = db
            .insert_catalog(&CatalogRow {
                id: 0,
                source: "musicbrainz".into(),
                kind: "recording".into(),
                mbid: "mb-qing".into(),
                release_mbid: String::new(),
                title: "晴天".into(),
                artist: "周杰伦".into(),
                album: "叶惠美".into(),
                album_artist: "周杰伦".into(),
                year: "2003".into(),
                track_no: Some(1),
                disc_no: None,
                release_type: String::new(),
                cover_path: None,
                created_at: String::new(),
            })
            .unwrap();

        // 本地：现场版标题，应靠模糊键挂上
        let mut live = sample_track(&dir.join("live.flac").to_string_lossy());
        live.title = "晴天 (Live)".into();
        live.album = String::new();
        db.upsert_track(&live, 1000, 1).unwrap();
        let live_id = db
            .get_track_by_path(&dir.join("live.flac").to_string_lossy())
            .unwrap()
            .unwrap()
            .id;

        let hit = db.find_catalog_fuzzy(&live).unwrap().expect("Live 版应模糊命中");
        assert_eq!(hit.id, cat_id);

        let n = db.auto_match_unlinked().unwrap();
        assert!(n >= 1, "auto_match 应能挂上 Live 版");
        let after = db.get_track_by_id(live_id).unwrap().unwrap();
        assert_eq!(after.catalog_id, Some(cat_id));

        // 精确键仍优先：catalog 里若另有同名 Live 行，应挂 Live 行而不是录音室版
        let live_cat = db
            .insert_catalog(&CatalogRow {
                id: 0,
                source: "musicbrainz".into(),
                kind: "recording".into(),
                mbid: "mb-qing-live".into(),
                release_mbid: String::new(),
                title: "晴天 (Live)".into(),
                artist: "周杰伦".into(),
                album: "无与伦比".into(),
                album_artist: "周杰伦".into(),
                year: "2004".into(),
                track_no: Some(1),
                disc_no: None,
                release_type: String::new(),
                cover_path: None,
                created_at: String::new(),
            })
            .unwrap();
        db.link_track_catalog(live_id, 0).unwrap(); // 清掉关联再测
        // catalog_id=0 会被 auto_match 视为未关联
        let n = db.auto_match_unlinked().unwrap();
        let after = db.get_track_by_id(live_id).unwrap().unwrap();
        assert_eq!(after.catalog_id, Some(live_cat), "精确 Live 行应优先于模糊命中");
        assert!(n >= 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn playlist_rematch_finds_moved_track() {
        let dir = std::env::temp_dir().join(format!("axmusic-rematch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("t.db");
        let _ = std::fs::remove_file(&db_path);
        let db = LibraryDb::open(&db_path).unwrap();

        // 归档后的新路径（文件在磁盘上）
        let new_path = dir
            .join("archived")
            .join("周杰伦")
            .join("周杰伦 - 晴天.flac");
        std::fs::create_dir_all(new_path.parent().unwrap()).unwrap();
        std::fs::write(&new_path, b"x").unwrap();
        let mut row = sample_track(&new_path.to_string_lossy());
        row.filename = "周杰伦 - 晴天.flac".into();
        db.upsert_track(&row, 1000, 1).unwrap();

        // 旧路径已不存在；EXTINF 线索 = 歌手 - 歌名
        let old_path = dir.join("Unarchived").join("晴天.flac");
        let hit = db
            .find_track_for_playlist_rematch(
                &old_path.to_string_lossy(),
                "周杰伦",
                "晴天",
                269_000,
            )
            .unwrap()
            .expect("应按 artist+title 找回归档后的曲目");
        assert_eq!(hit.path, new_path.to_string_lossy());

        // 无歌手线索时靠文件名主名 `artist - title`
        let hit2 = db
            .find_track_for_playlist_rematch(
                &old_path.to_string_lossy(),
                "",
                "晴天",
                269_000,
            )
            .unwrap()
            .expect("应按 title/文件名找回");
        assert_eq!(hit2.path, new_path.to_string_lossy());

        // 库里没有的歌 → None
        let miss = db
            .find_track_for_playlist_rematch(
                &old_path.to_string_lossy(),
                "不存在",
                "没有这首歌",
                1000,
            )
            .unwrap();
        assert!(miss.is_none(), "匹配不到才算真失效");

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn temp_db(tag: &str) -> (std::path::PathBuf, LibraryDb) {
        let dir = std::env::temp_dir().join(format!("axmusic-album-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("t.db");
        let _ = std::fs::remove_file(&db_path);
        let db = LibraryDb::open(&db_path).unwrap();
        (dir, db)
    }

    fn seed_catalog(db: &LibraryDb, release_mbid: &str, album: &str, album_artist: &str) -> i64 {
        db.insert_catalog(&CatalogRow {
            id: 0,
            source: "musicbrainz".into(),
            kind: "recording".into(),
            mbid: format!("mb-{}", album),
            release_mbid: release_mbid.into(),
            title: "晴天".into(),
            artist: album_artist.into(),
            album: album.into(),
            album_artist: album_artist.into(),
            year: "2003".into(),
            track_no: Some(1),
            disc_no: None,
            release_type: String::new(),
            cover_path: None,
            created_at: String::new(),
        })
        .unwrap()
    }

    /// 整理写入 album_artist 后不应把同一专辑裂成两张（标签归一合并）。
    #[test]
    fn album_merges_when_album_artist_written() {
        let (dir, db) = temp_db("merge-aa");

        let mut organized = sample_track(&dir.join("archived/周杰伦/周杰伦 - 晴天.flac").to_string_lossy());
        organized.album_artist = "周杰伦".into();
        organized.track_no = Some(1);
        db.upsert_track(&organized, 1000, 1).unwrap();

        let mut pending = sample_track(&dir.join("Unarchived/东风破.flac").to_string_lossy());
        pending.title = "东风破".into();
        pending.album_artist = String::new(); // 未整理：album_artist 空
        pending.artist = "周杰伦".into();
        pending.track_no = Some(2);
        db.upsert_track(&pending, 1000, 2).unwrap();

        let albums = db.list_albums().unwrap();
        assert_eq!(albums.len(), 1, "同专不应裂成两张，got {:?}", albums);
        assert_eq!(albums[0].track_count, 2);
        assert_eq!(albums[0].album, "叶惠美");

        let tracks = db.tracks_of_album(&albums[0].group_key).unwrap();
        assert_eq!(tracks.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 无 mbid 时用 catalog 专辑身份作佐证合并。
    #[test]
    fn album_merges_by_catalog_identity_without_mbid() {
        let (dir, db) = temp_db("merge-cat");

        let cat_a = seed_catalog(&db, "", "叶惠美", "周杰伦");
        let mut a = sample_track(&dir.join("a.flac").to_string_lossy());
        a.album_artist = "周杰伦".into();
        a.catalog_id = Some(cat_a);
        db.upsert_track(&a, 1000, 1).unwrap();
        db.link_track_catalog(
            db.get_track_by_path(&a.path).unwrap().unwrap().id,
            cat_a,
        )
        .unwrap();

        // 另一半：标签 album_artist 被写成了别的写法，且未填 mbid
        let mut b = sample_track(&dir.join("b.flac").to_string_lossy());
        b.title = "东风破".into();
        b.album = "葉惠美".into(); // 繁体
        b.album_artist = "周傑倫".into();
        b.track_no = Some(2);
        db.upsert_track(&b, 1000, 2).unwrap();
        let b_id = db.get_track_by_path(&b.path).unwrap().unwrap().id;
        // catalog 同一专辑身份（无 release_mbid）
        let cat_b = seed_catalog(&db, "", "叶惠美", "周杰伦");
        db.link_track_catalog(b_id, cat_b).unwrap();

        let albums = db.list_albums().unwrap();
        assert_eq!(albums.len(), 1, "catalog 佐证应合并，got {:?}", albums);
        assert_eq!(albums[0].track_count, 2);
        assert_eq!(albums[0].album, "叶惠美");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 有 release_mbid 时更强：标签不一致也能并。
    #[test]
    fn album_merges_by_release_mbid() {
        let (dir, db) = temp_db("merge-rel");

        let cat = seed_catalog(&db, "rel-1", "叶惠美", "周杰伦");
        let mut a = sample_track(&dir.join("a.flac").to_string_lossy());
        a.mb_release_mbid = "rel-1".into();
        a.catalog_id = Some(cat);
        db.upsert_track(&a, 1000, 1).unwrap();
        db.link_track_catalog(db.get_track_by_path(&a.path).unwrap().unwrap().id, cat)
            .unwrap();

        let mut b = sample_track(&dir.join("b.flac").to_string_lossy());
        b.title = "东风破".into();
        b.album = "随便写的专辑名".into();
        b.album_artist = String::new();
        b.mb_release_mbid = "rel-1".into();
        b.track_no = Some(2);
        db.upsert_track(&b, 1000, 2).unwrap();

        let albums = db.list_albums().unwrap();
        assert_eq!(albums.len(), 1, "同 release 应合并，got {:?}", albums);
        assert_eq!(albums[0].track_count, 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 不同专辑仍分开。
    #[test]
    fn album_keeps_different_albums_apart() {
        let (dir, db) = temp_db("split");

        let mut a = sample_track(&dir.join("a.flac").to_string_lossy());
        a.album = "叶惠美".into();
        db.upsert_track(&a, 1000, 1).unwrap();

        let mut b = sample_track(&dir.join("b.flac").to_string_lossy());
        b.title = "夜曲".into();
        b.album = "十一月的萧邦".into();
        b.track_no = Some(1);
        db.upsert_track(&b, 1000, 2).unwrap();

        let albums = db.list_albums().unwrap();
        assert_eq!(albums.len(), 2, "不同专辑不应合并，got {:?}", albums);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 历史行补拼音键后，曲名按拼音序（爱 < 周）。
    #[test]
    fn backfill_pinyin_and_sort_by_title() {
        let (dir, db) = temp_db("pinyin");

        let mut zhou = sample_track(&dir.join("zhou.flac").to_string_lossy());
        zhou.title = "周杰伦".into();
        db.upsert_track(&zhou, 1000, 1).unwrap();

        let mut ai = sample_track(&dir.join("ai.flac").to_string_lossy());
        ai.title = "爱在西元前".into();
        ai.track_no = Some(2);
        db.upsert_track(&ai, 1000, 2).unwrap();

        // 模拟「拼音功能之前入库」：清掉排序键（NULL=未算）
        db.raw_conn()
            .execute(
                "UPDATE tracks SET py_title = NULL, py_artist = NULL, py_album = NULL, py_album_artist = NULL",
                [],
            )
            .unwrap();
        let n = db.backfill_pinyin_keys().unwrap();
        assert!(n >= 2, "应补写历史行，got {n}");

        let list = db
            .list_tracks(&TrackFilter {
                sort: Some("title".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].title, "爱在西元前", "拼音序：ai < zhou，got {:?}", list.iter().map(|t| &t.title).collect::<Vec<_>>());
        assert_eq!(list[1].title, "周杰伦");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 歌手详情曲目用宽口径（album_artist 或 artist 命中）：
    /// 合辑演唱者（album_artist=群星）、合唱名（artist=A/B）从听歌史跳进时不应为空；
    /// 专辑区仍是窄口径，不把别人专辑艺人的半张专辑列进来。
    #[test]
    fn artist_tracks_match_lead_singer_and_duet() {
        let (dir, db) = temp_db("artist-match");

        // 合辑：演唱者张韶涵，专辑艺人群星
        let mut va = sample_track(&dir.join("va.flac").to_string_lossy());
        va.title = "隐形的翅膀".into();
        va.artist = "张韶涵".into();
        va.album = "潘朵拉".into();
        va.album_artist = "群星".into();
        db.upsert_track(&va, 1000, 1).unwrap();

        // 合唱：artist=周杰伦/温岚，album_artist=周杰伦
        let mut duet = sample_track(&dir.join("duet.flac").to_string_lossy());
        duet.title = "屋顶".into();
        duet.artist = "周杰伦/温岚".into();
        duet.album = "叶惠美".into();
        duet.album_artist = "周杰伦".into();
        db.upsert_track(&duet, 1000, 2).unwrap();

        let zsh = db.tracks_of_artist("张韶涵").unwrap();
        assert_eq!(zsh.len(), 1, "合辑演唱者应能查到自己的歌，got {:?}", zsh.iter().map(|t| &t.title).collect::<Vec<_>>());
        assert_eq!(zsh[0].title, "隐形的翅膀");

        let duet_hits = db.tracks_of_artist("周杰伦/温岚").unwrap();
        assert_eq!(duet_hits.len(), 1, "合唱名应命中合唱曲");
        assert_eq!(duet_hits[0].title, "屋顶");

        // 宽口径下点「周杰伦」：命中 album_artist=周杰伦 的合唱曲
        let zjl = db.tracks_of_artist("周杰伦").unwrap();
        assert_eq!(zjl.len(), 1);
        assert_eq!(zjl[0].title, "屋顶");

        // 专辑区保持窄口径：张韶涵名下不列「群星」的半张合辑
        assert!(db.albums_of_artist("张韶涵").unwrap().is_empty());
        assert_eq!(db.albums_of_artist("周杰伦").unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 歌手墙与歌手详情曲目都按拼音排序（阿 du < 林 lin < 群 qun < 周 zhou；碟内 十一 shi < 叶 ye）。
    #[test]
    fn artist_lists_sort_by_pinyin() {
        let (dir, db) = temp_db("artist-py-sort");

        let mut adu = sample_track(&dir.join("adu.flac").to_string_lossy());
        adu.title = "天黑".into();
        adu.artist = "阿杜".into();
        adu.album = "天黑".into();
        adu.album_artist = "阿杜".into();
        db.upsert_track(&adu, 1000, 1).unwrap();

        let mut ljj = sample_track(&dir.join("ljj.flac").to_string_lossy());
        ljj.title = "江南".into();
        ljj.artist = "林俊杰".into();
        ljj.album = "第二天堂".into();
        ljj.album_artist = "林俊杰".into();
        db.upsert_track(&ljj, 1000, 2).unwrap();

        let mut qx = sample_track(&dir.join("qx.flac").to_string_lossy());
        qx.title = "隐形的翅膀".into();
        qx.artist = "张韶涵".into();
        qx.album = "潘朵拉".into();
        qx.album_artist = "群星".into();
        db.upsert_track(&qx, 1000, 3).unwrap();

        // 周杰伦两张专辑，插入顺序与拼音序相反（叶 ye 应先于 十一 shi 入庫）
        let mut ye = sample_track(&dir.join("ye.flac").to_string_lossy());
        ye.title = "晴天".into();
        ye.artist = "周杰伦".into();
        ye.album = "叶惠美".into();
        ye.album_artist = "周杰伦".into();
        db.upsert_track(&ye, 1000, 4).unwrap();

        let mut shi = sample_track(&dir.join("shi.flac").to_string_lossy());
        shi.title = "夜曲".into();
        shi.artist = "周杰伦".into();
        shi.album = "十一月的萧邦".into();
        shi.album_artist = "周杰伦".into();
        db.upsert_track(&shi, 1000, 5).unwrap();

        let wall = db.list_artists().unwrap();
        let names: Vec<&str> = wall.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["阿杜", "林俊杰", "群星", "周杰伦"], "歌手墙应按拼音序");

        let tracks = db.tracks_of_artist("周杰伦").unwrap();
        let albums: Vec<&str> = tracks.iter().map(|t| t.album.as_str()).collect();
        assert_eq!(albums, ["十一月的萧邦", "叶惠美"], "曲目应按专辑拼音分组排序");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

