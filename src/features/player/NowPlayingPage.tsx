import { listen } from "@tauri-apps/api/event";
import {
  Pause,
  Play,
  Repeat,
  Repeat1,
  Search,
  Shuffle,
  SkipBack,
  SkipForward,
  SlidersHorizontal,
  Volume1,
  Volume2,
  VolumeX,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { api, formatTime } from "../../lib/api";
import {
  FULL_LYRICS_DISP_DEFAULT,
  fullLyricsDisp,
  lyricsDisplayVars,
  type LyricsDisp,
} from "../../lib/lyricsDisplay";
import { exitTrueFullscreen, toggleTrueFullscreen } from "../../lib/trueFullscreen";
import { nextRepeat, REPEAT_TITLE } from "../../lib/playMode";
import type { AppSettings } from "../../lib/types";
import { DEFAULT_VOLUME, LOW_VOLUME_THRESHOLD } from "../../lib/volume";
import { onLyricsSaved, openLyricsWindow } from "../../lib/lyricsWindow";
import {
  clockNow,
  clockReanchor,
  clockSyncFromSnapshot,
  createPlayClock,
  type PlayClock,
} from "../../lib/playClock";
import { WindowControls } from "../../components/WindowControls";
import { ContextMenu } from "../../components/ContextMenu";
import { FavoriteHeart } from "../../components/FavoriteHeart";
import { useApp } from "../../state/useApp";
import { LyricsView, type LyricsViewHandle } from "./LyricsView";
import { LyricsStyleDialog } from "./LyricsStyleDialog";
import { pickLyrics } from "./lrc";
import "./NowPlayingPage.css";

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
  const setShuffle = useApp((s) => s.setShuffle);
  const setRepeat = useApp((s) => s.setRepeat);
  const setFullPlayer = useApp((s) => s.setFullPlayer);
  const shuffle = player?.shuffle ?? false;
  const repeat = player?.repeat ?? "off";

  const track = player?.track ?? null;
  const path = track?.path ?? "";
  const pos = player?.position_ms ?? 0;
  const dur = Math.max(player?.duration_ms ?? track?.duration_ms ?? 0, 1);
  const playing = player?.status === "Playing";

  const [mediaState, setMediaState] = useState<{
    path: string;
    info: MediaInfo | null;
  }>({ path: "", info: null });
  /** 渲染期派生：path 一变立刻清空，避免旧封面/旧歌词残影 */
  const info = mediaState.path === path ? mediaState.info : null;
  const [seeking, setSeeking] = useState(false);
  const [seekMs, setSeekMs] = useState(0);
  const [closing, setClosing] = useState(false);
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number } | null>(null);
  const [styleOpen, setStyleOpen] = useState(false);
  /** 曲目代数：切歌 / 单曲循环重播时 +1，强制歌词 DOM 与引擎整表重建 */
  const [trackGen, setTrackGen] = useState(0);
  const lastPosForGenRef = useRef(0);
  /** 切歌后吞掉第一次 jumpedBack（gapless 归零），避免连弹两遍 */
  const skipJumpGenRef = useRef(false);
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
    setMediaState({ path, info: null });
    if (!path) return;
    void api.trackMediaInfo(path).then((m) => {
      if (!cancelled) setMediaState({ path, info: m });
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

  /** 播放界面全局右键：搜索歌词 / 歌词样式 */
  function onLyricsContextMenu(e: React.MouseEvent) {
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
  /** 满窗歌词显示参数（设置页「满窗歌词」） */
  const [lyricsDisp, setLyricsDisp] = useState<LyricsDisp>(FULL_LYRICS_DISP_DEFAULT);
  useEffect(() => {
    let cancelled = false;
    void api.getSettings().then((s) => {
      if (cancelled) return;
      setLyricsPrefer(s.lyrics_prefer);
      setLyricsDisp(fullLyricsDisp(s));
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // 设置页「满窗歌词」改动实时同步（自身样式弹窗 patch 的回声等值重放，幂等）
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen<AppSettings>("settings://changed", (e) => {
      if (cancelled) return;
      const s = e.payload;
      setLyricsPrefer(s.lyrics_prefer);
      setLyricsDisp(fullLyricsDisp(s));
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
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
    resetLyricsScroll();
    try {
      await seek(target);
    } finally {
      reanchorClock(target);
      setSeeking(false);
    }
  }

  const lyricsViewRef = useRef<LyricsViewHandle>(null);

  function requestClose() {
    if (closing) return;
    setClosing(true);
    // 真全屏是播放页专属状态，缩回主界面时一并退出（主界面没有退出入口）
    void exitTrueFullscreen();
    window.setTimeout(() => setFullPlayer(false), 280);
  }

  /** 空白处双击 ↔ 真全屏（覆盖任务栏）；交互元素（歌词/按钮/进度条/弹层等）不触发 */
  function onStageDoubleClick(e: React.MouseEvent) {
    const t = e.target as HTMLElement;
    if (t.closest("button, input, .np-lyrics, .np-seek, .np-window-controls")) return;
    void toggleTrueFullscreen();
  }

  // Esc 退出真全屏（惯例，与浏览器/播放器一致）
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      void exitTrueFullscreen();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  /** 点击句子：跳转（闪光由 LyricsView 负责） */
  function seekToLine(_i: number, ms: number) {
    void commitSeek(ms);
  }

  function resetLyricsScroll() {
    lyricsViewRef.current?.resetScroll();
  }

  const lyricsStyle = lyricsDisplayVars(lyricsDisp);

  /** 弹窗调参：先套到歌词（实时预览），再落盘 */
  function patchLyricsDisp(next: Partial<typeof lyricsDisp>) {
    const merged = { ...lyricsDisp, ...next };
    setLyricsDisp(merged);
    void api
      .updateSettings({
        lyrics_font_scale: merged.fontScale,
        lyrics_font: merged.font,
        lyrics_line_height: merged.lineHeight,
      })
      .catch(() => void 0);
  }

  // 切歌升代（强制歌词整表重建）
  useEffect(() => {
    setTrackGen((g) => g + 1);
    lastPosForGenRef.current = 0;
    // 仅 gapless 尾（pos 仍停在上一首末）才吞第一次 jumpedBack；
    // 立刻从头播（pos≈0）时别吞，否则单曲重播会被误伤
    skipJumpGenRef.current = pos > 1500;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path]);

  // 单曲循环重播 / 点「下一首」回到自己：path 不变但进度归零，也要升代
  useEffect(() => {
    const prev = lastPosForGenRef.current;
    lastPosForGenRef.current = pos;
    if (seeking) return;
    const jumpedBack = prev > 2000 && pos < 800;
    if (jumpedBack) {
      if (skipJumpGenRef.current) {
        skipJumpGenRef.current = false;
      } else {
        setTrackGen((g) => g + 1);
      }
    }
  }, [pos, seeking]);

  // 歌词子窗口保存后刷新当前曲目歌词
  useEffect(() => {
    if (!path) return;
    return onLyricsSaved((t) => {
      if (t.path && t.path !== path) return;
      void api.trackMediaInfo(path).then((m) => {
        setMediaState({ path, info: m });
        // 空歌词 → 有歌词：升代强制整表重建
        setTrackGen((g) => g + 1);
      });
    });
  }, [path]);

  const title = info?.title || track?.title || info?.filename || "未在播放";
  const artist = info?.artist || info?.album_artist || "";
  const cover = info?.cover_data ?? null;
  const vol = player?.volume ?? DEFAULT_VOLUME;
  /** 静音前音量 + 静音意图：与迷你条同一套语义 */
  const lastVolRef = useRef(DEFAULT_VOLUME);
  const mutedRef = useRef(false);

  function toggleMute() {
    if (!mutedRef.current && vol > 0) {
      lastVolRef.current = vol;
      mutedRef.current = true;
      void setVolume(0);
    } else {
      mutedRef.current = false;
      void setVolume(lastVolRef.current);
    }
  }

  function renderVolumeIcon() {
    if (vol <= 0) return <VolumeX size={15} />;
    if (vol < LOW_VOLUME_THRESHOLD) return <Volume1 size={15} />;
    return <Volume2 size={15} />;
  }

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
        style={
          cover && cover.startsWith("data:image/")
            ? { backgroundImage: `url("${cover.replace(/"/g, "")}")` }
            : undefined
        }
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
              className={`np-icon mode${shuffle ? " active" : ""}`}
              title={shuffle ? "随机播放（开）" : "随机播放"}
              onClick={() => void setShuffle(!shuffle)}
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
              className={`np-icon mode${repeat !== "off" ? " active" : ""}`}
              title={REPEAT_TITLE[repeat]}
              onClick={() => void setRepeat(nextRepeat(repeat))}
            >
              {repeat === "one" ? <Repeat1 size={18} /> : <Repeat size={18} />}
            </button>
          </div>

          <div className="np-vol">
            <button
              className="np-icon np-vol-btn"
              title={vol > 0 ? "静音" : "恢复音量"}
              aria-label={vol > 0 ? "静音" : "恢复音量"}
              onClick={toggleMute}
            >
              {renderVolumeIcon()}
            </button>
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={vol}
              aria-label="音量"
              onChange={(e) => {
                const v = Number(e.target.value);
                if (v > 0) {
                  lastVolRef.current = v;
                  mutedRef.current = false;
                }
                void setVolume(v);
              }}
              style={{ ["--pct" as string]: `${vol * 100}%` }}
            />
          </div>
        </section>

        <section className="np-right">
          <LyricsView
            key={path || "idle"}
            ref={lyricsViewRef}
            lines={lines}
            plain={plain}
            synced={synced}
            getTimeMs={currentTimeMs}
            onSeekLine={seekToLine}
            style={lyricsStyle}
            layoutKey={`${lyricsDisp.fontScale}|${lyricsDisp.font}|${lyricsDisp.lineHeight}`}
            rebuildKey={`${path}|${trackGen}`}
            empty={
              !track ? (
                <div className="np-lyrics-empty tertiary">从专辑或管理表挑一首开始</div>
              ) : !synced && plain.length === 0 ? (
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
              ) : null
            }
          />
        </section>
      </div>

      {ctxMenu && (
        <ContextMenu x={ctxMenu.x} y={ctxMenu.y} onCover>
          {track && (
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
          )}
          <button
            className="np-ctx-item"
            role="menuitem"
            onClick={() => {
              setCtxMenu(null);
              setStyleOpen(true);
            }}
          >
            <SlidersHorizontal size={14} />
            歌词样式
          </button>
        </ContextMenu>
      )}

      {styleOpen &&
        createPortal(
          <LyricsStyleDialog
            value={lyricsDisp}
            onChange={patchLyricsDisp}
            onClose={() => setStyleOpen(false)}
          />,
          document.body,
        )}
    </div>
  );
}
