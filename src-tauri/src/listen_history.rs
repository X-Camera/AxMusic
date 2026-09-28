//! 听歌历史：独立于库目录的播放事件日志。
//!
//! 落盘 `data_root/listen_history.db`，与洗库的 `axmusic.db` 隔离——
//! 播放不依赖库，库外文件同样统计；库重建/迁移不影响听歌史。
//! 身份分层：有 `mb_recording_mbid` 用它，否则 `title|artist`（小写去空白）。

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::library::LibraryDb;
use crate::player::{PlayStatus, PlayerSnapshot};

/// 有效听歌阈值：听满 20 秒即记一笔（用户选择，不再对齐 scrobble 的半首/4 分钟）。
const MIN_PLAY_MS: u64 = 20_000;
/// 播放中定期落库间隔（强杀/崩溃最多丢这么多听歌时长）
const FLUSH_EVERY_MS: u64 = 15_000;
/// 单次 wall-clock 增量上限（休眠/挂起后不虚增）。
/// 轮询间隙超过该值时改用播放位置增量校正（WebView 托盘节流等场景不丢时长）。
const MAX_TICK_DELTA_MS: u64 = 2_000;
/// 间隙超过此值视为「可能被节流」，优先信 position 增量
const THROTTLE_GAP_MS: u64 = 3_000;

pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    /// 自然播完
    Natural,
    /// 手动切歌 / 换曲
    Skip,
    /// 退出应用
    Exit,
    /// 进行中/进程被杀的占位（收口前；finalize 会改写成真实原因）
    Interrupted,
}

impl EndReason {
    fn as_str(self) -> &'static str {
        match self {
            EndReason::Natural => "natural",
            EndReason::Skip => "skip",
            EndReason::Exit => "exit",
            EndReason::Interrupted => "interrupted",
        }
    }
}

/// 播放时元数据快照（事件落盘后不再跟随文件/库变化）。
#[derive(Debug, Clone, Default)]
pub struct MetaSnapshot {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: String,
    pub track_no: Option<i64>,
    pub mb_recording_mbid: String,
    pub catalog_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListenEvent {
    pub id: i64,
    /// Unix 毫秒
    pub started_at: i64,
    pub ended_at: i64,
    /// 实际听的时长（暂停不计）
    pub play_ms: i64,
    pub track_duration_ms: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: String,
    pub track_no: Option<i64>,
    pub mb_recording_mbid: String,
    pub catalog_id: Option<i64>,
    pub source: String,
    pub end_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListenSummary {
    pub total_plays: i64,
    pub total_ms: i64,
    pub unique_tracks: i64,
    pub unique_artists: i64,
    pub unique_albums: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopListenItem {
    /// 聚合键（mbid 或 title|artist）
    pub key: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub plays: i64,
    pub total_ms: i64,
    /// 组内代表文件路径（可播 / 取封面；MAX 取值，文件可能已挪动）
    pub path: String,
    /// 组内曲目时长代表值（入队用）
    pub track_duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyListen {
    /// YYYY-MM-DD（本地时区日切）
    pub date: String,
    pub plays: i64,
    pub total_ms: i64,
}

/// 独立听歌库。
pub struct ListenDb {
    conn: Connection,
}

/// 同一首歌的聚合键：优先 recording MBID，否则 title|artist。
fn identity_sql() -> &'static str {
    "COALESCE(NULLIF(mb_recording_mbid, ''), lower(trim(title)) || '|' || lower(trim(artist)))"
}

impl ListenDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建听歌库目录失败: {}", parent.display()))?;
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_millis(3_000))?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_default() -> Result<Self> {
        let root = crate::paths::ensure_data_root();
        Self::open(&root.join("listen_history.db"))
    }

    /// 启动失败兜底：纯内存库（本次会话仍可记，退出即丢）。
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;

            CREATE TABLE IF NOT EXISTS play_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                started_at INTEGER NOT NULL,
                ended_at INTEGER NOT NULL,
                play_ms INTEGER NOT NULL DEFAULT 0,
                track_duration_ms INTEGER NOT NULL DEFAULT 0,
                path TEXT NOT NULL DEFAULT '',
                title TEXT NOT NULL DEFAULT '',
                artist TEXT NOT NULL DEFAULT '',
                album TEXT NOT NULL DEFAULT '',
                album_artist TEXT NOT NULL DEFAULT '',
                year TEXT NOT NULL DEFAULT '',
                track_no INTEGER,
                mb_recording_mbid TEXT NOT NULL DEFAULT '',
                catalog_id INTEGER,
                source TEXT NOT NULL DEFAULT '',
                end_reason TEXT NOT NULL DEFAULT 'skip'
            );

