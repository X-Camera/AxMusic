import { useCallback, useEffect, useState } from "react";
import { Play } from "lucide-react";

import { api, trackRowToQueueItem } from "../../lib/api";
import type { AlbumCard, TrackRow } from "../../lib/types";
import { useApp } from "../../state/useApp";
import { TopBar } from "../../components/TopBar";
import "./AlbumWall.css";

export function AlbumWallPage() {
  const playQueue = useApp((s) => s.playQueue);
  const [albums, setAlbums] = useState<AlbumCard[]>([]);
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);
  const [tracks, setTracks] = useState<TrackRow[]>([]);
  const [loading, setLoading] = useState(false);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const list = await api.getAlbums();
      setAlbums(list);
    } catch {
      setAlbums([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const filtered = albums.filter((a) => {
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return (
      a.album.toLowerCase().includes(q) ||
      a.album_artist.toLowerCase().includes(q) ||
      a.year.includes(q)
    );
  });

  async function openAlbum(a: AlbumCard) {
    const key = `${a.album_artist}::${a.album}`;
    if (expanded === key) {
      setExpanded(null);
      setTracks([]);
      return;
    }
    setExpanded(key);
    try {
      const list = await api.getAlbumTracks(a.album, a.album_artist);
      setTracks(list);
    } catch {
      setTracks([]);
    }
  }

  async function playAlbum(a: AlbumCard, e: React.MouseEvent) {
    e.stopPropagation();
    const list = await api.getAlbumTracks(a.album, a.album_artist).catch(() => [] as TrackRow[]);
    if (list.length === 0) return;
    await playQueue(list.map(trackRowToQueueItem), 0);
  }

  return (
    <>
      <TopBar title="专辑墙" searchValue={query} onSearch={setQuery} />
      <div className="page-scroll">
        {loading && albums.length === 0 ? (
          <div className="empty-state">加载中…</div>
        ) : filtered.length === 0 ? (
          <div className="empty-state">
            <div className="display" style={{ fontSize: 20 }}>
              还没有专辑
            </div>
            <p className="muted">
              到侧栏「管理」初始化库目录并刷新扫描，或先把音频放进库文件夹再扫描。
            </p>
          </div>
        ) : (
          <div className="album-wall">
            {filtered.map((a) => {
              const key = `${a.album_artist}::${a.album}`;
              const open = expanded === key;
              return (
                <div key={key} className={`album-cell${open ? " open" : ""}`}>
                  <button className="album-card" onClick={() => void openAlbum(a)}>
                    <div className={`album-cover${a.has_cover ? "" : " placeholder"}`}>
                      <span className="album-initial">
                        {(a.album || a.album_artist || "?").slice(0, 1).toUpperCase()}
                      </span>
                      <span
                        className="album-play"
                        title="播放专辑"
                        onClick={(e) => void playAlbum(a, e)}
                      >
                        <Play size={16} fill="currentColor" />
                      </span>
                    </div>
                    <div className="album-name">{a.album || "Unknown Album"}</div>
                    <div className="album-sub tertiary">
                      {a.album_artist || "Unknown Artist"}
                      {a.year ? ` · ${a.year}` : ""} · {a.track_count}
                    </div>
                  </button>
                  {open && (
                    <div className="album-tracks panel">
                      {tracks.length === 0 ? (
                        <div className="tertiary" style={{ padding: 12 }}>
                          无曲目
                        </div>
                      ) : (
                        tracks.map((t, idx) => (
                          <button
                            key={t.id}
                            className="album-track-row"
                            onDoubleClick={() => void playQueue(tracks.map(trackRowToQueueItem), idx)}
                            onClick={() => void playQueue(tracks.map(trackRowToQueueItem), idx)}
                            title="播放"
                          >
                            <span className="tertiary mono">{String(idx + 1).padStart(2, "0")}</span>
                            <span className="ellipsis">{t.title || t.filename}</span>
                            <span className="tertiary mono">
                              {Math.floor(t.duration_ms / 60000)}:
                              {String(Math.floor((t.duration_ms % 60000) / 1000)).padStart(2, "0")}
                            </span>
                          </button>
                        ))
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </>
  );
}
