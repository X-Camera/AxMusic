import { useCallback, useEffect, useMemo, useState } from "react";
import { LayoutGrid, List, ListEnd, ListPlus, Play } from "lucide-react";

import { api, formatTime, trackRowToAddItem, trackRowToQueueItem } from "../../lib/api";
import type { PlaylistAddItem, TrackRow } from "../../lib/types";
import { useApp } from "../../state/useApp";
import { TopBar } from "../../components/TopBar";
import { FavoriteHeart } from "../../components/FavoriteHeart";
import { PlaylistPicker } from "../playlists/PlaylistPicker";
import { AlbumCover } from "./AlbumCover";
import "./Songs.css";

type ViewMode = "list" | "grid";

const SONGS_LIMIT = 10000;

export function SongsPage() {
  const playQueue = useApp((s) => s.playQueue);
  const enqueue = useApp((s) => s.enqueue);
  const [tracks, setTracks] = useState<TrackRow[]>([]);
  const [totalCount, setTotalCount] = useState<number | null>(null);
  const [query, setQuery] = useState("");
  const [viewMode, setViewMode] = useState<ViewMode>("list");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pickerItems, setPickerItems] = useState<PlaylistAddItem[] | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void api.getSettings().then((s) => {
      if (!cancelled) setViewMode(s.songs_view);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [list, total] = await Promise.all([
        api.getTracks({ sort: "title", limit: SONGS_LIMIT }),
        api.getTrackCount(),
      ]);
      setTracks(list);
      setTotalCount(total);
    } catch (e) {
      // 保留旧列表，错误单独提示；空态与故障态分开
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  /** 列表被 limit 截断（库比一页大）：提示并引导用搜索过滤 */
  const truncated = totalCount != null && totalCount > tracks.length;

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return tracks;
    return tracks.filter(
      (t) =>
        (t.title || t.filename).toLowerCase().includes(q) ||
        t.artist.toLowerCase().includes(q) ||
        t.album.toLowerCase().includes(q) ||
        t.album_artist.toLowerCase().includes(q),
    );
  }, [tracks, query]);

  /** 点单曲：只播这一首，并替换当前播放队列 */
  function playOne(t: TrackRow) {
    void playQueue([trackRowToQueueItem(t)], 0);
  }

  /** 播放全部：整页入队 */
  function playAll() {
    if (filtered.length === 0) return;
    void playQueue(filtered.map(trackRowToQueueItem), 0);
  }

  function switchView(mode: ViewMode) {
    setViewMode(mode);
    void api.updateSettings({ songs_view: mode }).catch(() => {});
  }

  function showToast(msg: string) {
    setToast(msg);
    window.setTimeout(() => setToast(null), 2000);
  }

  const viewToggle = (
    <div className="view-toggle" role="group" aria-label="显示模式">
      <button
        className={`view-toggle-btn${viewMode === "list" ? " active" : ""}`}
        title="列表"
        aria-pressed={viewMode === "list"}
        onClick={() => switchView("list")}
      >
        <List size={15} />
      </button>
      <button
        className={`view-toggle-btn${viewMode === "grid" ? " active" : ""}`}
        title="卡片"
        aria-pressed={viewMode === "grid"}
        onClick={() => switchView("grid")}
      >
        <LayoutGrid size={15} />
      </button>
    </div>
  );

  return (
    <>
      <TopBar
        title="歌曲"
        searchValue={query}
        onSearch={setQuery}
        actions={
          <>
            {viewToggle}
            {filtered.length > 0 && (
              <>
                {toast && <span className="ok-toast">{toast}</span>}
                <button
                  className="btn"
                  title="全部加入歌单"
                  onClick={() => setPickerItems(filtered.map(trackRowToAddItem))}
                >
                  <ListPlus size={15} /> 加入歌单
                </button>
                <button className="btn btn-primary" title="播放全部" onClick={() => playAll()}>
                  <Play size={15} /> 播放全部
                </button>
              </>
            )}
          </>
        }
      />
      <div className="page-scroll">
        {truncated && (
          <div className="notice-line">
            库共 {totalCount} 首，这里仅显示前 {tracks.length} 首；用顶部搜索缩小范围。
          </div>
        )}
        {loading && tracks.length === 0 ? (
          <div className="empty-state">加载中…</div>
        ) : error && tracks.length === 0 ? (
          <div className="empty-state">
            <div className="display" style={{ fontSize: 20 }}>
              加载失败
            </div>
            <p className="muted">{error}</p>
            <button className="btn" onClick={() => void reload()}>
              重试
            </button>
          </div>
        ) : tracks.length === 0 ? (
          <div className="empty-state">
            <div className="display" style={{ fontSize: 20 }}>
              还没有歌曲
            </div>
            <p className="muted">
              到侧栏「管理」初始化库目录并刷新扫描，或先把音频放进库文件夹再扫描。
            </p>
          </div>
        ) : filtered.length === 0 ? (
          <div className="empty-state">
            <p className="muted">没有匹配「{query}」的歌曲</p>
          </div>
        ) : viewMode === "list" ? (
          <div className="songs-list" key="list">
            <div className="songs-head tertiary">
              <span />
              <span />
              <span>曲名</span>
              <span>歌手</span>
              <span>专辑</span>
              <span />
              <span>时长</span>
            </div>
            {filtered.map((t, idx) => (
              <div
                key={t.id}
                className="song-row"
                title="播放"
                onClick={() => playOne(t)}
              >
                <span className="tertiary mono">{String(idx + 1).padStart(2, "0")}</span>
                <span className="fav-col">
                  <FavoriteHeart item={trackRowToAddItem(t)} />
                </span>
                <span className="ellipsis">{t.title || t.filename}</span>
                <span className="tertiary ellipsis">{t.artist || "—"}</span>
                <span className="tertiary ellipsis">{t.album || "—"}</span>
                <span className="song-actions">
                  <button
                    className="link-btn"
                    title="加入歌单"
                    onClick={(e) => {
                      e.stopPropagation();
                      setPickerItems([trackRowToAddItem(t)]);
                    }}
                  >
                    <ListPlus size={13} />
                  </button>
                </span>
                <span className="tertiary mono">{formatTime(t.duration_ms)}</span>
              </div>
            ))}
          </div>
        ) : (
          <div className="songs-grid" key="grid">
            {filtered.map((t) => (
              <div
                key={t.id}
                className="song-card"
                title="播放"
                onClick={() => playOne(t)}
              >
                <div className="song-card-cover">
                  <AlbumCover
                    path={t.path}
                    mtime={t.mtime}
                    hasCover={t.has_cover}
                    initial={(t.title || t.filename || "?").slice(0, 1).toUpperCase()}
                  />
                  <span className="song-card-play" aria-hidden>
                    <Play size={36} fill="currentColor" strokeWidth={0} />
                  </span>
                  <button
                    type="button"
                    className="song-card-add"
                    title="加入队列"
                    onClick={(e) => {
                      e.stopPropagation();
                      void enqueue([trackRowToQueueItem(t)]);
                    }}
                  >
                    <ListEnd size={14} />
                  </button>
                </div>
                <div className="song-card-title-row">
                  <FavoriteHeart item={trackRowToAddItem(t)} />
                  <div className="song-card-title">{t.title || t.filename}</div>
                </div>
                <div className="song-card-sub tertiary">
                  <span className="ellipsis">
                    {t.artist || "—"}
                    {t.album ? ` · ${t.album}` : ""}
                  </span>
                  <span className="mono song-card-dur">{formatTime(t.duration_ms)}</span>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
      {pickerItems && (
        <PlaylistPicker
          items={pickerItems}
          onClose={() => setPickerItems(null)}
          onAdded={(name) => {
            setPickerItems(null);
            showToast(`已加入「${name}」`);
          }}
        />
      )}
    </>
  );
}