            CREATE INDEX IF NOT EXISTS idx_play_events_started ON play_events(started_at);
            CREATE INDEX IF NOT EXISTS idx_play_events_mbid ON play_events(mb_recording_mbid);
            CREATE INDEX IF NOT EXISTS idx_play_events_title_artist ON play_events(title, artist);
            "#,
        )?;
        Ok(())
    }

    pub fn insert_event(&self, e: &PendingPlay, end_reason: EndReason) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO play_events (
                started_at, ended_at, play_ms, track_duration_ms,
                path, title, artist, album, album_artist, year, track_no,
                mb_recording_mbid, catalog_id, source, end_reason
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![
                e.started_at_ms as i64,
                e.ended_at_ms as i64,
                e.play_ms as i64,
                e.track_duration_ms as i64,
                e.path,
                e.meta.title,
                e.meta.artist,
                e.meta.album,
                e.meta.album_artist,
                e.meta.year,
                e.meta.track_no,
                e.meta.mb_recording_mbid,
                e.meta.catalog_id,
                e.source,
                end_reason.as_str(),
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 更新已落库的进行中记录（收口或定期刷新）。
    pub fn update_event(&self, id: i64, e: &PendingPlay, end_reason: EndReason) -> Result<()> {
        self.conn.execute(
            "UPDATE play_events SET
                ended_at = ?2, play_ms = ?3, track_duration_ms = ?4,
                end_reason = ?5
             WHERE id = ?1",
            params![
                id,
                e.ended_at_ms as i64,
                e.play_ms as i64,
                e.track_duration_ms as i64,
                end_reason.as_str(),
            ],
        )?;
        Ok(())
    }

    /// 是否达到「算听了一次」阈值：听满 20 秒即记。
    pub fn counts_as_play(e: &PendingPlay) -> bool {
        e.play_ms >= MIN_PLAY_MS
    }

    /// `since=None` 表示全部。since 为 Unix 毫秒下界（含）。
    /// 时段/按日维度按**本地时区**分桶（unixepoch 默认 UTC，差 8 小时会把下午记成早上）。
    /// 无 LIMIT 的查询 since 绑 `?1`；带 `LIMIT ?1` 的查询 since 绑 `?2`。
    /// None 绑 NULL 时 `COALESCE(?n, started_at)` 恒真。
    const RANGE_SQL_1: &'static str = " AND started_at >= COALESCE(?1, started_at) ";
    const RANGE_SQL_2: &'static str = " AND started_at >= COALESCE(?2, started_at) ";

    pub fn summary(&self, since_ms: Option<i64>) -> Result<ListenSummary> {
        // unique_albums 与 top("album") 同键（album_artist|album），避免两处口径不一致
        let sql = format!(
            "SELECT COUNT(*), COALESCE(SUM(play_ms),0),
                    COUNT(DISTINCT {id}),
                    COUNT(DISTINCT lower(trim(artist))),
                    COUNT(DISTINCT lower(trim(album_artist)) || '|' || lower(trim(album)))
             FROM play_events WHERE 1=1 {yf}",
            id = identity_sql(),
            yf = Self::RANGE_SQL_1
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let row = stmt.query_row(params![since_ms], |r| {
            Ok(ListenSummary {
                total_plays: r.get(0)?,
                total_ms: r.get(1)?,
                unique_tracks: r.get(2)?,
                unique_artists: r.get(3)?,
                unique_albums: r.get(4)?,
            })
        })?;
        Ok(row)
    }

    /// kind: `track` | `album` | `artist`（未知值报错，不静默当 track）。
    /// 注意：title/artist/album 是组内 MAX() 代表值，仅作展示，不代表「最常听」那一首/专。
    pub fn top(
        &self,
        kind: &str,
        limit: i64,
        since_ms: Option<i64>,
    ) -> Result<Vec<TopListenItem>> {
        let limit = limit.clamp(1, 500);
        let yf = Self::RANGE_SQL_2;
        let id = identity_sql();
        let key_expr = match kind {
            "artist" => "lower(trim(artist))",
            "album" => "lower(trim(album_artist)) || '|' || lower(trim(album))",
            "track" => id,
            other => anyhow::bail!("未知 kind: {other}（应为 track | album | artist）"),
        };
        // 常听榜按听歌时长排序（用户口径），次数作次序
        let sql = format!(
            "SELECT {key_expr} AS k,
                    MAX(title) AS title,
                    MAX(artist) AS artist,
                    MAX(album) AS album,
                    COUNT(*) AS plays,
                    COALESCE(SUM(play_ms),0) AS total_ms,
                    COALESCE(MAX(path), '') AS path,
                    COALESCE(MAX(track_duration_ms), 0) AS track_duration_ms
             FROM play_events
             WHERE 1=1 {yf}
             GROUP BY k
             ORDER BY total_ms DESC, plays DESC
             LIMIT ?1",
            key_expr = key_expr,
            yf = yf,
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![limit, since_ms], |r| {
                Ok(TopListenItem {
                    key: r.get::<_, String>(0)?,
                    title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    artist: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    album: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    plays: r.get(4)?,
                    total_ms: r.get(5)?,
                    path: r.get::<_, Option<String>>(6)?.unwrap_or_default(),
                    track_duration_ms: r.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn recent(&self, limit: i64, since_ms: Option<i64>) -> Result<Vec<ListenEvent>> {
        let limit = limit.clamp(1, 500);
        let yf = Self::RANGE_SQL_2;
        let sql = format!(
            "SELECT id, started_at, ended_at, play_ms, track_duration_ms,
                    path, title, artist, album, album_artist, year, track_no,
                    mb_recording_mbid, catalog_id, source, end_reason
             FROM play_events
             WHERE 1=1 {yf}
             ORDER BY started_at DESC
             LIMIT ?1",
            yf = yf
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![limit, since_ms], map_event)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 导入去重键：同一时刻同一文件同一听歌时长视为同一次。
    pub fn has_event(&self, started_at: i64, path: &str, play_ms: i64) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM play_events
             WHERE started_at = ?1 AND path = ?2 AND play_ms = ?3",
            params![started_at, path, play_ms],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// 合并另一份听歌史（导入）。返回 (新增, 跳过重复)。
    /// 分块事务（每 500 条）：免逐条 fsync，中途失败不整批回滚到零；块间释放写锁不堵播放落库。
    pub fn merge_events(&self, events: &[ListenEvent]) -> Result<(i64, i64)> {
        const CHUNK: usize = 500;
        let mut added = 0i64;
        let mut skipped = 0i64;
        for chunk in events.chunks(CHUNK) {
            let tx = self.conn.unchecked_transaction()?;
            for e in chunk {
                let n: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM play_events
                     WHERE started_at = ?1 AND path = ?2 AND play_ms = ?3",
                    params![e.started_at, e.path, e.play_ms],
                    |r| r.get(0),
                )?;
                if n > 0 {
                    skipped += 1;
                    continue;
                }
                tx.execute(
                    "INSERT INTO play_events (
                        started_at, ended_at, play_ms, track_duration_ms,
                        path, title, artist, album, album_artist, year, track_no,
                        mb_recording_mbid, catalog_id, source, end_reason
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
                    params![
                        e.started_at,
                        e.ended_at,
                        e.play_ms,
                        e.track_duration_ms,
                        e.path,
                        e.title,
                        e.artist,
                        e.album,
                        e.album_artist,
                        e.year,
                        e.track_no,
                        e.mb_recording_mbid,
                        e.catalog_id,
                        e.source,
                        e.end_reason,
                    ],
                )?;
                added += 1;
            }
            tx.commit()?;
        }
        Ok((added, skipped))
    }

    /// 从只读源库读出全部听歌事件（导入预览/执行共用）。
    pub fn load_all_from(conn: &Connection) -> Result<Vec<ListenEvent>> {
        if !table_exists(conn, "play_events")? {
            return Ok(Vec::new());
        }
        let mut stmt = conn.prepare(
            "SELECT id, started_at, ended_at, play_ms, track_duration_ms,
                    path, title, artist, album, album_artist, year, track_no,
                    mb_recording_mbid, catalog_id, source, end_reason
             FROM play_events
             ORDER BY started_at",
        )?;
        let rows = stmt
            .query_map([], map_event)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 按日听歌量（本地日切），用于趋势/热力。
    pub fn daily(&self, since_ms: Option<i64>) -> Result<Vec<DailyListen>> {
        let yf = Self::RANGE_SQL_1;
        let sql = format!(
            "SELECT strftime('%Y-%m-%d', started_at / 1000, 'unixepoch', 'localtime') AS d,
                    COUNT(*) AS plays,
                    COALESCE(SUM(play_ms),0) AS total_ms
             FROM play_events
             WHERE 1=1 {yf}
             GROUP BY d
             ORDER BY d",
            yf = yf
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![since_ms], |r| {
                Ok(DailyListen {
                    date: r.get(0)?,
                    plays: r.get(1)?,
                    total_ms: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 时段分布（0–23，本地小时），年报「几点听歌」。
    pub fn hour_hist(&self, since_ms: Option<i64>) -> Result<Vec<i64>> {
        let yf = Self::RANGE_SQL_1;
        let sql = format!(
            "SELECT CAST(strftime('%H', started_at / 1000, 'unixepoch', 'localtime') AS INTEGER) AS h,
                    COUNT(*) AS plays
             FROM play_events
             WHERE 1=1 {yf}
             GROUP BY h",
            yf = yf
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut hist = vec![0i64; 24];
        let mut rows = stmt.query(params![since_ms])?;
        while let Some(row) = rows.next()? {
            let h: i64 = row.get(0)?;
            let n: i64 = row.get(1)?;
            if (0..24).contains(&h) {
                hist[h as usize] = n;
            }
        }
        Ok(hist)
    }
}

fn map_event(r: &rusqlite::Row<'_>) -> rusqlite::Result<ListenEvent> {
    Ok(ListenEvent {
        id: r.get(0)?,
        started_at: r.get(1)?,
        ended_at: r.get(2)?,
        play_ms: r.get(3)?,
        track_duration_ms: r.get(4)?,
        path: r.get(5)?,
        title: r.get(6)?,
        artist: r.get(7)?,
        album: r.get(8)?,
        album_artist: r.get(9)?,
        year: r.get(10)?,
        track_no: r.get(11)?,
        mb_recording_mbid: r.get(12)?,
        catalog_id: r.get(13)?,
        source: r.get(14)?,
        end_reason: r.get(15)?,
    })
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let found = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |_| Ok(()),
        )
        .optional()?;
    Ok(found.is_some())
}

/// 正在累计的一次听歌。
#[derive(Debug, Clone)]
pub struct PendingPlay {
    pub path: String,
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
    pub play_ms: u64,
    pub track_duration_ms: u64,
    pub meta: MetaSnapshot,
    pub source: String,
    last_wall_ms: u64,
    last_position_ms: u64,
    /// 已落库行 id（听满阈值后写入；None = 还在内存）
    event_id: Option<i64>,
    /// 上次落库时的 play_ms
    persisted_ms: u64,
}

/// 内存中的听歌计时器：切歌/退出收口；播放中每 15s 刷库一次（抗强杀）。
pub struct ListenTracker {
    pending: Option<PendingPlay>,
}

impl Default for ListenTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ListenTracker {
    pub fn new() -> Self {
        Self { pending: None }
    }

    /// 结束当前 pending（若未达阈值则静默丢弃）。
    pub fn finish_current(&mut self, reason: EndReason, db: Option<&ListenDb>) {
        if self.pending.is_none() {
            return;
        }
        self.finalize(reason, db);
    }

    /// 当前 pending 路径（切歌前预解析元数据用，避免持锁做文件 I/O）。
    pub fn pending_path(&self) -> Option<&str> {
        self.pending.as_ref().map(|p| p.path.as_str())
    }

    /// 观察一次播放器快照：累计时长 / 切歌落库 / 开新 pending。
    /// `meta` 仅在开新 pending 时使用（调用方应在外侧解析好，勿在持锁时读文件）。
    pub fn observe(
        &mut self,
        snap: &PlayerSnapshot,
        db: Option<&ListenDb>,
        meta: Option<MetaSnapshot>,
    ) {
        let now = now_unix_ms();
        let path = snap.track.as_ref().map(|t| t.path.clone());

        let mut same_track = false;
        let track_changed = match self.pending.as_mut() {
            Some(p) => {
                let same = path.as_deref() == Some(p.path.as_str());
                if same {
                    if snap.status == PlayStatus::Playing {
                        let wall = now.saturating_sub(p.last_wall_ms);
                        let pos = snap.position_ms;
                        let pos_delta = pos.saturating_sub(p.last_position_ms);
                        // 正常轮询：认 wall（seek 前跳不会虚增）。
                        // 间隙过大（托盘节流/卡顿）：认 position，休眠时 position 不涨则不虚增。
                        let delta = if wall >= THROTTLE_GAP_MS {
                            pos_delta.min(wall)
                        } else {
                            wall.min(MAX_TICK_DELTA_MS)
                        };
                        p.play_ms += delta;
                        p.last_position_ms = pos;
                    } else {
                        p.last_position_ms = snap.position_ms;
                    }
                    p.last_wall_ms = now;
                    p.ended_at_ms = now;
                    if snap.duration_ms > 0 {
                        p.track_duration_ms = snap.duration_ms;
                    }
                    same_track = true;
                    false
                } else {
                    true
                }
            }
            None => false,
        };
        if same_track {
            // 听满阈值后定期落库：进程被杀最多丢 FLUSH_EVERY_MS
            self.persist_live(db);
            return;
        }
        if track_changed {
            self.finalize(EndReason::Skip, db);
        }

        if let Some(t) = snap.track.as_ref() {
            let meta = meta.unwrap_or_else(|| MetaSnapshot {
                title: t.title.clone(),
                ..Default::default()
            });
            let dur = if snap.duration_ms > 0 {
                snap.duration_ms
            } else {
                t.duration_ms
            };
            self.pending = Some(PendingPlay {
                path: t.path.clone(),
                started_at_ms: now,
                ended_at_ms: now,
                play_ms: 0,
                track_duration_ms: dur,
                meta,
                source: "player".into(),
                last_wall_ms: now,
                last_position_ms: snap.position_ms,
                event_id: None,
                persisted_ms: 0,
            });
        }
    }

    /// 听满阈值后：首次插入，之后每 FLUSH_EVERY_MS 刷一次（抗强杀）。
    /// 占位 end_reason = interrupted；finalize 收口时改成真实原因。
    fn persist_live(&mut self, db: Option<&ListenDb>) {
        let Some(db) = db else { return };
        let Some(p) = self.pending.as_mut() else {
            return;
        };
        if p.play_ms < MIN_PLAY_MS {
            return;
        }
        let due = match p.event_id {
            None => true,
            Some(_) => p.play_ms.saturating_sub(p.persisted_ms) >= FLUSH_EVERY_MS,
        };
        if !due {
            return;
        }
        match p.event_id {
            None => match db.insert_event(p, EndReason::Interrupted) {
                Ok(id) => {
                    p.event_id = Some(id);
                    p.persisted_ms = p.play_ms;
                }
                Err(e) => eprintln!("[AxMusic] 听歌记录写入失败: {e}"),
            },
            Some(id) => {
                if let Err(e) = db.update_event(id, p, EndReason::Interrupted) {
                    eprintln!("[AxMusic] 听歌记录刷新失败: {e}");
                } else {
                    p.persisted_ms = p.play_ms;
                }
            }
        }
    }

    fn finalize(&mut self, reason: EndReason, db: Option<&ListenDb>) {
        let Some(mut p) = self.pending.take() else {
            return;
        };
        p.ended_at_ms = now_unix_ms().max(p.started_at_ms);
        let Some(db) = db else {
            return;
        };
        // 已有落库行：收口更新真实原因；否则达阈值才插入
        if let Some(id) = p.event_id {
            if let Err(e) = db.update_event(id, &p, reason) {
                eprintln!("[AxMusic] 听歌记录收口失败: {e}");
            }
            return;
        }
        if !ListenDb::counts_as_play(&p) {
            return;
        }
        if let Err(e) = db.insert_event(&p, reason) {
            eprintln!("[AxMusic] 听歌记录写入失败: {e}");
        }
    }
}

/// 从库表或音频标签解析元数据快照（可能做文件 I/O，**调用方勿持 listen 锁**）。
pub(crate) fn resolve_meta(path: &str, title_hint: &str, lib: Option<&LibraryDb>) -> MetaSnapshot {
    if let Some(db) = lib {
        if let Ok(Some(row)) = db.get_track_by_path(path) {
            return MetaSnapshot {
                title: non_empty(row.title, title_hint),
                artist: row.artist,
                album: row.album,
                album_artist: row.album_artist,
                year: row.year,
                track_no: row.track_no,
                mb_recording_mbid: row.mb_recording_mbid,
                catalog_id: row.catalog_id,
            };
        }
    }
    // 库外 / 未入库：读标签一次（仅切歌时）
    if let Ok(row) = crate::scanner::read_track(Path::new(path)) {
        return MetaSnapshot {
            title: non_empty(row.title, title_hint),
            artist: row.artist,
            album: row.album,
            album_artist: row.album_artist,
            year: row.year,
            track_no: row.track_no,
            mb_recording_mbid: row.mb_recording_mbid,
            catalog_id: None,
        };
    }
    MetaSnapshot {
        title: title_hint.to_string(),
        ..Default::default()
    }
}

fn non_empty(v: String, fallback: &str) -> String {
    if v.trim().is_empty() {
        fallback.to_string()
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(play_ms: u64, duration_ms: u64) -> PendingPlay {
        PendingPlay {
            path: "x.flac".into(),
            started_at_ms: 0,
            ended_at_ms: play_ms,
            play_ms,
            track_duration_ms: duration_ms,
            meta: MetaSnapshot::default(),
            source: "player".into(),
            last_wall_ms: 0,
            last_position_ms: 0,
            event_id: None,
            persisted_ms: 0,
        }
    }

    #[test]
    fn threshold_twenty_seconds() {
        assert!(!ListenDb::counts_as_play(&pending(19_000, 180_000)));
        assert!(ListenDb::counts_as_play(&pending(20_000, 180_000)));
        // 长曲同样只要 20s
        assert!(ListenDb::counts_as_play(&pending(21_000, 600_000)));
        // 短于 20s 不记
        assert!(!ListenDb::counts_as_play(&pending(5_000, 8_000)));
        assert!(ListenDb::counts_as_play(&pending(20_000, 8_000)));
    }

    /// 回归：since 参数位（?1 / ?2）与 query 参数个数必须对齐
    #[test]
    fn queries_bind_since_correctly() {
        let db = ListenDb::open_in_memory().unwrap();
        let mut p = pending(30_000, 180_000);
        p.meta.title = "t".into();
        p.meta.artist = "a".into();
        let id = db.insert_event(&p, EndReason::Natural).unwrap();
        assert!(id > 0);
        for since in [None, Some(0i64), Some(i64::MAX)] {
            db.summary(since).unwrap();
            db.top("track", 5, since).unwrap();
            db.top("artist", 5, since).unwrap();
            db.recent(5, since).unwrap();
            db.hour_hist(since).unwrap();
            db.daily(since).unwrap();
        }
    }
}
