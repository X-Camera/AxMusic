import { useCallback, useEffect, useState } from "react";
import { ChevronDown, ChevronUp, ListMusic, Play, Plus, Trash2 } from "lucide-react";

import { api, entryToQueueItem, formatTime } from "../../lib/api";
import type { PlaylistDetail, PlaylistSummary } from "../../lib/types";
import { useApp } from "../../state/useApp";
import { TopBar } from "../../components/TopBar";
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

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setList(await api.playlistList());
      setNoRoot(false);
    } catch (e) {
      const msg = String(e);
      setList([]);
      setNoRoot(msg.includes("尚未初始化"));
      if (!msg.includes("尚未初始化")) setError(msg);
    } finally {
      setLoading(false);
    }
  }, []);

  const loadDetail = useCallback(async (name: string) => {
    try {
      setDetail(await api.playlistGet(name));
      setError(null);
    } catch (e) {
      setDetail(null);
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    if (selected) void loadDetail(selected);
    else setDetail(null);
  }, [selected, loadDetail]);

  async function onCreate() {
    const name = window.prompt("新建歌单名称", "");
    if (!name || !name.trim()) return;
    try {
      await api.playlistCreate(name.trim(), []);
      await reload();
      setSelected(name.trim());
    } catch (e) {
      setError(String(e));
    }
  }

  async function onRename(name: string) {
    const next = window.prompt("重命名歌单", name);
    if (!next || !next.trim() || next.trim() === name) return;
    try {
      await api.playlistRename(name, next.trim());
      await reload();
      if (selected === name) setSelected(next.trim());
    } catch (e) {
      setError(String(e));
    }
  }

  async function onDelete(name: string) {
    if (!window.confirm(`删除歌单「${name}」？`)) return;
    try {
      await api.playlistDelete(name);
      if (selected === name) setSelected(null);
      await reload();
    } catch (e) {
      setError(String(e));
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

  async function onMove(index: number, delta: -1 | 1) {
    if (!detail) return;
    const to = index + delta;
    if (to < 0 || to >= detail.entries.length) return;
    try {
      setDetail(await api.playlistMoveTrack(detail.name, index, to));
    } catch (e) {
      setError(String(e));
    }
  }

  async function onRemove(index: number) {
    if (!detail) return;
    try {
      setDetail(await api.playlistRemoveTrack(detail.name, index));
    } catch (e) {
      setError(String(e));
    }
  }

  async function onCleanMissing() {
    if (!detail) return;
    try {
      let d = detail;
      for (let i = d.entries.length - 1; i >= 0; i--) {
        if (!d.entries[i].exists) d = await api.playlistRemoveTrack(d.name, i);
      }
      setDetail(d);
      await reload();
      setToast("已清理失效条目");
    } catch (e) {
      setError(String(e));
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
      <div className="page-scroll">
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
                  list.map((p) => (
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
                  ))
                )}
              </div>

              <div>
                {!detail ? (
                  <div className="empty-state">
                    <p className="muted">左侧选一个歌单查看曲目；或从管理表/专辑「加入歌单」。</p>
                  </div>
                ) : (
                  <>
                    <div className="pl-head">
                      <div>
                        <div className="display" style={{ fontSize: 18 }}>
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
                          disabled={!detail.entries.some((e) => e.exists)}
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
                        <p className="muted">歌单为空，去管理表或专辑「加入歌单」。</p>
                      </div>
                    ) : (
                      <div className="pl-tracks">
                        {detail.entries.map((e, i) => {
                          const title = e.track?.title || e.title;
                          const artist = e.track?.artist || e.artist;
                          return (
                            <div key={`${e.rel_path}-${i}`} className={`pl-track-row${e.exists ? "" : " missing"}`}>
                              <span className="tertiary mono">{String(i + 1).padStart(2, "0")}</span>
                              <span className="ellipsis" title={title}>
                                {title}
                                {!e.exists && <span className="chip" style={{ marginLeft: 8 }}>缺失</span>}
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
                                  onClick={() => void onMove(i, -1)}
                                >
                                  <ChevronUp size={13} />
                                </button>
                                <button
                                  className="link-btn"
                                  title="下移"
                                  disabled={i === detail.entries.length - 1}
                                  onClick={() => void onMove(i, 1)}
                                >
                                  <ChevronDown size={13} />
                                </button>
                                <button
                                  className="link-btn"
                                  title="移除"
                                  onClick={() => void onRemove(i)}
                                >
                                  移除
                                </button>
                              </span>
                            </div>
                          );
                        })}
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
