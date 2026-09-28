import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Clock3, Headphones, ListMusic, Music2, Play, RefreshCw, UserRound } from "lucide-react";

import { TopBar } from "../../components/TopBar";
import { api } from "../../lib/api";
import type { ListenEvent, ListenSummary, QueueItem, TopListenItem } from "../../lib/types";
import { useApp } from "../../state/useApp";
import { AlbumCover } from "../browse/AlbumCover";
import "./History.css";

type RangeKey = "7d" | "30d" | "all";

const RANGE_OPTIONS: { key: RangeKey; label: string; days?: number }[] = [
  { key: "7d", label: "近 7 天", days: 7 },
  { key: "30d", label: "近 30 天", days: 30 },
  { key: "all", label: "全部" },
];

const DAY_MS = 24 * 60 * 60 * 1000;
/** 与 History.css 中 .history-hours-bars height / .history-hour-bar min-height 对应 */
const HOUR_BAR_MAX_PX = 72;
const HOUR_BAR_MIN_PX = 4;
/** 常听歌曲 / 常听歌手条数 */
const TOP_TRACKS = 10;
const TOP_ARTISTS = 8;
/** 最近播放去重后条数 */
const RECENT_LIMIT = 12;
/** 最近播放多取一些再按歌去重 */
const RECENT_FETCH = 80;

function rangeSince(key: RangeKey): number | undefined {
  const days = RANGE_OPTIONS.find((r) => r.key === key)?.days;
  if (days == null) return undefined;
  return Date.now() - days * DAY_MS;
}

function formatListenMs(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) ms = 0;
  const totalMin = Math.floor(ms / 60000);
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  if (h > 0) return `${h} 小时 ${m} 分`;
  return `${m} 分`;
}

/** 最近播放相对时间：刚刚 / x 小时前 / 昨天 / 前天 / N 天前 / N 周前 / N 个月前 / N 年前 */
function formatRelativeWhen(ts: number): string {
  const now = Date.now();
  const diff = Math.max(0, now - ts);
  const min = Math.floor(diff / 60000);
  if (min < 30) return "刚刚";

  const nowD = new Date(now);
  const then = new Date(ts);
  const startOf = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const dayDiff = Math.round((startOf(nowD) - startOf(then)) / DAY_MS);

  if (dayDiff <= 0) {
    const h = Math.floor(min / 60);
    return h <= 1 ? "1 小时前" : `${h} 小时前`;
  }
  if (dayDiff === 1) return "昨天";
  if (dayDiff === 2) return "前天";
  if (dayDiff < 7) return `${dayDiff} 天前`;

  const weeks = Math.floor(dayDiff / 7);
  if (dayDiff < 30) return `${weeks} 周前`;

  const months = Math.floor(dayDiff / 30);
  if (dayDiff < 365) return `${months} 个月前`;

  return `${Math.floor(dayDiff / 365)} 年前`;
}

function trackInitial(title: string, fallback: string): string {
  return (title || fallback || "?").slice(0, 1).toUpperCase();
}

function topToQueueItem(t: TopListenItem): QueueItem {
  return {
    path: t.path,
    title: t.title || t.path.split(/[\\/]/).pop() || t.path,
    duration_ms: t.track_duration_ms,
  };
}

function listenToQueueItem(e: ListenEvent): QueueItem {
  return {
    path: e.path,
    title: e.title || e.path.split(/[\\/]/).pop() || e.path,
    duration_ms: e.track_duration_ms,
  };
}

/** 同一首歌只留最近一次（path 优先，空 path 退 title|artist） */
function dedupeRecent(list: ListenEvent[]): ListenEvent[] {
  const seen = new Set<string>();
  const out: ListenEvent[] = [];
  for (const e of list) {
    const key = e.path || `${e.title}|${e.artist}`.toLowerCase();
    if (!key || seen.has(key)) continue;
    seen.add(key);
    out.push(e);
  }
  return out;
}

const EMPTY_SUMMARY: ListenSummary = {
  total_plays: 0,
  total_ms: 0,
  unique_tracks: 0,
  unique_artists: 0,
  unique_albums: 0,
};

