import { FolderInput, ListPlus, Pause, Play, Repeat, Shuffle, SkipBack, SkipForward, Volume2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { api, formatTime } from "../lib/api";
import { useApp } from "../state/useApp";
import "./MiniPlayer.css";

export function MiniPlayer() {
  const player = useApp((s) => s.player);
  const refreshPlayer = useApp((s) => s.refreshPlayer);
  const toggle = useApp((s) => s.toggle);
  const next = useApp((s) => s.next);
  const prev = useApp((s) => s.prev);
  const seek = useApp((s) => s.seek);
  const setVolume = useApp((s) => s.setVolume);

  const [seeking, setSeeking] = useState(false);
  const [seekMs, setSeekMs] = useState(0);
  const [outsideLib, setOutsideLib] = useState(false);
  const [including, setIncluding] = useState(false);
  const [includedAt, setIncludedAt] = useState<string | null>(null);
  const [savedAt, setSavedAt] = useState<string | null>(null);
  const pollRef = useRef<number | null>(null);

  useEffect(() => {
    void refreshPlayer();
    pollRef.current = window.setInterval(() => {
      void refreshPlayer();
    }, 500);
    return () => {
      if (pollRef.current != null) window.clearInterval(pollRef.current);
    };
  }, [refreshPlayer]);

  const track = player?.track ?? null;

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

  /** 队列存为歌单（另存为新歌单，重名不覆盖） */
  async function saveQueueAs() {
    const queue = player?.queue ?? [];
    if (queue.length === 0) return;
    const name = window.prompt("存为歌单名称", "");
    if (!name || !name.trim()) return;
    try {
      await api.playlistCreate(
        name.trim(),
        queue.map((q) => ({
          path: q.path,
          title: q.title,
          artist: "",
          duration_ms: q.duration_ms,
        })),
      );
      setSavedAt(name.trim());
      window.setTimeout(() => setSavedAt(null), 2000);
    } catch (e) {
      window.alert(String(e));
    }
  }

  const duration = player?.duration_ms ?? 0;
  const position = seeking ? seekMs : (player?.position_ms ?? 0);
  const volume = player?.volume ?? 0.8;
  const playing = player?.status === "Playing";

  const pct = duration > 0 ? Math.min(100, (position / duration) * 100) : 0;

  return (
    <footer className="mini-player" aria-label="迷你播放条">
      <div className="mp-track">
        <div className="mp-cover" aria-hidden>
          {track ? (track.title || "?").slice(0, 1) : "♪"}
        </div>
        <div className="mp-meta">
          <div className="mp-title">{track?.title ?? "未在播放"}</div>
          <div className="mp-sub tertiary">
            {track ? track.path.split(/[\\/]/).slice(-2).join(" / ") : "双击专辑墙或管理表开始"}
          </div>
        </div>
        {outsideLib && (
          <button
            className="mp-include"
            title="纳入库管理：复制进库目录并登记（当前文件在库外）"
            disabled={including}
            onClick={() => void includeInLibrary()}
          >
            <FolderInput size={14} />
            入库
          </button>
        )}
        {includedAt && track?.path === includedAt && (
          <span className="mp-included tertiary" title="已复制进库">
            已入库
          </span>
        )}
      </div>

      <div className="mp-center">
        <div className="mp-btns">
          <button className="mp-icon" title="随机（占位）" disabled>
            <Shuffle size={15} />
          </button>
          <button className="mp-icon" title="上一首" onClick={() => void prev()} disabled={!track}>
            <SkipBack size={17} />
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
            <SkipForward size={17} />
          </button>
          <button className="mp-icon" title="单曲循环（占位）" disabled>
            <Repeat size={15} />
          </button>
        </div>
        <div className="mp-progress">
          <span className="mono tertiary">{formatTime(position)}</span>
          <input
            className="mp-slider"
            type="range"
            min={0}
            max={Math.max(duration, 1)}
            value={position}
            aria-label="播放进度"
            onChange={(e) => {
              setSeeking(true);
              setSeekMs(Number(e.target.value));
            }}
            onPointerUp={(e) => {
              const ms = Number((e.target as HTMLInputElement).value);
              setSeeking(false);
              void seek(ms);
            }}
            onKeyUp={(e) => {
              const ms = Number((e.target as HTMLInputElement).value);
              if (e.key === "Enter" || e.key === "ArrowLeft" || e.key === "ArrowRight") {
                setSeeking(false);
                void seek(ms);
              }
            }}
            style={{ ["--pct" as string]: `${pct}%` }}
          />
          <span className="mono tertiary">{formatTime(duration)}</span>
        </div>
      </div>

      <div className="mp-volume">
        {savedAt ? (
          <span className="mp-included tertiary" title="已存为歌单">
            已存为「{savedAt}」
          </span>
        ) : (
          <button
            className="mp-icon"
            title="队列存为歌单"
            disabled={(player?.queue?.length ?? 0) === 0}
            onClick={() => void saveQueueAs()}
          >
            <ListPlus size={15} />
          </button>
        )}
        <Volume2 size={15} className="tertiary" />
        <input
          className="mp-slider mp-vol"
          type="range"
          min={0}
          max={1}
          step={0.01}
          value={volume}
          aria-label="音量"
          onChange={(e) => void setVolume(Number(e.target.value))}
          style={{ ["--pct" as string]: `${volume * 100}%` }}
        />
      </div>
    </footer>
  );
}
