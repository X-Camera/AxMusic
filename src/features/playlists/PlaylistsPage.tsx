import { useCallback, useEffect, useRef, useState } from "react";
import { ChevronDown, ChevronUp, Heart, ListMusic, Play, Plus, Trash2 } from "lucide-react";

import { api, entryToQueueItem, formatTime } from "../../lib/api";
import type { PlaylistDetail, PlaylistSummary } from "../../lib/types";
import { confirmText, promptText } from "../../lib/dialog";
import { friendlyErr } from "../../lib/errors";
import { useApp } from "../../state/useApp";
import { useFavorites } from "../../state/useFavorites";
import { TopBar } from "../../components/TopBar";
import { FavoriteHeart } from "../../components/FavoriteHeart";
import { VirtualList, LIST_ROW_HEIGHT } from "../../components/VirtualList";
import "./Playlists.css";

export function PlaylistsPage() {
  const playQueue = useApp((s) => s.playQueue);
  const [list, setList] = useState<PlaylistSummary[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [detail, setDetail] = useState<PlaylistDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [noRoot, setNoRoot] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  /** 列表/详情请求代次：快速切换歌单时丢弃过期响应 */
  const listSeqRef = useRef(0);
  const detailSeqRef = useRef(0);

  const reload = useCallback(async () => {
    const seq = ++listSeqRef.current;
    setLoading(true);
    setError(null);
    try {
      const list = await api.playlistList();
      if (seq !== listSeqRef.current) return;
      setList(list);
      setNoRoot(false);
    } catch (e) {
      if (seq !== listSeqRef.current) return;
      const msg = friendlyErr(e);
      setList([]);
      setNoRoot(msg.includes("尚未初始化"));
      if (!msg.includes("尚未初始化")) setError(msg);
    } finally {
      if (seq === listSeqRef.current) setLoading(false);
    }
  }, []);

  const loadDetail = useCallback(async (name: string) => {
    const seq = ++detailSeqRef.current;
    try {
      const d = await api.playlistGet(name);
      if (seq !== detailSeqRef.current) return;
      setDetail(d);
      // 打开详情会做失效条目重匹配自愈；心形对照键跟着刷
      void useFavorites.getState().reload();
      setError(null);
    } catch (e) {
      if (seq !== detailSeqRef.current) return;
      setDetail(null);
      setError(friendlyErr(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  // 进入页面时自动选中「喜爱」（无则第一项），避免左侧看似选中、右侧空着
  useEffect(() => {
    if (selected || list.length === 0) return;
    const first = list.find((p) => p.is_favorites) ?? list[0];
    if (first) setSelected(first.name);
  }, [list, selected]);

  useEffect(() => {
    if (selected) void loadDetail(selected);
    else {
      detailSeqRef.current += 1;
      setDetail(null);
    }
  }, [selected, loadDetail]);

  // 迷你条/满窗/其它列表改喜爱时，刷新左侧计数；若正开着「喜爱」则同步曲目
  // （用 is_favorites 标志判定，不认字面量；list 故意不进依赖——只在 favRev/selected 变化时判定）
  const favRev = useFavorites((s) => s.rev);
  useEffect(() => {
    if (favRev === 0) return;
    void reload();
    const isFav = list.find((p) => p.name === selected)?.is_favorites ?? false;
    if (isFav && selected) void loadDetail(selected);
  }, [favRev, selected, reload, loadDetail]);

  async function onCreate() {
    const name = await promptText("新建歌单名称", "");
    if (!name) return;
    try {
      await api.playlistCreate(name.trim(), []);
      await reload();
      setSelected(name.trim());
    } catch (e) {
      setError(friendlyErr(e));
    }
  }

  async function onRename(name: string) {
    const next = await promptText("重命名歌单", name);
    if (!next || !next.trim() || next.trim() === name) return;
    try {
      await api.playlistRename(name, next.trim());
      await reload();
      if (selected === name) setSelected(next.trim());
    } catch (e) {
      setError(friendlyErr(e));
    }
  }

  async function onDelete(name: string) {
    if (!(await confirmText("删除歌单", `删除歌单「${name}」？`))) return;
    try {
      await api.playlistDelete(name);
      if (selected === name) setSelected(null);
      await reload();
    } catch (e) {
      setError(friendlyErr(e));
    }
  }

  /** 播放全部（跳过失效条目） */
  function playAll() {
    if (!detail) return;
    const alive = detail.entries.filter((e) => e.exists);
    if (alive.length === 0) return;
    void playQueue(alive.map(entryToQueueItem), 0);
  }

  /** 点单曲：只播这一首，并替换当前播放队列 */
  function playOne(e: PlaylistDetail["entries"][number]) {
    if (!e.exists) return;
    void playQueue([entryToQueueItem(e)], 0);
  }

  /** 上移/下移一格：按条目 rel_path 定位，连续点击不怕列表已错位 */
  async function onMove(entry: PlaylistDetail["entries"][number], delta: -1 | 1) {
    if (!detail) return;
    try {
      setDetail(await api.playlistMoveTrack(detail.name, entry.rel_path, delta));
    } catch (e) {
      setError(friendlyErr(e));
    }
  }

  async function onRemove(entry: PlaylistDetail["entries"][number]) {
    if (!detail) return;
    try {
      const next = await api.playlistRemoveTrack(detail.name, entry.rel_path);
      setDetail(next);
      // 左侧列表计数同步（喜爱走 favRev 联动，普通歌单自己刷）
      if (detail.is_favorites) void useFavorites.getState().reload();
      else void reload();
    } catch (e) {
      setError(friendlyErr(e));
    }
  }

  /** 批量清理失效条目：后端一次读写完事，不再逐条 IPC */
  async function onCleanMissing() {
    if (!detail) return;
    const missing = detail.entries.filter((e) => !e.exists).length;
    if (missing === 0) return;
    try {
      setDetail(await api.playlistCleanMissing(detail.name));
      await reload();
      if (detail.is_favorites) void useFavorites.getState().reload();
      setToast(`已清理 ${missing} 首失效条目`);
    } catch (e) {
      setError(friendlyErr(e));
    }
  }

  return (
    <>
      <TopBar
        title="歌单"
        actions={
          <button className="btn btn-primary" disabled={noRoot} onClick={() => void onCreate()}>
            <Plus size={15} /> 新建歌单
          </button>
        }
      />
      <div className="page-scroll" ref={scrollRef}>
        {noRoot ? (
          <div className="empty-state">
            <div className="display" style={{ fontSize: 20 }}>
              歌单
            </div>
            <p className="muted">
              先到侧栏「管理」初始化库目录，歌单将存放在 <span className="mono">&lt;库&gt;/playlists/</span>{" "}
              下的 .m3u8 文件里。
            </p>
          </div>
        ) : loading && list.length === 0 ? (
          <div className="empty-state">加载中…</div>
        ) : (
          <>
            {error && <div className="error-line">{error}</div>}
            {toast && (
              <div className="toast-line pl-toast" onClick={() => setToast(null)}>
                {toast}
              </div>
            )}
            <div className="pl-layout">
              <div className="panel pl-list">
                {list.length === 0 ? (
                  <div className="tertiary" style={{ padding: 12 }}>
                    还没有歌单
                  </div>
                ) : (
                  <>
                    {list
                      .filter((p) => p.is_favorites)
                      .map((p) => (
                        <div
                          key={p.name}
                          className={`pl-row favorites${selected === p.name ? " active" : ""}`}
                          onClick={() => setSelected(p.name)}
                        >
                          <Heart size={15} className="pl-fav-icon" fill="currentColor" />
                          <span className="pl-row-main">
                            <span className="ellipsis">{p.name}</span>
                            <span className="tertiary mono" style={{ fontSize: 11 }}>
                              {p.track_count} 首{p.total_ms > 0 ? ` · ${formatTime(p.total_ms)}` : ""}
                            </span>
                          </span>
                          <span className="pl-badge">系统</span>
                        </div>
                      ))}
                    {list.some((p) => !p.is_favorites) && (
                      <div className="pl-user-divider">自定义歌单</div>
                    )}
                    {list
                      .filter((p) => !p.is_favorites)
                      .map((p) => (
                        <div
                          key={p.name}
                          className={`pl-row${selected === p.name ? " active" : ""}`}
                          onClick={() => setSelected(p.name)}
                        >
                          <ListMusic size={15} className="tertiary" />
                          <span className="pl-row-main">
                            <span className="ellipsis">{p.name}</span>
                            <span className="tertiary mono" style={{ fontSize: 11 }}>
                              {p.track_count} 首{p.total_ms > 0 ? ` · ${formatTime(p.total_ms)}` : ""}
                            </span>
                          </span>
                          <span className="pl-row-actions">
                            <button
                              className="link-btn"
                              title="重命名"
                              onClick={(e) => {
                                e.stopPropagation();
                                void onRename(p.name);
                              }}
                            >
                              重命名
                            </button>
                            <button
                              className="link-btn"
                              title="删除歌单"
                              onClick={(e) => {
                                e.stopPropagation();
                                void onDelete(p.name);
                              }}
                            >
                              <Trash2 size={13} />
                            </button>
                          </span>
                        </div>
                      ))}
                  </>
                )}
              </div>

              <div>
                {!detail ? (
                  <div className="empty-state">
                    <p className="muted">左侧选一个歌单查看曲目；或从管理表/专辑「加入歌单」。</p>
                  </div>
                ) : (
                  <>
                    <div className={`pl-head${detail.is_favorites ? " favorites" : ""}`}>
                      <div>
                        <div className="display" style={{ fontSize: 18 }}>
                          {detail.is_favorites && <Heart size={18} fill="currentColor" />}
                          {detail.name}
                        </div>
                        <div className="tertiary">
                          {detail.entries.length} 首
                          {detail.entries.some((e) => !e.exists)
                            ? ` · ${detail.entries.filter((e) => !e.exists).length} 首缺失`
                            : ""}
                        </div>
                      </div>
                      <div style={{ display: "flex", gap: 8 }}>
                        <button
                          className="btn"
                          disabled={!detail.entries.some((e) => !e.exists)}
                          title="移除磁盘上已不存在的条目"
                          onClick={() => void onCleanMissing()}
                        >
                          清理失效
                        </button>
                        <button
                          className="btn btn-primary"
                          disabled={!detail.entries.some((e) => e.exists)}
                          onClick={() => playAll()}
                        >
                          <Play size={15} /> 播放全部
                        </button>
                      </div>
                    </div>
                    {detail.entries.length === 0 ? (
                      <div className="empty-state">
                        <p className="muted">
                          {detail.is_favorites
                            ? "还没有喜爱的歌曲，点任意列表旁的心形即可加入。"
                            : "歌单为空，去管理表或专辑「加入歌单」。"}
                        </p>
                      </div>
                    ) : (
                      <div className="pl-tracks">
                        <VirtualList
                          items={detail.entries}
                          rowHeight={LIST_ROW_HEIGHT}
                          getItemKey={(e) => e.path}
                          getScrollElement={() => scrollRef.current}
                          renderRow={(e, i) => {
                            const title = e.track?.title || e.title;
                            const artist = e.track?.artist || e.artist;
                            return (
                              <div className={`pl-track-row${e.exists ? "" : " missing"}`}>
                                <span className="tertiary mono">{String(i + 1).padStart(2, "0")}</span>
                                <span className="fav-col">
                                  <FavoriteHeart
                                    item={{
                                      path: e.path,
                                      title,
                                      artist,
                                      duration_ms: e.duration_ms,
                                    }}
                                    onToggle={(fav) => {
                                      if (!detail.is_favorites) return;
                                      if (!fav) {
                                        const path = e.path;
                                        setDetail({
                                          ...detail,
                                          entries: detail.entries.filter((x) => x.path !== path),
                                        });
                                      }
                                      void reload();
                                      void useFavorites.getState().reload();
                                    }}
                                  />
                                </span>
                                <span className="ellipsis" title={title}>
                                  {title}
                                  {!e.exists && <span className="chip chip-gap">缺失</span>}
                                </span>
                                <span className="tertiary ellipsis" title={artist}>
                                  {artist || "—"}
                                </span>
                                <span className="tertiary mono">{formatTime(e.duration_ms)}</span>
                                <span className="pl-track-actions">
                                  <button
                                    className="link-btn"
                                    title="播放"
                                    disabled={!e.exists}
                                    onClick={() => playOne(e)}
                                  >
                                    播放
                                  </button>
                                  <button
                                    className="link-btn"
                                    title="上移"
                                    disabled={i === 0}
                                    onClick={() => void onMove(e, -1)}
                                  >
                                    <ChevronUp size={13} />
                                  </button>
                                  <button
                                    className="link-btn"
                                    title="下移"
                                    disabled={i === detail.entries.length - 1}
                                    onClick={() => void onMove(e, 1)}
                                  >
                                    <ChevronDown size={13} />
                                  </button>
                                  <button
                                    className="link-btn"
                                    title="移除"
                                    onClick={() => void onRemove(e)}
                                  >
                                    移除
                                  </button>
                                </span>
                              </div>
                            );
                          }}
                        />
                      </div>
                    )}
                  </>
                )}
              </div>
            </div>
          </>
        )}
      </div>
    </>
  );
}
