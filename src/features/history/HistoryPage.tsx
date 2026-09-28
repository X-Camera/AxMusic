import { useCallback, useEffect, useRef, useState } from "react";
import { Clock3, Headphones, ListMusic, Music2, RefreshCw, UserRound } from "lucide-react";

import { TopBar } from "../../components/TopBar";
import { api, formatTime } from "../../lib/api";
import type { ListenEvent, ListenSummary, TopListenItem } from "../../lib/types";
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

function formatWhen(ts: number): string {
  const d = new Date(ts);
  const pad = (n: number) => n.toString().padStart(2, "0");
  const now = new Date();
  const sameYear = d.getFullYear() === now.getFullYear();
  const date = sameYear
    ? `${d.getMonth() + 1}月${d.getDate()}日`
    : `${d.getFullYear()}年${d.getMonth() + 1}月${d.getDate()}日`;
  return `${date} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
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
        api.listenTop("track", 10, since),
        api.listenTop("artist", 8, since),
        api.listenHourHist(since),
        api.listenRecent(40, since),
      ]);
      if (!aliveRef.current || seq !== reloadSeqRef.current) return;
      setSummary(sum);
      setTopTracks(tracks);
      setTopArtists(artists);
      setHourHist(hist);
      setRecent(list);
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
                <h2>常听歌曲</h2>
                {topTracks.length === 0 ? (
                  <p className="history-muted">暂无数据</p>
                ) : (
                  <ol className="history-list">
                    {topTracks.map((t, i) => (
                      <li key={t.key}>
                        <span className="history-rank">{i + 1}</span>
                        <div className="history-item-main">
                          <div className="history-item-title">{t.title || t.key}</div>
                          <div className="history-item-sub">{t.artist || "未知歌手"}</div>
                        </div>
                        <div className="history-item-meta">
                          <div>{t.plays} 次</div>
                          <div className="tertiary">{formatListenMs(t.total_ms)}</div>
                        </div>
                      </li>
                    ))}
                  </ol>
                )}
              </section>

              <section className="history-panel">
                <h2>常听歌手</h2>
                {topArtists.length === 0 ? (
                  <p className="history-muted">暂无数据</p>
                ) : (
                  <ol className="history-list">
                    {topArtists.map((t, i) => (
                      <li key={t.key}>
                        <span className="history-rank">{i + 1}</span>
                        <div className="history-item-main">
                          {/* 后端 album 是组内 MAX 代表值，不作副标题，避免误读为「代表专辑」 */}
                          <div className="history-item-title">{t.artist || t.key}</div>
                        </div>
                        <div className="history-item-meta">
                          <div>{t.plays} 次</div>
                          <div className="tertiary">{formatListenMs(t.total_ms)}</div>
                        </div>
                      </li>
                    ))}
                  </ol>
                )}
              </section>
            </div>

            <section className="history-panel">
              <h2>最近播放</h2>
              {recent.length === 0 ? (
                <p className="history-muted">暂无数据</p>
              ) : (
                <ol className="history-list">
                  {recent.map((e) => (
                    <li key={e.id}>
                      <div className="history-item-main">
                        <div className="history-item-title">{e.title || e.path}</div>
                        <div className="history-item-sub">
                          {e.artist || "未知歌手"}
                          {e.album ? ` · ${e.album}` : ""}
                        </div>
                      </div>
                      <div className="history-item-meta">
                        <div>{formatWhen(e.started_at)}</div>
                        <div className="tertiary">听 {formatTime(e.play_ms)}</div>
                      </div>
                    </li>
                  ))}
                </ol>
              )}
            </section>
          </>
        )}
      </div>
    </>
  );
}
