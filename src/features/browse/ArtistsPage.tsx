import { useCallback, useEffect, useRef, useState } from "react";
import { ListPlus, Play } from "lucide-react";

import { api, formatTime, trackRowToAddItem, trackRowToQueueItem } from "../../lib/api";
import type { AlbumCard, ArtistCard, PlaylistAddItem, TrackRow } from "../../lib/types";
import { useApp } from "../../state/useApp";
import { TopBar } from "../../components/TopBar";
import { PlaylistPicker } from "../playlists/PlaylistPicker";
import { AlbumCover } from "./AlbumCover";
import "./Artists.css";

export function ArtistsPage() {
  const playQueue = useApp((s) => s.playQueue);
  const [artists, setArtists] = useState<ArtistCard[]>([]);
  const [query, setQuery] = useState("");
  /** 当前打开的歌手；null = 墙 */
  const [selected, setSelected] = useState<ArtistCard | null>(null);
  const [albums, setAlbums] = useState<AlbumCard[]>([]);
  const [tracks, setTracks] = useState<TrackRow[]>([]);
  const [detailLoading, setDetailLoading] = useState(false);
  const [loading, setLoading] = useState(false);
  const [pickerItems, setPickerItems] = useState<PlaylistAddItem[] | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  /** 加载序号，丢弃过期的详情响应 */
  const loadSeq = useRef(0);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const list = await api.getArtists();
      setArtists(list);
    } catch {
      setArtists([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const filtered = artists.filter((a) => {
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return a.name.toLowerCase().includes(q);
  });

  async function openArtist(a: ArtistCard) {
    const seq = ++loadSeq.current;
    setSelected(a);
    setAlbums([]);
    setTracks([]);
    setDetailLoading(true);
    try {
      const [albumList, trackList] = await Promise.all([
        api.getArtistAlbums(a.name),
        api.getArtistTracks(a.name),
      ]);
      if (seq !== loadSeq.current) return;
      setAlbums(albumList);
      setTracks(trackList);
    } catch {
      if (seq !== loadSeq.current) return;
      setAlbums([]);
      setTracks([]);
    } finally {
      if (seq === loadSeq.current) setDetailLoading(false);
    }
  }

  function closeArtist() {
    loadSeq.current += 1;
    setSelected(null);
    setAlbums([]);
    setTracks([]);
    setDetailLoading(false);
  }

  async function loadTracks(name: string): Promise<TrackRow[]> {
    if (selected?.name === name && tracks.length > 0) return tracks;
    return api.getArtistTracks(name).catch(() => [] as TrackRow[]);
  }

  async function playArtist(a: ArtistCard, e?: React.MouseEvent) {
    e?.stopPropagation();
    const list = await loadTracks(a.name);
    if (list.length === 0) return;
    await playQueue(list.map(trackRowToQueueItem), 0);
  }

  async function playAlbum(al: AlbumCard, e?: React.MouseEvent) {
    e?.stopPropagation();
    const list = await api
      .getAlbumTracks(al.album, al.album_artist)
      .catch(() => [] as TrackRow[]);
    if (list.length === 0) return;
    await playQueue(list.map(trackRowToQueueItem), 0);
  }

  function playTrack(idx: number) {
    if (tracks.length === 0) return;
    void playQueue(tracks.map(trackRowToQueueItem), idx);
  }

  function showToast(msg: string) {
    setToast(msg);
    window.setTimeout(() => setToast(null), 2000);
  }

  const artistInitial = (name: string) => (name || "?").slice(0, 1).toUpperCase();

  return (
    <>
      <TopBar
        title={selected ? undefined : "歌手"}
        searchValue={selected ? undefined : query}
        onSearch={selected ? undefined : setQuery}
        onBack={selected ? closeArtist : undefined}
        actions={
          selected ? (
            <>
              {toast && <span className="ok-toast">{toast}</span>}
              {tracks.length > 0 && (
                <>
                  <button
                    className="btn"
                    title="全部曲目加入歌单"
                    onClick={() => setPickerItems(tracks.map(trackRowToAddItem))}
                  >
                    <ListPlus size={15} /> 加入歌单
                  </button>
                  <button
                    className="btn btn-primary"
                    title="播放该歌手全部曲目"
                    onClick={() => void playArtist(selected)}
                  >
                    <Play size={15} /> 播放全部
                  </button>
                </>
              )}
            </>
          ) : undefined
        }
      />
      <div className="page-scroll">
        {selected ? (
          <div className="artist-detail" key={selected.name}>
            <div className="artist-hero">
              <AlbumCover
                path={selected.cover_track_path}
                mtime={selected.cover_track_mtime}
                hasCover={selected.has_cover}
                initial={artistInitial(selected.name)}
                size="detail"
              />
              <div className="artist-hero-meta">
                <div className="display" style={{ fontSize: 24, marginBottom: 8 }}>
                  {selected.name || "Unknown Artist"}
                </div>
                <div className="tertiary">
                  {selected.album_count} 张专辑 · {selected.track_count} 首
                </div>
              </div>
            </div>

            {detailLoading ? (
              <div className="empty-state">加载中…</div>
            ) : (
              <>
                {albums.length > 0 && (
                  <section>
                    <h2 className="artist-section-title">专辑</h2>
                    <div className="artist-albums">
                      {albums.map((al) => (
                        <button
                          key={`${al.album_artist}::${al.album}`}
                          className="album-card"
                          title="播放专辑"
                          onClick={(e) => void playAlbum(al, e)}
                        >
                          <div className="album-cover-wrap">
                            <AlbumCover
                              path={al.cover_track_path}
                              mtime={al.cover_track_mtime}
                              hasCover={al.has_cover}
                              initial={(al.album || "?").slice(0, 1).toUpperCase()}
                            />
                            <span className="album-play" title="播放专辑">
                              <Play size={16} fill="currentColor" />
                            </span>
                          </div>
                          <div className="album-name">{al.album || "Unknown Album"}</div>
                          <div className="album-sub tertiary">
                            {al.year ? `${al.year} · ` : ""}
                            {al.track_count} 首
                          </div>
                        </button>
                      ))}
                    </div>
                  </section>
                )}

                <section>
                  <h2 className="artist-section-title">曲目</h2>
                  {tracks.length === 0 ? (
                    <div className="empty-state">
                      <p className="muted">无曲目</p>
                    </div>
                  ) : (
                    <div className="album-track-list">
                      <div className="album-track-head tertiary">
                        <span />
                        <span>曲名</span>
                        <span>专辑</span>
                        <span />
                        <span>时长</span>
                      </div>
                      {tracks.map((t, idx) => (
                        <div
                          key={t.id}
                          className="album-track-row"
                          title="播放"
                          onClick={() => playTrack(idx)}
                        >
                          <span className="tertiary mono">{String(idx + 1).padStart(2, "0")}</span>
                          <span className="ellipsis">{t.title || t.filename}</span>
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
                  )}
                </section>
              </>
            )}
          </div>
        ) : loading && artists.length === 0 ? (
          <div className="empty-state">加载中…</div>
        ) : filtered.length === 0 ? (
          <div className="empty-state">
            <div className="display" style={{ fontSize: 20 }}>
              {artists.length === 0 ? "还没有歌手" : "没有匹配的歌手"}
            </div>
            <p className="muted">
              {artists.length === 0
                ? "到侧栏「管理」初始化库目录并刷新扫描，或先把音频放进库文件夹再扫描。"
                : `没有匹配「${query}」的歌手`}
            </p>
          </div>
        ) : (
          <div className="artist-wall" key="wall">
            {filtered.map((a) => (
              <button
                key={a.name}
                className="artist-card"
                title="打开歌手"
                onClick={() => void openArtist(a)}
              >
                <div className="artist-cover-wrap">
                  <AlbumCover
                    path={a.cover_track_path}
                    mtime={a.cover_track_mtime}
                    hasCover={a.has_cover}
                    initial={artistInitial(a.name)}
                  />
                  <span
                    className="album-play"
                    title="播放歌手"
                    onClick={(e) => void playArtist(a, e)}
                  >
                    <Play size={16} fill="currentColor" />
                  </span>
                </div>
                <div className="artist-name">{a.name || "Unknown Artist"}</div>
                <div className="artist-sub tertiary">
                  {a.album_count} 专 · {a.track_count} 首
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
