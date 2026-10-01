import { Captions, FolderInput, ListMusic, Pause, Play, Repeat, Repeat1, Shuffle, SkipBack, SkipForward, Volume1, Volume2, VolumeX } from "lucide-react";
import { notNil } from "../lib/nil";
import { useEffect, useRef, useState } from "react";

import { api, formatTime } from "../lib/api";
import {
  clockNow,
  clockReanchor,
  clockSyncFromSnapshot,
  createPlayClock,
} from "../lib/playClock";
import { nextRepeat, REPEAT_TITLE } from "../lib/playMode";
import { DEFAULT_VOLUME, LOW_VOLUME_THRESHOLD } from "../lib/volume";
import { useApp } from "../state/useApp";
import { FavoriteHeart } from "./FavoriteHeart";
import { ReplayGainBadge } from "./ReplayGainBadge";
import "./MiniPlayer.css";

export function MiniPlayer() {
  const player = useApp((s) => s.player);
  const refreshPlayer = useApp((s) => s.refreshPlayer);
  const toggle = useApp((s) => s.toggle);
  const next = useApp((s) => s.next);
  const prev = useApp((s) => s.prev);
  const seek = useApp((s) => s.seek);
  const setVolume = useApp((s) => s.setVolume);
  const setFullPlayer = useApp((s) => s.setFullPlayer);
  const queuePanelOpen = useApp((s) => s.queuePanelOpen);
  const toggleQueuePanel = useApp((s) => s.toggleQueuePanel);
  const lyricsPanelOpen = useApp((s) => s.lyricsPanelOpen);
  const toggleLyricsPanel = useApp((s) => s.toggleLyricsPanel);
  const playerError = useApp((s) => s.playerError);
  const clearPlayerError = useApp((s) => s.clearPlayerError);

  // 引擎错误（如格式暂不支持）：弹 4s 自动消失，点击立即关闭
  useEffect(() => {
    if (!playerError) return;
    const t = window.setTimeout(clearPlayerError, 4000);
    return () => window.clearTimeout(t);
  }, [playerError, clearPlayerError]);

  const [seeking, setSeeking] = useState(false);
  const [seekMs, setSeekMs] = useState(0);
  const seekMsRef = useRef(0);
  const seekingRef = useRef(false);
  const [outsideLib, setOutsideLib] = useState(false);
  const [including, setIncluding] = useState(false);
  const [includedAt, setIncludedAt] = useState<string | null>(null);
  const [cover, setCover] = useState<string | null>(null);
  const pollRef = useRef<number | null>(null);
  /** 进度圆点：轮询 500ms 太粗，用时钟外推 + rAF 直接写 DOM，避免一顿一顿 */
  const seekInputRef = useRef<HTMLInputElement>(null);
  const timeLabelRef = useRef<HTMLSpanElement>(null);
  const clockRef = useRef(createPlayClock());
  const lastPathRef = useRef("");

  useEffect(() => {
    void refreshPlayer();
    pollRef.current = window.setInterval(() => {
      // 后台标签/最小化时暂停轮询，省 CPU
      if (document.hidden) return;
      void refreshPlayer();
    }, 500);
    return () => {
      if (notNil(pollRef.current)) window.clearInterval(pollRef.current);
    };
  }, [refreshPlayer]);

  const track = player?.track ?? null;

  // 封面小图（懒取，切歌刷新）
  useEffect(() => {
    let cancelled = false;
    setCover(null);
    if (!track?.path) return;
    void api
      .trackCoverThumb(track.path)
      .then((url) => {
        if (!cancelled) setCover(url);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [track?.path]);

  // 「纳入库管理」只对库外文件有意义
  useEffect(() => {
    let cancelled = false;
    setIncludedAt(null);
    if (!track?.path) {
      setOutsideLib(false);
      return;
    }
    void api
      .isInLibrary(track.path)
      .then((inLib) => {
        if (!cancelled) setOutsideLib(!inLib);
      })
      .catch(() => {
        if (!cancelled) setOutsideLib(false);
      });
    return () => {
      cancelled = true;
    };
  }, [track?.path]);

  async function includeInLibrary() {
    if (!track?.path || including) return;
    setIncluding(true);
    try {
      await api.includeInLibrary(track.path);
      setOutsideLib(false);
      setIncludedAt(track.path);
    } catch {
      /* keep button visible for retry */
    } finally {
      setIncluding(false);
    }
  }

  const duration = player?.duration_ms ?? 0;
  const volume = player?.volume ?? DEFAULT_VOLUME;
  const shuffle = player?.shuffle ?? false;
  const repeat = player?.repeat ?? "off";
  const setShuffle = useApp((s) => s.setShuffle);
  const setRepeat = useApp((s) => s.setRepeat);
  const playing = player?.status === "Playing";
  const queueLen = player?.queue?.length ?? 0;
  const trackPath = track?.path ?? "";
  /** 静音前音量：点喇叭恢复用 */
  const lastVolRef = useRef(DEFAULT_VOLUME);
  /** 静音意图：不依赖 setVolume 回填前的旧 volume，避免连点切换失效 */
  const mutedRef = useRef(false);

  /** 点喇叭：静音 ↔ 恢复到静音前音量 */
  function toggleMute() {
    if (!mutedRef.current && volume > 0) {
      lastVolRef.current = volume;
      mutedRef.current = true;
      void setVolume(0);
    } else {
      mutedRef.current = false;
      void setVolume(lastVolRef.current);
    }
  }

  /** 音量图标：静音 / 低 / 正常（避免 JSX 嵌套三元） */
  function renderVolumeIcon() {
    if (volume <= 0) return <VolumeX size={15} />;
    if (volume < LOW_VOLUME_THRESHOLD) return <Volume1 size={15} />;
    return <Volume2 size={15} />;
  }

  // 轮询快照只作锚点；拖动中跟 seekMs；播放中由 rAF 外推
  useEffect(() => {
    const force = trackPath !== lastPathRef.current;
    lastPathRef.current = trackPath;
    if (seeking) {
      clockReanchor(clockRef.current, seekMs, false);
      return;
    }
    clockSyncFromSnapshot(clockRef.current, player?.position_ms ?? 0, playing, { force });
  }, [player?.position_ms, playing, seeking, seekMs, trackPath]);

  // rAF 平滑绘制圆点与时间（绕开 React 受控 value 的 500ms 阶跃）
  useEffect(() => {
    let raf = 0;
    let lastMs = -1;
    const paint = () => {
      raf = requestAnimationFrame(paint);
      const input = seekInputRef.current;
      if (!input) return;
      const max = Math.max(Number(input.max) || duration || 1, 1);
      let ms: number;
      if (seekingRef.current) {
        ms = seekMsRef.current;
      } else {
        ms = clockNow(clockRef.current);
        const rounded = Math.round(ms);
        if (rounded !== lastMs) {
          lastMs = rounded;
          input.value = String(Math.min(rounded, max));
        }
      }
      const clamped = Math.min(Math.max(ms, 0), max);
      input.style.setProperty("--pct", String((clamped / max) * 100));
      if (timeLabelRef.current) {
        timeLabelRef.current.textContent = `${formatTime(Math.round(clamped))} / ${formatTime(max)}`;
      }
    };
    raf = requestAnimationFrame(paint);
    return () => cancelAnimationFrame(raf);
  }, [duration]);

  /** 拖动中只改本地位置；松手后 await seek，避免旧 position_ms 把圆点打回去 */
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
      // 若 seek 期间又开始新的拖动，保持本地位置
      if (!seekingRef.current) setSeeking(false);
    }
  }

  const commitSeekRef = useRef(commitSeek);
  commitSeekRef.current = commitSeek;

  // 窗口级 pointerup：拖出进度条再松手也能提交
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

  return (
    <footer className="mini-player" aria-label="迷你播放条">
      {playerError && (
        <button
          key={playerError.seq}
          type="button"
          className="mp-error-toast"
          onClick={clearPlayerError}
        >
          {playerError.msg}
        </button>
      )}
      <div className="mp-seek-wrap">
        <input
          ref={seekInputRef}
          className="mp-slider mp-seek"
          type="range"
          min={0}
          max={Math.max(duration, 1)}
          defaultValue={0}
          aria-label="播放进度"
          disabled={!track && duration <= 0}
          onChange={(e) => onSeekInput(Number(e.target.value))}
          onKeyUp={(e) => {
            if (e.key === "Enter" || e.key === "ArrowLeft" || e.key === "ArrowRight" || e.key === "Home" || e.key === "End") {
              void commitSeek();
            }
          }}
          style={{
            ["--thumb-w" as string]: "14px",
          }}
        />
      </div>

      <div className="mp-track">
        <button
          className="mp-cover-btn"
          title={track ? "打开满窗播放" : "未在播放"}
          disabled={!track}
          onClick={() => setFullPlayer(true)}
        >
          {cover ? (
            <img className="mp-cover-img" src={cover} alt="" />
          ) : (
            <span className="mp-cover" aria-hidden>
              {track ? (track.title || "?").slice(0, 1) : "♪"}
            </span>
          )}
        </button>
        <div className="mp-meta">
          <div className="mp-title-row">
            <button
              className="mp-title"
              title={track ? "打开满窗播放" : "未在播放"}
              disabled={!track}
              onClick={() => setFullPlayer(true)}
            >
              {track?.title ?? "未在播放"}
            </button>
            <ReplayGainBadge info={player?.replaygain} />
            {track && (
              <FavoriteHeart
                item={{
                  path: track.path,
                  title: track.title,
                  artist: "",
                  duration_ms: track.duration_ms,
                }}
                size={13}
                className="inline"
              />
            )}
            {outsideLib && (
              <button
                className="mp-include"
                title="纳入库管理：复制进库目录并登记（当前文件在库外）"
                disabled={including}
                onClick={() => void includeInLibrary()}
              >
                <FolderInput size={13} />
                入库
              </button>
            )}
            {includedAt && track?.path === includedAt && (
              <span className="mp-included tertiary" title="已复制进库">
                已入库
              </span>
            )}
          </div>
          <div className="mp-sub tertiary">
            {track ? track.path.split(/[\\/]/).slice(-2).join(" / ") : "双击专辑或管理表开始"}
          </div>
        </div>
      </div>

      <div className="mp-center">
        <div className="mp-btns">
          <button className="mp-icon" title="上一首" onClick={() => void prev()} disabled={!track}>
            <SkipBack size={18} fill="currentColor" />
          </button>
          <button
            className="mp-play"
            title={playing ? "暂停" : "播放"}
            onClick={() => void toggle()}
            disabled={!track && (player?.queue?.length ?? 0) === 0}
          >
            {playing ? <Pause size={18} fill="currentColor" /> : <Play size={18} fill="currentColor" />}
          </button>
          <button className="mp-icon" title="下一首" onClick={() => void next()} disabled={!track}>
            <SkipForward size={18} fill="currentColor" />
          </button>
        </div>
      </div>

      <div className="mp-right">
        <span ref={timeLabelRef} className="mp-time mono tertiary" />
        <button
          className={`mp-icon mode${shuffle ? " active" : ""}`}
          title={shuffle ? "随机播放（开）" : "随机播放"}
          onClick={() => void setShuffle(!shuffle)}
        >
          <Shuffle size={15} />
        </button>
        <button
          className={`mp-icon mode${repeat !== "off" ? " active" : ""}`}
          title={REPEAT_TITLE[repeat]}
          onClick={() => void setRepeat(nextRepeat(repeat))}
        >
          {repeat === "one" ? <Repeat1 size={15} /> : <Repeat size={15} />}
        </button>
        <div className="mp-volume">
          <button
            type="button"
            className="mp-icon mp-vol-btn"
            title={volume > 0 ? "静音" : "恢复音量"}
            aria-label={volume > 0 ? "静音" : "恢复音量"}
            onClick={toggleMute}
          >
            {renderVolumeIcon()}
          </button>
          <input
            className="mp-slider mp-vol"
            type="range"
            min={0}
            max={1}
            step={0.01}
            value={volume}
            aria-label="音量"
            onChange={(e) => {
              const v = Number(e.target.value);
              if (v > 0) {
                lastVolRef.current = v;
                mutedRef.current = false;
              }
              void setVolume(v);
            }}
            style={{
              ["--pct" as string]: String(volume * 100),
              ["--thumb-w" as string]: "12px",
            }}
          />
        </div>
        <div className="mp-queue-wrap">
          <button
            className={`mp-icon mp-queue${lyricsPanelOpen ? " active" : ""}`}
            title={lyricsPanelOpen ? "收起歌词" : "歌词"}
            aria-expanded={lyricsPanelOpen}
            onClick={() => toggleLyricsPanel()}
          >
            <Captions size={15} />
          </button>
          <button
            className={`mp-icon mp-queue${queuePanelOpen ? " active" : ""}`}
            title={
              queuePanelOpen
                ? "收起播放列表"
                : queueLen > 0
                  ? `播放列表 · ${queueLen} 首`
                  : "播放列表为空"
            }
            aria-expanded={queuePanelOpen}
            onClick={() => toggleQueuePanel()}
          >
            <ListMusic size={15} />
            {queueLen > 0 && <span className="mp-queue-badge">{queueLen > 99 ? "99+" : queueLen}</span>}
          </button>
        </div>
      </div>
    </footer>
  );
}
