import {
  Pause,
  Play,
  Repeat,
  Shuffle,
  SkipBack,
  SkipForward,
  Volume1,
  Volume2,
  VolumeX,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { api, formatTime } from "../../lib/api";
import { WindowControls } from "../../components/WindowControls";
import { useApp } from "../../state/useApp";
import { findLrcIndex, lineProgress, pickLyrics, type LrcLine } from "./lrc";
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

function prefersReducedMotion() {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** 缓出带回一点弹性，用于歌词跟随 / 松手回弹（自研，非第三方） */
function easeOutBack(t: number): number {
  const c1 = 1.15;
  const c3 = c1 + 1;
  return 1 + c3 * Math.pow(t - 1, 3) + c1 * Math.pow(t - 1, 2);
}

function scrollTopToCenter(box: HTMLElement, el: HTMLElement): number {
  const boxRect = box.getBoundingClientRect();
  const elRect = el.getBoundingClientRect();
  const delta = elRect.top + elRect.height / 2 - (boxRect.top + boxRect.height / 2);
  return Math.max(0, box.scrollTop + delta);
}

/** 顶部白色横条 = 缩回主界面（替代左上角按钮） */
export function NowPlayingPage() {
  const player = useApp((s) => s.player);
  const toggle = useApp((s) => s.toggle);
  const next = useApp((s) => s.next);
  const prev = useApp((s) => s.prev);
  const seek = useApp((s) => s.seek);
  const setVolume = useApp((s) => s.setVolume);
  const setFullPlayer = useApp((s) => s.setFullPlayer);

  const track = player?.track ?? null;
  const path = track?.path ?? "";
  const pos = player?.position_ms ?? 0;
  const dur = Math.max(player?.duration_ms ?? track?.duration_ms ?? 0, 1);
  const playing = player?.status === "Playing";

  const [info, setInfo] = useState<MediaInfo | null>(null);
  const [seeking, setSeeking] = useState(false);
  const [seekMs, setSeekMs] = useState(0);
  const [closing, setClosing] = useState(false);
  const [draggingLrc, setDraggingLrc] = useState(false);
  /** rAF 推得的当前句：只在换行时 setState，避免每帧重渲染 */
  const [activeIdx, setActiveIdx] = useState(-1);
  const [flashIdx, setFlashIdx] = useState(-1);
  const listRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{
    y: number;
    startMs: number;
    startScroll: number;
    moved: boolean;
    lastMs: number;
  } | null>(null);
  const skipClickRef = useRef(false);
  const springRafRef = useRef(0);
  const flashTimerRef = useRef(0);
  const activeIdxRef = useRef(-1);
  /** 播放中用基准时间外推，填补 500ms 轮询间隙（行内进度 / 跟手） */
  const clockRef = useRef({ baseMs: 0, baseAt: 0 });
  const lastPosRef = useRef(0);
  const liveRef = useRef({
    lines: [] as LrcLine[],
    seeking: false,
    seekMs: 0,
    pos: 0,
    playing: false,
    synced: false,
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

  const { lines, plain, synced } = useMemo(
    () => pickLyrics(info?.embedded ?? null, info?.sidecar ?? null),
    [info],
  );

  function reanchorClock(ms: number) {
    const c = clockRef.current;
    c.baseMs = ms;
    c.baseAt = performance.now();
    lastPosRef.current = ms;
  }

  useEffect(() => {
    liveRef.current = { lines, seeking, seekMs, pos, playing, synced };
    // 拖动/seek 中：歌词跟预览进度；否则跟播放时钟（轮询对齐 + 间隙外推）
    if (seeking) {
      reanchorClock(seekMs);
      return;
    }
    if (!playing) {
      reanchorClock(pos);
      return;
    }
    if (pos !== lastPosRef.current) {
      reanchorClock(pos);
    }
  }, [lines, seeking, seekMs, pos, playing, synced]);

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
    const c = clockRef.current;
    if (live.playing) return c.baseMs + (performance.now() - c.baseAt);
    return c.baseMs;
  }

  function cancelSpring() {
    if (springRafRef.current) {
      cancelAnimationFrame(springRafRef.current);
      springRafRef.current = 0;
    }
  }

  /** 弹簧滚到目标 offset（松手回弹 / 换行跟随） */
  function springScrollTo(target: number) {
    const box = listRef.current;
    if (!box) return;
    cancelSpring();
    const start = box.scrollTop;
    const dist = target - start;
    if (prefersReducedMotion() || Math.abs(dist) < 0.5) {
      box.scrollTop = target;
      return;
    }
    const duration = 480;
    const t0 = performance.now();
    const step = (now: number) => {
      const t = Math.min(1, (now - t0) / duration);
      box.scrollTop = start + dist * easeOutBack(t);
      springRafRef.current = t < 1 ? requestAnimationFrame(step) : 0;
    };
    springRafRef.current = requestAnimationFrame(step);
  }

  function alignActiveLine(instant = false) {
    const box = listRef.current;
    const idx = activeIdxRef.current;
    if (!box || idx < 0) return;
    const el = box.querySelector<HTMLElement>(`[data-i="${idx}"]`);
    if (!el) return;
    const top = scrollTopToCenter(box, el);
    if (instant || prefersReducedMotion()) {
      cancelSpring();
      box.scrollTop = top;
    } else {
      springScrollTo(top);
    }
  }

  // 换行居中跟随（拖动中不抢滚动；seek 结束后也要重新对齐）
  useEffect(() => {
    if (!synced || activeIdx < 0 || draggingLrc || seeking) return;
    alignActiveLine(false);
  }, [activeIdx, synced, draggingLrc, seeking, lines.length]);

  // 行内粗进度 + 当前句识别（rAF；只在换行时 setState）
  useEffect(() => {
    if (!synced || lines.length === 0) return;
    let raf = 0;
    const tick = () => {
      const ms = currentTimeMs();
      const idx = findLrcIndex(lines, ms);
      if (idx !== activeIdxRef.current) {
        activeIdxRef.current = idx;
        setActiveIdx(idx);
      }
      const box = listRef.current;
      if (box && idx >= 0) {
        const el = box.querySelector<HTMLElement>(`[data-i="${idx}"]`);
        if (el) {
          const p = lineProgress(lines[idx], ms);
          el.style.setProperty("--line-p", `${(p * 100).toFixed(2)}%`);
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [synced, lines]);

  // 切歌清空行状态
  useEffect(() => {
    activeIdxRef.current = -1;
    setActiveIdx(-1);
    setFlashIdx(-1);
  }, [path]);

  useEffect(() => () => {
    cancelSpring();
    window.clearTimeout(flashTimerRef.current);
  }, []);

  function requestClose() {
    if (closing) return;
    setClosing(true);
    window.setTimeout(() => setFullPlayer(false), 280);
  }

  /** 纵向拖动歌词：列表跟手滚动 + 进度偏移（上拖前进）；点击句子跳转 */
  function onLrcPointerDown(e: React.MouseEvent) {
    const box = listRef.current;
    if (!box) return;
    cancelSpring();
    const start = seeking ? seekMs : pos;
    dragRef.current = {
      y: e.clientY,
      startMs: start,
      startScroll: box.scrollTop,
      moved: false,
      lastMs: start,
    };
    setDraggingLrc(true);
  }

  function onLrcPointerMove(e: React.MouseEvent) {
    const d = dragRef.current;
    const box = listRef.current;
    if (!d || !box) return;
    const dy = d.y - e.clientY;
    if (!d.moved && Math.abs(dy) < 4) return;
    d.moved = true;
    skipClickRef.current = true;
    box.scrollTop = d.startScroll + dy;
    const boxH = box.clientHeight || 480;
    const msPerPx = Math.max(dur / (boxH * 3), 20);
    const nextMs = Math.min(dur, Math.max(0, d.startMs + dy * msPerPx));
    d.lastMs = nextMs;
    setSeeking(true);
    setSeekMs(nextMs);
  }

  /** 提交进度：seek 结束后退出 scrub 并对齐时钟，避免歌词冻住 */
  async function commitSeek(ms: number) {
    const target = Math.round(ms);
    setSeekMs(target);
    setSeeking(true);
    reanchorClock(target);
    try {
      await seek(target);
    } finally {
      reanchorClock(target);
      setSeeking(false);
    }
  }

  function onLrcPointerUp() {
    const d = dragRef.current;
    dragRef.current = null;
    setDraggingLrc(false);
    if (d?.moved) {
      void commitSeek(d.lastMs);
    } else {
      setSeeking(false);
    }
    window.setTimeout(() => {
      skipClickRef.current = false;
    }, 0);
  }

  function flashLine(i: number) {
    setFlashIdx(i);
    window.clearTimeout(flashTimerRef.current);
    flashTimerRef.current = window.setTimeout(() => setFlashIdx(-1), 400);
  }

  function seekToLine(i: number, ms: number) {
    if (skipClickRef.current) return;
    flashLine(i);
    void commitSeek(ms);
  }

  const title = info?.title || track?.title || info?.filename || "未在播放";
  const artist = info?.artist || info?.album_artist || "";
  const cover = info?.cover_data ?? null;
  const showMs = seeking ? seekMs : pos;
  const remain = Math.max(dur - showMs, 0);
  const pct = Math.min(100, Math.max(0, (showMs / dur) * 100));
  const vol = player?.volume ?? 0.8;

  function seekFromEvent(e: React.MouseEvent<HTMLElement>) {
    const rect = e.currentTarget.getBoundingClientRect();
    const ratio = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
    return Math.round(ratio * dur);
  }

  return (
    <div className={`np-page${playing ? " playing" : ""}${closing ? " closing" : ""}`}>
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
              <h2 className="np-title">{title}</h2>
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
              <div className="np-seek-fill" style={{ width: `${pct}%` }} />
            </div>
            <div className="np-seek-times">
              <span>{formatTime(showMs)}</span>
              <span>-{formatTime(remain)}</span>
            </div>
          </div>

          <div className="np-btns">
            <button className="np-icon" title="随机（占位）" disabled>
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
            <button className="np-icon" title="循环（占位）" disabled>
              <Repeat size={18} />
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
            className={`np-lyrics${draggingLrc ? " dragging" : ""}`}
            ref={listRef}
            onMouseDown={onLrcPointerDown}
            onMouseMove={onLrcPointerMove}
            onMouseUp={onLrcPointerUp}
            onMouseLeave={() => {
              if (dragRef.current) onLrcPointerUp();
            }}
          >
            {!track && (
              <div className="np-lyrics-empty tertiary">从专辑墙或管理表挑一首开始</div>
            )}
            {track && !synced && plain.length === 0 && (
              <div className="np-lyrics-empty tertiary">
                这首歌没有歌词。可到管理页「歌词」搜索。
              </div>
            )}
            {track && !synced && plain.length > 0 && (
              <div className="np-lines">
                <div className="np-line-spacer" aria-hidden />
                {plain.map((t, i) => (
                  <div key={i} className="np-line plain d2">
                    <div className="np-main">{t}</div>
                  </div>
                ))}
                <div className="np-line-spacer" aria-hidden />
              </div>
            )}
            {synced && (
              <div className="np-lines">
                <div className="np-line-spacer" aria-hidden />
                {lines.map((l: LrcLine, i) => {
                  const on = i === activeIdx;
                  const state = on ? "on" : i < activeIdx ? "past" : "next";
                  const dist = Math.min(4, Math.abs(i - activeIdx));
                  const text = l.text || "⋯";
                  return (
                    <div
                      key={`${l.timeMs}-${i}`}
                      data-i={i}
                      className={[
                        "np-line",
                        state,
                        `d${dist}`,
                        flashIdx === i ? "flash" : "",
                      ]
                        .filter(Boolean)
                        .join(" ")}
                      onClick={() => seekToLine(i, l.timeMs)}
                    >
                      <div className="np-main">
                        <span className="np-main-base">{text}</span>
                        {on ? (
                          <span className="np-main-fill" aria-hidden>
                            {text}
                          </span>
                        ) : null}
                      </div>
                      {l.trans ? <div className="np-trans">{l.trans}</div> : null}
                    </div>
                  );
                })}
                <div className="np-line-spacer" aria-hidden />
              </div>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
