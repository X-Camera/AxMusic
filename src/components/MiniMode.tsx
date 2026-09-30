import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Maximize2,
  Pause,
  Play,
  SkipBack,
  SkipForward,
  X,
} from "lucide-react";
import { notNil } from "../lib/nil";
import { useEffect, useMemo, useRef, useState } from "react";

import { api, formatTime } from "../lib/api";
import {
  clockNow,
  clockReanchor,
  clockSyncFromSnapshot,
  createPlayClock,
} from "../lib/playClock";
import { exitMiniMode } from "../lib/miniMode";
import { useApp } from "../state/useApp";
import { BrandMark } from "./BrandMark";
import { findLrcIndex, pickLyrics } from "../features/player/lrc";
import type { LyricsCurrent } from "../lib/types";
import "./MiniMode.css";

/**
 * mini 模式：同窗缩成桌面角紧凑小卡。
 * 左封面｜右上：歌词+还原/关闭；中：纯进度条；下：三键+时间。
 */
export function MiniMode() {
  const player = useApp((s) => s.player);
  const refreshPlayer = useApp((s) => s.refreshPlayer);
  const toggle = useApp((s) => s.toggle);
  const next = useApp((s) => s.next);
  const prev = useApp((s) => s.prev);
  const seek = useApp((s) => s.seek);
  const setFullPlayer = useApp((s) => s.setFullPlayer);
  const setMiniMode = useApp((s) => s.setMiniMode);

  const track = player?.track ?? null;
  const path = track?.path ?? "";
  const playing = player?.status === "Playing";
  const duration = player?.duration_ms ?? 0;

  const [cover, setCover] = useState<string | null>(null);
  const [lyricState, setLyricState] = useState<{
    path: string;
    info: LyricsCurrent | null;
  }>({ path: "", info: null });
  /** path 一变立刻视为空歌词，避免旧句残影 */
  const lyricInfo = lyricState.path === path ? lyricState.info : null;

  const [seeking, setSeeking] = useState(false);
  const [seekMs, setSeekMs] = useState(0);
  const seekMsRef = useRef(0);
  const seekingRef = useRef(false);
  const seekInputRef = useRef<HTMLInputElement>(null);
  const timeLabelRef = useRef<HTMLSpanElement>(null);
  const lyricLineRef = useRef<HTMLDivElement>(null);
  const clockRef = useRef(createPlayClock());
  const lastPathRef = useRef("");
  const pollRef = useRef<number | null>(null);

  // 播放轮询（mini 自持，主壳已卸载）；回前台立刻对齐，避免隐藏期墙钟外推过冲
  useEffect(() => {
    void refreshPlayer();
    pollRef.current = window.setInterval(() => {
      if (document.hidden) return;
      void refreshPlayer();
    }, 500);
    const onVisible = () => {
      if (!document.hidden) void refreshPlayer();
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      if (notNil(pollRef.current)) window.clearInterval(pollRef.current);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [refreshPlayer]);

  // 封面
  useEffect(() => {
    let cancelled = false;
    setCover(null);
    if (!path) return;
    void api
      .trackCoverThumb(path)
      .then((url) => {
        if (!cancelled) setCover(url);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [path]);

  // 歌词（嵌/挂，外挂优先）
  useEffect(() => {
    let cancelled = false;
    setLyricState({ path: "", info: null });
    if (!path) return;
    void api
      .lyricsCurrent(null, path)
      .then((info) => {
        if (!cancelled) setLyricState({ path, info });
      })
      .catch(() => {
        if (!cancelled) setLyricState({ path, info: null });
      });
    return () => {
      cancelled = true;
    };
  }, [path]);

  const lyrics = useMemo(() => {
    if (!lyricInfo) return { lines: [], plain: [], synced: false };
    return pickLyrics(lyricInfo.embedded, lyricInfo.sidecar, "sidecar");
  }, [lyricInfo]);

  // 时钟锚点：拖动中跟 seekMs，播放中由 rAF 外推
  useEffect(() => {
    const force = path !== lastPathRef.current;
    if (force) {
      // 切歌丢弃未提交的拖动，避免把旧曲 seekMs 提交给新曲
      seekingRef.current = false;
      seekMsRef.current = 0;
      setSeeking(false);
    }
    lastPathRef.current = path;
    if (seeking) {
      clockReanchor(clockRef.current, seekMs, false);
      return;
    }
    clockSyncFromSnapshot(clockRef.current, player?.position_ms ?? 0, playing, { force });
  }, [player?.position_ms, playing, seeking, seekMs, path]);

  // rAF：进度 + 时间 + 当前歌词句
  useEffect(() => {
    let raf = 0;
    let lastMs = -1;
    let lastLine = -2;
    const paint = () => {
      raf = requestAnimationFrame(paint);
      const input = seekInputRef.current;
      const max = Math.max(duration, 1);
      let ms: number;
      if (seekingRef.current) {
        ms = seekMsRef.current;
      } else {
        ms = clockNow(clockRef.current);
        const rounded = Math.round(ms);
        if (rounded !== lastMs && input) {
          lastMs = rounded;
          input.value = String(Math.min(rounded, max));
        }
      }
      const clamped = Math.min(Math.max(ms, 0), max);
      if (input) input.style.setProperty("--pct", String((clamped / max) * 100));
      if (timeLabelRef.current) {
        timeLabelRef.current.textContent = formatTime(Math.round(clamped));
      }

      const lineEl = lyricLineRef.current;
      if (lineEl) {
        let idx: number;
        if (lyrics.synced) {
          idx = findLrcIndex(lyrics.lines, clamped);
        } else {
          idx = -1;
        }
        if (idx !== lastLine) {
          lastLine = idx;
          if (lyrics.synced && idx >= 0) {
            const line = lyrics.lines[idx];
            lineEl.textContent = line.trans ? `${line.text}  ·  ${line.trans}` : line.text;
            lineEl.dataset.state = "on";
          } else {
            // 无歌词不回退歌名（底行已显示）
            lineEl.textContent = "";
            lineEl.dataset.state = "off";
          }
        }
      }
    };
    raf = requestAnimationFrame(paint);
    return () => cancelAnimationFrame(raf);
  }, [duration, lyrics, track?.title]);

  function onSeekInput(ms: number) {
    seekingRef.current = true;
    seekMsRef.current = ms;
    setSeeking(true);
    setSeekMs(ms);
  }

  async function commitSeek() {
    if (!seekingRef.current) return;
    seekingRef.current = false;
    const ms = seekMsRef.current;
    try {
      await seek(ms);
    } finally {
      if (!seekingRef.current) setSeeking(false);
    }
  }

  const commitSeekRef = useRef(commitSeek);
  commitSeekRef.current = commitSeek;

  useEffect(() => {
    const end = () => {
      void commitSeekRef.current();
    };
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    return () => {
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
    };
  }, []);

  async function backToDefault() {
    try {
      await exitMiniMode();
      setMiniMode(false);
    } catch {
      setMiniMode(false);
      setFullPlayer(false);
    }
  }

  return (
    <div className="mini-mode" data-tauri-drag-region="deep">
      <div className="mm-body">
        <div className="mm-cover-wrap">
          {cover ? (
            <img className="mm-cover-img" src={cover} alt="" draggable={false} />
          ) : (
            <div className="mm-cover-fallback" aria-hidden>
              <BrandMark size={80} />            </div>
          )}
        </div>

        <div className="mm-right">
          <div className="mm-top-row">
            <div
              ref={lyricLineRef}
              className="mm-lyric"
              data-state="off"
              title="当前歌词"
            />
            <div className="mm-actions">
              <button
                className="mm-icon"
                title="回到默认模式"
                aria-label="回到默认模式"
                onClick={() => void backToDefault()}
              >
                <Maximize2 size={12} />
              </button>
              <button
                className="mm-icon mm-close"
                title="关闭"
                aria-label="关闭"
                onClick={() => void getCurrentWindow().close()}
              >
                <X size={12} />
              </button>
            </div>
          </div>

          <div className="mm-seek-wrap">
            <input
              ref={seekInputRef}
              className="mm-slider"
              type="range"
              min={0}
              max={Math.max(duration, 1)}
              defaultValue={0}
              aria-label="播放进度"
              disabled={!track && duration <= 0}
              onChange={(e) => onSeekInput(Number(e.target.value))}
              onKeyUp={(e) => {
                if (
                  e.key === "Enter" ||
                  e.key === "ArrowLeft" ||
                  e.key === "ArrowRight" ||
                  e.key === "Home" ||
                  e.key === "End"
                ) {
                  void commitSeek();
                }
              }}
            />
          </div>

          <div className="mm-controls-row">
            <div className="mm-track-title" title={track?.title ?? ""}>
              {track?.title ?? "未在播放"}
            </div>
            <div className="mm-controls">
              <button
                className="mm-icon"
                title="上一首"
                aria-label="上一首"
                onClick={() => void prev()}
                disabled={!track}
              >
                <SkipBack size={15} fill="currentColor" />
              </button>
              <button
                className="mm-icon mm-play"
                title={playing ? "暂停" : "播放"}
                aria-label={playing ? "暂停" : "播放"}
                onClick={() => void toggle()}
                disabled={!track && (player?.queue?.length ?? 0) === 0}
              >
                {playing ? (
                  <Pause size={16} fill="currentColor" />
                ) : (
                  <Play size={16} fill="currentColor" />
                )}
              </button>
              <button
                className="mm-icon"
                title="下一首"
                aria-label="下一首"
                onClick={() => void next()}
                disabled={!track}
              >
                <SkipForward size={15} fill="currentColor" />
              </button>
            </div>
            <span ref={timeLabelRef} className="mm-time mono" />
          </div>
        </div>
      </div>
    </div>
  );
}