/** 统计页：近 7 天 / 近 30 天 / 全部 三档；数据源 data_root/listen_history.db */
export function HistoryPage() {
  const playQueue = useApp((s) => s.playQueue);
  const requestOpenArtist = useApp((s) => s.requestOpenArtist);
  const [range, setRange] = useState<RangeKey>("7d");
  const [summary, setSummary] = useState<ListenSummary>(EMPTY_SUMMARY);
  const [topTracks, setTopTracks] = useState<TopListenItem[]>([]);
  const [topArtists, setTopArtists] = useState<TopListenItem[]>([]);
  const [recent, setRecent] = useState<ListenEvent[]>([]);
  const [hourHist, setHourHist] = useState<number[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  /** 请求代次：快速切换时间范围时丢弃过期响应 */
  const reloadSeqRef = useRef(0);
  const aliveRef = useRef(true);

  useEffect(() => {
    aliveRef.current = true;
    return () => {
      aliveRef.current = false;
    };
  }, []);

  const reload = useCallback(async () => {
    const seq = ++reloadSeqRef.current;
    const since = rangeSince(range);
    setLoading(true);
    setError(null);
    try {
      const [sum, tracks, artists, hist, list] = await Promise.all([
        api.listenSummary(since),
        api.listenTop("track", TOP_TRACKS, since),
        api.listenTop("artist", TOP_ARTISTS, since),
        api.listenHourHist(since),
        api.listenRecent(RECENT_FETCH, since),
      ]);
      if (!aliveRef.current || seq !== reloadSeqRef.current) return;
      setSummary(sum);
      setTopTracks(tracks);
      setTopArtists(artists);
      setHourHist(hist);
      setRecent(dedupeRecent(list).slice(0, RECENT_LIMIT));
    } catch (e) {
      if (!aliveRef.current || seq !== reloadSeqRef.current) return;
      setError(String(e));
    } finally {
      if (aliveRef.current && seq === reloadSeqRef.current) setLoading(false);
    }
  }, [range]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const maxHour = Math.max(1, ...hourHist);
  const empty = !error && !loading && summary.total_plays === 0;

  const topQueue = useMemo(() => topTracks.map(topToQueueItem), [topTracks]);
  const recentQueue = useMemo(() => recent.map(listenToQueueItem), [recent]);

  /** 点卡片：整列表入队并从这首起播 */
  function playFrom(items: QueueItem[], index: number) {
    if (items.length === 0) return;
    const start = Math.max(0, Math.min(index, items.length - 1));
    void playQueue(items, start);
  }

  function playAll(items: QueueItem[]) {
    if (items.length === 0) return;
    void playQueue(items, 0);
  }

  return (
    <>
      <TopBar
        title="统计"
        actions={
          <>
            <div className="history-ranges" role="tablist" aria-label="时间范围">
              {RANGE_OPTIONS.map((r) => (
                <button
                  key={r.key}
                  type="button"
                  role="tab"
                  aria-selected={range === r.key}
                  className={`chip${range === r.key ? " active" : ""}`}
                  onClick={() => setRange(r.key)}
                >
                  {r.label}
                </button>
              ))}
            </div>
            <button
              type="button"
              className="btn history-refresh"
              title="刷新"
              onClick={() => void reload()}
              disabled={loading}
            >
              <RefreshCw size={15} className={loading ? "spin" : undefined} /> 刷新
            </button>
          </>
        }
      />
      <div className="page-scroll history-scroll">
        {error && <div className="history-error">{error}</div>}
        {empty && (
          <div className="history-empty">
            <Headphones size={36} className="tertiary" />
            <p>
              {range === "all"
                ? "还没有听歌记录。听满 20 秒后会自动记一笔。"
                : "这段时间还没有听歌记录。"}
            </p>
          </div>
        )}

        {!empty && (
          <>
            <section className="history-cards">
              <article className="history-card">
                <Music2 size={18} className="accent-icon" />
                <div>
                  <div className="history-card-value">{summary.total_plays}</div>
                  <div className="history-card-label">播放次数</div>
                </div>
              </article>
              <article className="history-card">
                <Clock3 size={18} className="accent-icon" />
                <div>
                  <div className="history-card-value">{formatListenMs(summary.total_ms)}</div>
                  <div className="history-card-label">听歌时长</div>
                </div>
              </article>
              <article className="history-card">
                <ListMusic size={18} className="accent-icon" />
                <div>
                  <div className="history-card-value">{summary.unique_tracks}</div>
                  <div className="history-card-label">不同曲目</div>
                </div>
              </article>
              <article className="history-card">
                <UserRound size={18} className="accent-icon" />
                <div>
                  <div className="history-card-value">{summary.unique_artists}</div>
                  <div className="history-card-label">不同歌手</div>
                </div>
              </article>
            </section>

            <section className="history-panel">
              <h2>几点听歌</h2>
              <p className="history-muted history-hint">
                当前所选时间范围内的本地时段分布
              </p>
              <div className="history-hours" aria-label="时段分布">
                <div className="history-hours-bars">
                  {hourHist.map((n, h) => (
                    <div key={h} className="history-hour-col" title={`${h}:00 · ${n} 次`}>
                      <div
                        className="history-hour-bar"
                        style={{
                          height: `${Math.max(
                            HOUR_BAR_MIN_PX,
                            Math.round((n / maxHour) * HOUR_BAR_MAX_PX),
                          )}px`,
                        }}
                      />
                    </div>
                  ))}
                </div>
                <div className="history-hours-labels" aria-hidden="true">
                  {hourHist.map((_, h) => (
                    <span key={h} className="history-hour-label">
                      {h % 3 === 0 ? h : ""}
                    </span>
                  ))}
                </div>
              </div>
            </section>

            <div className="history-grid">
              <section className="history-panel">
                <div className="history-panel-head">
                  <h2>常听歌曲</h2>
                  {topTracks.length > 0 && (
                    <button
                      type="button"
                      className="btn btn-primary history-play-all"
                      title="播放全部"
                      onClick={() => playAll(topQueue)}
                    >
                      <Play size={14} /> 播放全部
                    </button>
                  )}
                </div>
                {topTracks.length === 0 ? (
                  <p className="history-muted">暂无数据</p>
                ) : (
                  <div className="history-card-grid history-card-grid-compact">
                    {topTracks.map((t, i) => (
                      <button
                        key={t.key}
                        type="button"
                        className="history-song-card"
                        title="播放"
                        onClick={() => playFrom(topQueue, i)}
                      >
                        <div className="history-song-cover">
                          <AlbumCover
                            path={t.path || null}
                            mtime={0}
                            hasCover={!!t.path}
                            initial={trackInitial(t.title, t.key)}
                          />
                          <span className="history-rank-badge">{i + 1}</span>
                          <span className="history-card-play" aria-hidden>
                            <Play size={22} fill="currentColor" strokeWidth={0} />
                          </span>
                        </div>
                        <div className="history-song-title">{t.title || t.key}</div>
                        <div className="history-song-meta">
                          <span>{formatListenMs(t.total_ms)}</span>
                          <span className="tertiary">{t.plays} 次</span>
                        </div>
                      </button>
                    ))}
                  </div>
                )}
              </section>

              <section className="history-panel">
                <div className="history-panel-head">
                  <h2>常听歌手</h2>
                </div>
                {topArtists.length === 0 ? (
                  <p className="history-muted">暂无数据</p>
                ) : (
                  <div className="history-artist-grid history-artist-grid-compact">
                    {topArtists.map((t, i) => (
                      <button
                        key={t.key}
                        type="button"
                        className="history-artist-card"
                        title="查看歌手"
                        onClick={() => {
                          const name = t.artist || t.key;
                          if (name) requestOpenArtist(name);
                        }}
                      >
                        <div className="history-artist-avatar">
                          <AlbumCover
                            path={t.path || null}
                            mtime={0}
                            hasCover={!!t.path}
                            initial={trackInitial(t.artist, t.key)}
                          />
                          <span className="history-rank-badge">{i + 1}</span>
                        </div>
                        <div className="history-artist-name">{t.artist || t.key}</div>
                        <div className="history-artist-meta">
                          <span>{formatListenMs(t.total_ms)}</span>
                          <span className="tertiary"> · {t.plays} 次</span>
                        </div>
                      </button>
                    ))}
                  </div>
                )}
              </section>
            </div>

            <section className="history-panel">
              <div className="history-panel-head">
                <h2>最近播放</h2>
                {recent.length > 0 && (
                  <button
                    type="button"
                    className="btn btn-primary history-play-all"
                    title="播放全部"
                    onClick={() => playAll(recentQueue)}
                  >
                    <Play size={14} /> 播放全部
                  </button>
                )}
              </div>
              {recent.length === 0 ? (
                <p className="history-muted">暂无数据</p>
              ) : (
                <div className="history-card-grid history-card-grid-compact">
                  {recent.map((e, i) => (
                    <button
                      key={e.id}
                      type="button"
                      className="history-song-card"
                      title="播放"
                      onClick={() => playFrom(recentQueue, i)}
                    >
                      <div className="history-song-cover">
                        <AlbumCover
                          path={e.path || null}
                          mtime={0}
                          hasCover={!!e.path}
                          initial={trackInitial(e.title, e.path)}
                        />
                        <span className="history-card-play" aria-hidden>
                          <Play size={22} fill="currentColor" strokeWidth={0} />
                        </span>
                      </div>
                      <div className="history-song-title">{e.title || e.path}</div>
                      <div className="history-song-meta">
                        <span className="tertiary">{formatRelativeWhen(e.started_at)}</span>
                      </div>
                    </button>
                  ))}
                </div>
              )}
            </section>
          </>
        )}
      </div>
    </>
  );
}
