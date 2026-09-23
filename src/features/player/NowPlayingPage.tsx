import {
  Pause,
  Play,
  Repeat,
  Repeat1,
  Search,
  Shuffle,
  SkipBack,
  SkipForward,
  Volume1,
  Volume2,
  VolumeX,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { api, formatTime } from "../../lib/api";
import { onLyricsSaved, openLyricsWindow } from "../../lib/lyricsWindow";
import {
  clockNow,
  clockReanchor,
  clockSyncFromSnapshot,
  createPlayClock,
  type PlayClock,
} from "../../lib/playClock";
import { WindowControls } from "../../components/WindowControls";
import { FavoriteHeart } from "../../components/FavoriteHeart";
import { useApp } from "../../state/useApp";
import { useAmllLyrics } from "./amll/useAmllLyrics";
import { pickLyrics, type LrcLine } from "./lrc";
import "./NowPlayingPage.css";

const appWindow = getCurrentWindow();

interface MediaInfo {
  path: string;
  filename: string;
  title: string;
  artist: string;
  album: string;
  album_artist: string;
  year: string;
  has_lyrics: boolean;
  cover_data: string | null;
  embedded: string | null;
  sidecar: string | null;
}

/** 顶部白色横条 = 缩回主界面（替代左上角按钮） */
export function NowPlayingPage() {
  const player = useApp((s) => s.player);
  const toggle = useApp((s) => s.toggle);
  const next = useApp((s) => s.next);
  const prev = useApp((s) => s.prev);
  const seek = useApp((s) => s.seek);
  const setVolume = useApp((s) => s.setVolume);
  const setPlayMode = useApp((s) => s.setPlayMode);
  const setFullPlayer = useApp((s) => s.setFullPlayer);
  const playMode = player?.play_mode ?? "sequential";

  const track = player?.track ?? null;
  const path = track?.path ?? "";
  const pos = player?.position_ms ?? 0;
  const dur = Math.max(player?.duration_ms ?? track?.duration_ms ?? 0, 1);
  const playing = player?.status === "Playing";

  const [info, setInfo] = useState<MediaInfo | null>(null);
  const [seeking, setSeeking] = useState(false);
  const [seekMs, setSeekMs] = useState(0);
  const [closing, setClosing] = useState(false);
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number } | null>(null);
  const [flashIdx, setFlashIdx] = useState(-1);
  const flashTimerRef = useRef(0);
  /** 播放中用基准时间外推，填补 500ms 轮询间隙 */
  const clockRef = useRef<PlayClock>(createPlayClock());
  const seekFillRef = useRef<HTMLDivElement>(null);
  const seekCurRef = useRef<HTMLSpanElement>(null);
  const seekRemainRef = useRef<HTMLSpanElement>(null);
  const lastPosRef = useRef(0);
  const liveRef = useRef({
    seeking: false,
    seekMs: 0,
    pos: 0,
    playing: false,
  });

  useEffect(() => {
    let cancelled = false;
    setInfo(null);
    if (!path) return;
    void api.trackMediaInfo(path).then((m) => {
      if (!cancelled) setInfo(m);
    });
    return () => {
      cancelled = true;
    };
  }, [path]);

  /** 打开独立「搜索歌词」子窗口（可拖出主应用） */
  async function openLyricsSearch() {
    if (!path) return;
    const filename = info?.filename || path.split(/[\\/]/).pop() || path;
    const title = info?.title || track?.title || filename;
    const artist = info?.artist || info?.album_artist || "";
    let id = 0;
    try {
      const row = await api.getTrackByPath(path);
      if (row) id = row.id;
    } catch {
      /* 无库 / 查询失败：按库外文件处理 */
    }
    await openLyricsWindow({ id, path, title, artist, filename });
  }

  /** 播放界面全局右键：搜索歌词（有则替换，无则直接搜） */
  function onLyricsContextMenu(e: React.MouseEvent) {
    if (!track) return;
    e.preventDefault();
    setCtxMenu({ x: e.clientX, y: e.clientY });
  }

  useEffect(() => {
    if (!ctxMenu) return;
    const close = () => setCtxMenu(null);
    window.addEventListener("click", close);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("resize", close);
    };
  }, [ctxMenu]);

  const [lyricsPrefer, setLyricsPrefer] = useState<"sidecar" | "embed">("sidecar");
  useEffect(() => {
    let cancelled = false;
    void api.getSettings().then((s) => {
      if (!cancelled) setLyricsPrefer(s.lyrics_prefer);
    });
    return () => {
      cancelled = true;
    };
  }, []);
  const { lines, plain, synced } = useMemo(
    () => pickLyrics(info?.embedded ?? null, info?.sidecar ?? null, lyricsPrefer),
    [info, lyricsPrefer],
  );

  function reanchorClock(ms: number) {
    clockReanchor(clockRef.current, ms, clockRef.current.playing);
    lastPosRef.current = ms;
  }

  useEffect(() => {
    liveRef.current = { seeking, seekMs, pos, playing };
    // 拖动/seek 中：歌词跟预览进度；否则跟播放时钟（轮询对齐 + 间隙外推）
    if (seeking) {
      clockReanchor(clockRef.current, seekMs, false);
      lastPosRef.current = seekMs;
      return;
    }
    // 播放中小漂移不重锚，避免进度每 500ms 微跳
    clockSyncFromSnapshot(clockRef.current, pos, playing);
    lastPosRef.current = pos;
  }, [seeking, seekMs, pos, playing]);

  // 进度填充与时间标签：rAF 外推平滑（与歌词同一时钟）
  useEffect(() => {
    let raf = 0;
    const paint = () => {
      raf = requestAnimationFrame(paint);
      const d = Math.max(dur, 1);
      const ms = Math.min(Math.max(currentTimeMs(), 0), d);
      const p = (ms / d) * 100;
      if (seekFillRef.current) seekFillRef.current.style.width = `${p}%`;
      if (seekCurRef.current) seekCurRef.current.textContent = formatTime(Math.round(ms));
      if (seekRemainRef.current) {
        seekRemainRef.current.textContent = `-${formatTime(Math.max(d - Math.round(ms), 0))}`;
      }
    };
    raf = requestAnimationFrame(paint);
    return () => cancelAnimationFrame(raf);
  }, [dur]);

  /** 兜底：seeking 不得长期卡住，否则歌词时间会冻在预览值上不再滚动 */
  useEffect(() => {
    if (!seeking) return;
    const t = window.setTimeout(() => setSeeking(false), 800);
    return () => window.clearTimeout(t);
  }, [seeking, seekMs]);

  function currentTimeMs(): number {
    const live = liveRef.current;
    // 仅 scrub 预览时用 seekMs；松手后立刻回到播放时钟
    if (live.seeking) return live.seekMs;
    return clockNow(clockRef.current);
  }

  /** 提交进度：seek 结束后退出 scrub 并对齐时钟与歌词滚动，避免冻住 */
  async function commitSeek(ms: number) {
    const target = Math.round(ms);
    setSeekMs(target);
    setSeeking(true);
    reanchorClock(target);
    resetScroll();
    try {
      await seek(target);
    } finally {
      reanchorClock(target);
      setSeeking(false);
    }
  }

  function flashLine(i: number) {
    setFlashIdx(i);
    window.clearTimeout(flashTimerRef.current);
    flashTimerRef.current = window.setTimeout(() => setFlashIdx(-1), 400);
  }

  /** 点击句子：闪一下 + 跳转（AM 操作逻辑，由歌词引擎在「未拖动」时回调） */
  function seekToLine(i: number, ms: number) {
    flashLine(i);
    void commitSeek(ms);
  }

  const {
    containerRef: lyricsRef,
    activeIdx,
    resetScroll,
  } = useAmllLyrics({
    lines,
    synced,
    playing,
    getTimeMs: currentTimeMs,
    onSeekLine: (i, ms) => seekToLine(i, ms),
  });

  // 切歌清空行状态
  useEffect(() => {
    setFlashIdx(-1);
  }, [path]);

  useEffect(
    () => () => {
      window.clearTimeout(flashTimerRef.current);
    },
    [],
  );

  function requestClose() {
    if (closing) return;
    setClosing(true);
    // 真全屏是播放页专属状态，缩回主界面时一并退出（主界面没有退出入口）
    void appWindow.isFullscreen().then((f) => {
      if (f) void appWindow.setFullscreen(false);
    });
    window.setTimeout(() => setFullPlayer(false), 280);
  }

  /** 空白处双击 ↔ 真全屏（覆盖任务栏）；交互元素（歌词/按钮/进度条/弹层等）不触发 */
  function onStageDoubleClick(e: React.MouseEvent) {
    const t = e.target as HTMLElement;
    if (t.closest("button, input, .np-lyrics, .np-seek, .np-window-controls")) return;
    void appWindow.isFullscreen().then((f) => void appWindow.setFullscreen(!f));
  }

  // Esc 退出真全屏（惯例，与浏览器/播放器一致）
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      void appWindow.isFullscreen().then((f) => {
        if (f) void appWindow.setFullscreen(false);
      });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // 歌词子窗口保存后刷新当前曲目歌词
  useEffect(() => {
    if (!path) return;
    return onLyricsSaved(() => {
      void api.trackMediaInfo(path).then((m) => setInfo(m));
    });
  }, [path]);

  const title = info?.title || track?.title || info?.filename || "未在播放";
  const artist = info?.artist || info?.album_artist || "";
  const cover = info?.cover_data ?? null;
  const vol = player?.volume ?? 0.8;

  function seekFromEvent(e: React.MouseEvent<HTMLElement>) {
    const rect = e.currentTarget.getBoundingClientRect();
    const ratio = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
    return Math.round(ratio * dur);
  }

  return (
    <div
      className={`np-page${playing ? " playing" : ""}${closing ? " closing" : ""}`}
      data-context-menu
      onDoubleClick={onStageDoubleClick}
      onContextMenu={onLyricsContextMenu}
    >
      <div
        className="np-bg"
        style={cover ? { backgroundImage: `url(${cover})` } : undefined}
        aria-hidden
      />
      <div className="np-bg-veil" aria-hidden />

      <header className="np-top" data-tauri-drag-region="deep">
        <button
          className="np-grabber"
          title="缩回主界面"
          aria-label="缩回主界面"
          onClick={requestClose}
        />
        <div className="np-window-controls">
          <WindowControls />
        </div>
      </header>

      <div className="np-stage">
        <section className="np-left">
          <div className="np-cover-wrap">
            {cover ? (
              <img className="np-cover" src={cover} alt="" />
            ) : (
              <div className="np-cover np-cover-empty">♪</div>
            )}
          </div>

          <div className="np-meta">
            <div>
              <div className="np-title-row">
                <h2 className="np-title">{title}</h2>
                {track && (
                  <FavoriteHeart
                    item={{
                      path: track.path,
                      title,
                      artist: artist || "",
                      duration_ms: track.duration_ms,
                    }}
                    size={20}
                    className="inline"
                  />
                )}
              </div>
              <p className="np-sub">{artist || "—"}</p>
            </div>
          </div>

          <div
            className="np-seek"
            onMouseDown={(e) => {
              setSeeking(true);
              setSeekMs(seekFromEvent(e));
            }}
            onMouseMove={(e) => {
              if (!seeking) return;
              setSeekMs(seekFromEvent(e));
            }}
            onMouseUp={(e) => {
              if (seeking) void commitSeek(seekFromEvent(e));
            }}
            onMouseLeave={() => {
              // 拖出进度条：按当前预览提交，避免 seeking 卡住
              if (seeking) void commitSeek(seekMs);
            }}
          >
            <div className="np-seek-track">
              <div ref={seekFillRef} className="np-seek-fill" />
            </div>
            <div className="np-seek-times">
              <span ref={seekCurRef} />
              <span ref={seekRemainRef} />
            </div>
          </div>

          <div className="np-btns">
            <button
              className={`np-icon mode${playMode === "shuffle" ? " active" : ""}`}
              title={playMode === "shuffle" ? "随机播放（开）" : "随机播放"}
              onClick={() =>
                void setPlayMode(playMode === "shuffle" ? "sequential" : "shuffle")
              }
            >
              <Shuffle size={18} />
            </button>
            <button className="np-icon" disabled={!track} onClick={() => void prev()} title="上一首">
              <SkipBack size={26} fill="currentColor" />
            </button>
            <button
              className="np-icon np-play"
              disabled={!track}
              onClick={() => void toggle()}
              title={playing ? "暂停" : "播放"}
            >
              {playing ? (
                <Pause size={34} fill="currentColor" />
              ) : (
                <Play size={34} fill="currentColor" />
              )}
            </button>
            <button className="np-icon" disabled={!track} onClick={() => void next()} title="下一首">
              <SkipForward size={26} fill="currentColor" />
            </button>
            <button
              className={`np-icon mode${playMode === "repeat_one" ? " active" : ""}`}
              title={playMode === "repeat_one" ? "单曲循环（开）" : "单曲循环"}
              onClick={() =>
                void setPlayMode(
                  playMode === "repeat_one" ? "sequential" : "repeat_one",
                )
              }
            >
              {playMode === "repeat_one" ? (
                <Repeat1 size={18} />
              ) : (
                <Repeat size={18} />
              )}
            </button>
          </div>

          <div className="np-vol">
            <button
              className="np-icon np-vol-btn"
              title="静音/恢复"
              onClick={() => void setVolume(vol > 0 ? 0 : 0.8)}
            >
              {vol <= 0 ? (
                <VolumeX size={15} />
              ) : vol < 0.45 ? (
                <Volume1 size={15} />
              ) : (
                <Volume2 size={15} />
              )}
            </button>
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={vol}
              aria-label="音量"
              onChange={(e) => void setVolume(Number(e.target.value))}
              style={{ ["--pct" as string]: `${vol * 100}%` }}
            />
          </div>
        </section>

        <section className="np-right">
          <div
            className={`np-lyrics${synced ? " amll" : ""}`}
            ref={lyricsRef}
          >
            {!track && (
              <div className="np-lyrics-empty tertiary">从专辑或管理表挑一首开始</div>
            )}
            {track && !synced && plain.length === 0 && (
              <div className="np-lyrics-empty">
                <button
                  className="np-lyrics-search-btn"
                  title="搜索歌词"
                  aria-label="搜索歌词"
                  onClick={() => void openLyricsSearch()}
                >
                  <Search size={18} strokeWidth={2} />
                  <span>歌词</span>
                </button>
              </div>
            )}
            {track && !synced && plain.length > 0 && (
              <div className="np-lines">
                <div className="np-line-spacer" aria-hidden />
                {plain.map((t, i) => (
                  <div key={i} className="np-line plain">
                    <div className="np-main">{t}</div>
                  </div>
                ))}
                <div className="np-line-spacer" aria-hidden />
              </div>
            )}
            {synced &&
              lines.map((l: LrcLine, i) => {
                const on = i === activeIdx;
                const state = on ? "on" : i < activeIdx ? "past" : "next";
                return (
                  <div
                    key={`${l.timeMs}-${i}`}
                    data-i={i}
                    className={["np-line", state, flashIdx === i ? "flash" : ""]
                      .filter(Boolean)
                      .join(" ")}
                  >
                    <div className="np-line-inner">
                      <div className="np-main">{l.text || "⋯"}</div>
                      {l.trans ? <div className="np-trans">{l.trans}</div> : null}
                    </div>
                  </div>
                );
              })}
          </div>
        </section>
      </div>

      {ctxMenu &&
        track &&
        createPortal(
          <div
            className="np-ctx-menu"
            style={{ left: ctxMenu.x, top: ctxMenu.y }}
            role="menu"
            onClick={(e) => e.stopPropagation()}
          >
            <button
              className="np-ctx-item"
              role="menuitem"
              onClick={() => {
                setCtxMenu(null);
                void openLyricsSearch();
              }}
            >
              <Search size={14} />
              搜索歌词
            </button>
          </div>,
          document.body,
        )}
    </div>
  );
}
