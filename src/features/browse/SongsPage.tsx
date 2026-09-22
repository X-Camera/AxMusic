import { useCallback, useEffect, useMemo, useState } from "react";
import { LayoutGrid, List, ListPlus, Play } from "lucide-react";

import { api, formatTime, trackRowToAddItem, trackRowToQueueItem } from "../../lib/api";
import type { PlaylistAddItem, TrackRow } from "../../lib/types";
import { useApp } from "../../state/useApp";
import { TopBar } from "../../components/TopBar";
import { PlaylistPicker } from "../playlists/PlaylistPicker";
import { AlbumCover } from "./AlbumCover";
import "./Songs.css";

type ViewMode = "list" | "grid";

const VIEW_KEY = "axmusic.songs.view";

function loadViewMode(): ViewMode {
  try {
    return window.localStorage.getItem(VIEW_KEY) === "grid" ? "grid" : "list";
  } catch {
    return "list";
  }
}

export function SongsPage() {
  const playQueue = useApp((s) => s.playQueue);
  const [tracks, setTracks] = useState<TrackRow[]>([]);
  const [query, setQuery] = useState("");
  const [viewMode, setViewMode] = useState<ViewMode>(loadViewMode);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pickerItems, setPickerItems] = useState<PlaylistAddItem[] | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await api.getTracks({ sort: "title", limit: 10000 });
      setTracks(list);
    } catch (e) {
      setTracks([]);
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

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

  function playFrom(idx: number) {
    if (filtered.length === 0) return;
    void playQueue(filtered.map(trackRowToQueueItem), idx);
  }

  function switchView(mode: ViewMode) {
    setViewMode(mode);
    try {
      window.localStorage.setItem(VIEW_KEY, mode);
    } catch {
      /* ignore */
    }
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
                <button className="btn btn-primary" title="播放全部" onClick={() => playFrom(0)}>
                  <Play size={15} /> 播放全部
                </button>
              </>
            )}
          </>
        }
      />
      <div className="page-scroll">
        {loading && tracks.length === 0 ? (
          <div className="empty-state">加载中…</div>
        ) : tracks.length === 0 ? (
          <div className="empty-state">
            <div className="display" style={{ fontSize: 20 }}>
              还没有歌曲
            </div>
            <p className="muted">
              {error
                ? error
                : "到侧栏「管理」初始化库目录并刷新扫描，或先把音频放进库文件夹再扫描。"}
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
                onClick={() => playFrom(idx)}
              >
                <span className="tertiary mono">{String(idx + 1).padStart(2, "0")}</span>
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
            {filtered.map((t, idx) => (
              <button
                key={t.id}
                className="song-card"
                title="播放"
                onClick={() => playFrom(idx)}
              >
                <div className="song-card-cover">
                  <AlbumCover
                    path={t.path}
                    mtime={t.mtime}
                    hasCover={t.has_cover}
                    initial={(t.title || t.filename || "?").slice(0, 1).toUpperCase()}
                  />
                  <span
                    className="song-card-play"
                    title="播放"
                    onClick={(e) => {
                      e.stopPropagation();
                      playFrom(idx);
                    }}
                  >
                    <Play size={16} fill="currentColor" />
                  </span>
                  <span
                    className="song-card-add"
                    title="加入歌单"
                    onClick={(e) => {
                      e.stopPropagation();
                      setPickerItems([trackRowToAddItem(t)]);
                    }}
                  >
                    <ListPlus size={14} />
                  </span>
                </div>
                <div className="song-card-title">{t.title || t.filename}</div>
                <div className="song-card-sub tertiary">
                  {t.artist || "—"}
                  {t.album ? ` · ${t.album}` : ""}
                </div>
              </button>
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
