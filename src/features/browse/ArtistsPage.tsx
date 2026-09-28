import { useCallback, useEffect, useRef, useState } from "react";
import { ListPlus, Play, UserRound } from "lucide-react";

import { api, formatTime, trackRowToAddItem, trackRowToQueueItem } from "../../lib/api";
import type { AlbumCard, ArtistCard, PlaylistAddItem, TrackRow } from "../../lib/types";
import { useToast } from "../../lib/useToast";
import { useApp } from "../../state/useApp";
import { TopBar } from "../../components/TopBar";
import { FavoriteHeart } from "../../components/FavoriteHeart";
import { VirtualList, LIST_ROW_HEIGHT } from "../../components/VirtualList";
import { PlaylistPicker } from "../playlists/PlaylistPicker";
import { AlbumCover } from "./AlbumCover";
import "./Artists.css";

export function ArtistsPage() {
  const playQueue = useApp((s) => s.playQueue);
  const pendingArtist = useApp((s) => s.pendingArtist);
  const clearPendingArtist = useApp((s) => s.clearPendingArtist);
  const [artists, setArtists] = useState<ArtistCard[]>([]);
  const [query, setQuery] = useState("");
  /** 当前打开的歌手；null = 墙 */
  const [selected, setSelected] = useState<ArtistCard | null>(null);
  const [albums, setAlbums] = useState<AlbumCard[]>([]);
  const [tracks, setTracks] = useState<TrackRow[]>([]);
  const [detailLoading, setDetailLoading] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pickerItems, setPickerItems] = useState<PlaylistAddItem[] | null>(null);
  const { toast, showToast } = useToast();
  const scrollRef = useRef<HTMLDivElement>(null);
  /** 加载序号，丢弃过期的详情响应 */
  const loadSeq = useRef(0);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await api.getArtists();
      setArtists(list);
    } catch (e) {
      // 失败保留旧数据，错误态与空态分开
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  /** 歌手列表晚于详情到达时，用列表里的封面/曲目数回填 */
  useEffect(() => {
    if (!selected) return;
    const found = artists.find((a) => a.name === selected.name);
    if (!found) return;
    if (
      found.has_cover !== selected.has_cover ||
      found.cover_track_path !== selected.cover_track_path ||
      found.track_count !== selected.track_count ||
      found.album_count !== selected.album_count
    ) {
      setSelected((cur) => (cur && cur.name === found.name ? found : cur));
    }
  }, [artists, selected]);

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

  /** 统计页点头像 → 定位歌手详情（列表未到先用名字占位，封面等列表到达后回填） */
  useEffect(() => {
    if (!pendingArtist) return;
    const name = pendingArtist;
    clearPendingArtist();
    const found =
      artists.find((a) => a.name === name) ??
      ({
        name,
        track_count: 0,
        album_count: 0,
        has_cover: false,
        cover_track_path: null,
        cover_track_mtime: 0,
      } satisfies ArtistCard);
    void openArtist(found);
  }, [pendingArtist, artists, clearPendingArtist]);

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

  /** 点单曲：只播这一首，并替换当前播放队列 */
  function playTrack(t: TrackRow) {
    void playQueue([trackRowToQueueItem(t)], 0);
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
      <div className="page-scroll" ref={scrollRef}>
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
                            {/* 点封面 = 播放专辑 */}
                            <span className="cover-hover-icon" aria-hidden>
                              <Play size={36} fill="currentColor" strokeWidth={0} />
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
                        <span />
                        <span>曲名</span>
                        <span>专辑</span>
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
                        )}
                      />
                    </div>
                  )}
                </section>
              </>
            )}
          </div>
        ) : loading && artists.length === 0 ? (
          <div className="empty-state">加载中…</div>
        ) : error && artists.length === 0 ? (
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
                  {/* 点卡片 = 打开歌手 */}
                  <span className="cover-hover-icon" aria-hidden>
                    <UserRound size={36} strokeWidth={1.5} />
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
