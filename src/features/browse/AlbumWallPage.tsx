import { useCallback, useEffect, useRef, useState } from "react";
import { friendlyErr } from "../../lib/errors";
import { Disc3, ListPlus, Play } from "lucide-react";

import { api, formatTime, trackRowToAddItem, trackRowToQueueItem } from "../../lib/api";
import type { AlbumCard, PlaylistAddItem, TrackRow } from "../../lib/types";
import { useToast } from "../../lib/useToast";
import { useApp } from "../../state/useApp";
import { TopBar } from "../../components/TopBar";
import { FavoriteHeart } from "../../components/FavoriteHeart";
import { VirtualList, LIST_ROW_HEIGHT } from "../../components/VirtualList";
import { PlaylistPicker } from "../playlists/PlaylistPicker";
import { AlbumCover } from "./AlbumCover";
import "./AlbumWall.css";

export function AlbumWallPage() {
  const playQueue = useApp((s) => s.playQueue);
  const [albums, setAlbums] = useState<AlbumCard[]>([]);
  const [query, setQuery] = useState("");
  /** 当前打开的专辑；null = 墙 */
  const [selected, setSelected] = useState<AlbumCard | null>(null);
  const [tracks, setTracks] = useState<TrackRow[]>([]);
  const [detailLoading, setDetailLoading] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pickerItems, setPickerItems] = useState<PlaylistAddItem[] | null>(null);
  const { toast, showToast } = useToast();
  const scrollRef = useRef<HTMLDivElement>(null);
  /** 加载序号，丢弃过期的 getAlbumTracks 响应 */
  const loadSeq = useRef(0);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await api.getAlbums();
      setAlbums(list);
    } catch (e) {
      // 失败保留旧数据，错误态与空态分开
      setError(friendlyErr(e));
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
    const seq = ++loadSeq.current;
    setSelected(a);
    setTracks([]);
    setDetailLoading(true);
    try {
      const list = await api.getAlbumTracks(a.album, a.album_artist);
      if (seq !== loadSeq.current) return;
      setTracks(list);
    } catch {
      if (seq !== loadSeq.current) return;
      setTracks([]);
    } finally {
      if (seq === loadSeq.current) setDetailLoading(false);
    }
  }

  function closeAlbum() {
    loadSeq.current += 1;
    setSelected(null);
    setTracks([]);
    setDetailLoading(false);
  }

  async function playAlbum(a: AlbumCard, e?: React.MouseEvent) {
    e?.stopPropagation();
    const list = selected && selected.album === a.album && selected.album_artist === a.album_artist
      ? tracks
      : await api.getAlbumTracks(a.album, a.album_artist).catch(() => [] as TrackRow[]);
    if (list.length === 0) return;
    await playQueue(list.map(trackRowToQueueItem), 0);
  }

  /** 点单曲：只播这一首，并替换当前播放队列 */
  function playTrack(t: TrackRow) {
    void playQueue([trackRowToQueueItem(t)], 0);
  }

  const albumInitial = (a: AlbumCard) =>
    (a.album || a.album_artist || "?").slice(0, 1).toUpperCase();

  return (
    <>
      <TopBar
        title={selected ? undefined : "专辑"}
        searchValue={selected ? undefined : query}
        onSearch={selected ? undefined : setQuery}
        onBack={selected ? closeAlbum : undefined}
        actions={
          selected ? (
            <>
              {toast && <span className="ok-toast">{toast}</span>}
              {tracks.length > 0 && (
                <>
                  <button
                    className="btn"
                    title="整专加入歌单"
                    onClick={() => setPickerItems(tracks.map(trackRowToAddItem))}
                  >
                    <ListPlus size={15} /> 加入歌单
                  </button>
                  <button
                    className="btn btn-primary"
                    title="播放专辑"
                    onClick={() => void playAlbum(selected)}
                  >
                    <Play size={15} /> 播放全部
                  </button>
                </>
              )}
            </>
          ) : undefined
        }
      />
      <div className="page-scroll" ref={scrollRef}>
        {selected ? (
          <div className="album-detail" key={`${selected.album_artist}::${selected.album}`}>
            <div className="album-detail-head">
              <AlbumCover
                path={selected.cover_track_path}
                mtime={selected.cover_track_mtime}
                hasCover={selected.has_cover}
                initial={albumInitial(selected)}
                size="detail"
              />
              <div className="album-detail-meta">
                <div className="display" style={{ fontSize: 24, marginBottom: 8 }}>
                  {selected.album || "Unknown Album"}
                </div>
                <div className="muted" style={{ fontWeight: 600 }}>
                  {selected.album_artist || "Unknown Artist"}
                </div>
                <div className="tertiary" style={{ marginTop: 4 }}>
                  {selected.year ? `${selected.year} · ` : ""}
                  {tracks.length || selected.track_count} 首
                </div>
              </div>
            </div>

            {detailLoading ? (
              <div className="empty-state">加载曲目…</div>
            ) : tracks.length === 0 ? (
              <div className="empty-state">
                <p className="muted">无曲目</p>
              </div>
            ) : (
              <div className="album-track-list">
                <div className="album-track-head tertiary">
                  <span />
                  <span />
                  <span>曲名</span>
                  <span>歌手</span>
                  <span />
                  <span>时长</span>
                </div>
                <VirtualList
                  items={tracks}
                  rowHeight={LIST_ROW_HEIGHT}
                  getItemKey={(t) => t.id}
                  getScrollElement={() => scrollRef.current}
                  renderRow={(t, idx) => (
                    <div
                      className="album-track-row"
                      title="播放"
                      onClick={() => playTrack(t)}
                    >
                      <span className="tertiary mono">{String(idx + 1).padStart(2, "0")}</span>
                      <span className="fav-col">
                        <FavoriteHeart item={trackRowToAddItem(t)} />
                      </span>
                      <span className="ellipsis">{t.title || t.filename}</span>
                      <span className="tertiary ellipsis">{t.artist || "—"}</span>
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
                  )}
                />
              </div>
            )}
          </div>
        ) : loading && albums.length === 0 ? (
          <div className="empty-state">加载中…</div>
        ) : error && albums.length === 0 ? (
          <div className="empty-state">
            <div className="display" style={{ fontSize: 20 }}>
              加载失败
            </div>
            <p className="muted">{error}</p>
            <button className="btn" onClick={() => void reload()}>
              重试
            </button>
          </div>
        ) : filtered.length === 0 ? (
          <div className="empty-state">
            <div className="display" style={{ fontSize: 20 }}>
              {albums.length === 0 ? "还没有专辑" : "没有匹配的专辑"}
            </div>
            <p className="muted">
              {albums.length === 0
                ? "到侧栏「管理」初始化库目录并刷新扫描，或先把音频放进库文件夹再扫描。"
                : `没有匹配「${query}」的专辑`}
            </p>
          </div>
        ) : (
          <div className="album-wall" key="wall">
            {filtered.map((a) => {
              return (
                <button
                  key={`${a.album_artist}::${a.album}`}
                  className="album-card"
                  title="打开专辑"
                  onClick={() => void openAlbum(a)}
                >
                  <div className="album-cover-wrap">
                    <AlbumCover
                      path={a.cover_track_path}
                      mtime={a.cover_track_mtime}
                      hasCover={a.has_cover}
                      initial={albumInitial(a)}
                    />
                    {/* 点封面 = 打开专辑，中央提示用唱片图标（非播放） */}
                    <span className="album-open" aria-hidden>
                      <Disc3 size={36} strokeWidth={1.5} />
                    </span>
                  </div>
                  <div className="album-name">{a.album || "Unknown Album"}</div>
                  <div className="album-sub tertiary">
                    {a.album_artist || "Unknown Artist"}
                    {a.year ? ` · ${a.year}` : ""} · {a.track_count}
                  </div>
                </button>
              );
            })}
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
