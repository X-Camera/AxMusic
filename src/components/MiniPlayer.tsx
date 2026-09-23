import { FolderInput, ListMusic, Pause, Play, Repeat, Repeat1, Shuffle, SkipBack, SkipForward, Volume2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import type { QueueItem } from "../lib/types";

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
  const setFullPlayer = useApp((s) => s.setFullPlayer);
  const playQueue = useApp((s) => s.playQueue);

  const [seeking, setSeeking] = useState(false);
  const [seekMs, setSeekMs] = useState(0);
  const seekMsRef = useRef(0);
  const seekingRef = useRef(false);
  const [outsideLib, setOutsideLib] = useState(false);
  const [including, setIncluding] = useState(false);
  const [includedAt, setIncludedAt] = useState<string | null>(null);
  const [savedAt, setSavedAt] = useState<string | null>(null);
  const [cover, setCover] = useState<string | null>(null);
  const [queueOpen, setQueueOpen] = useState(false);
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
  const playMode = player?.play_mode ?? "sequential";
  const setPlayMode = useApp((s) => s.setPlayMode);
  const playing = player?.status === "Playing";
  const queueLen = player?.queue?.length ?? 0;

  const pct = duration > 0 ? Math.min(100, (position / duration) * 100) : 0;

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

  // 队列弹层：点外关闭
  useEffect(() => {
    if (!queueOpen) return;
    const onDown = (e: PointerEvent) => {
      const t = e.target as HTMLElement | null;
      if (t?.closest(".mp-queue-wrap")) return;
      setQueueOpen(false);
    };
    window.addEventListener("pointerdown", onDown);
    return () => window.removeEventListener("pointerdown", onDown);
  }, [queueOpen]);

  async function playQueueAt(items: QueueItem[], index: number) {
    setQueueOpen(false);
    await playQueue(items, index);
  }

  return (
    <footer className="mini-player" aria-label="迷你播放条">
      <div className="mp-seek-wrap">
        <input
          className="mp-slider mp-seek"
          type="range"
          min={0}
          max={Math.max(duration, 1)}
          value={position}
          aria-label="播放进度"
          disabled={!track && duration <= 0}
          onChange={(e) => onSeekInput(Number(e.target.value))}
          onKeyUp={(e) => {
            if (e.key === "Enter" || e.key === "ArrowLeft" || e.key === "ArrowRight" || e.key === "Home" || e.key === "End") {
              void commitSeek();
            }
          }}
          style={{
            ["--pct" as string]: String(pct),
            ["--thumb-w" as string]: "16px",
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
        <span className="mp-time mono tertiary">
          {formatTime(position)} / {formatTime(duration)}
        </span>
        <button
          className={`mp-icon mode${playMode === "shuffle" ? " active" : ""}`}
          title={playMode === "shuffle" ? "随机播放（开）" : "随机播放"}
          onClick={() =>
            void setPlayMode(playMode === "shuffle" ? "sequential" : "shuffle")
          }
        >
          <Shuffle size={15} />
        </button>
        <button
          className={`mp-icon mode${playMode === "repeat_one" ? " active" : ""}`}
          title={playMode === "repeat_one" ? "单曲循环（开）" : "单曲循环"}
          onClick={() =>
            void setPlayMode(
              playMode === "repeat_one" ? "sequential" : "repeat_one",
            )
          }
        >
          {playMode === "repeat_one" ? <Repeat1 size={15} /> : <Repeat size={15} />}
        </button>
        <div className="mp-volume">
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
            style={{
              ["--pct" as string]: String(volume * 100),
              ["--thumb-w" as string]: "12px",
            }}
          />
        </div>
        <div className="mp-queue-wrap">
          <button
            className="mp-icon mp-queue"
            title={queueLen > 0 ? `播放队列 · ${queueLen} 首` : "播放队列为空"}
            aria-expanded={queueOpen}
            onClick={() => setQueueOpen((v) => !v)}
          >
            <ListMusic size={15} />
            {queueLen > 0 && <span className="mp-queue-badge">{queueLen > 99 ? "99+" : queueLen}</span>}
          </button>
          {queueOpen && (
            <div className="mp-queue-panel" role="listbox" aria-label="播放队列">
              <div className="mp-queue-head">
                <span>播放队列 · {queueLen} 首</span>
                {savedAt ? (
                  <span className="tertiary" title="已存为歌单">
                    已存为「{savedAt}」
                  </span>
                ) : (
                  <button
                    className="link-btn"
                    disabled={queueLen === 0}
                    onClick={() => void saveQueueAs()}
                  >
                    存为歌单
                  </button>
                )}
              </div>
              <div className="mp-queue-list">
                {queueLen === 0 ? (
                  <div className="tertiary mp-queue-empty">队列为空</div>
                ) : (
                  (player?.queue ?? []).map((q, i) => (
                    <button
                      key={`${q.path}-${i}`}
                      className={`mp-queue-item${i === player?.queue_index ? " active" : ""}`}
                      role="option"
                      aria-selected={i === player?.queue_index}
                      onClick={() => void playQueueAt(player?.queue ?? [], i)}
                    >
                      <span className="mp-queue-item-title">{q.title || q.path.split(/[\\/]/).pop()}</span>
                      <span className="mono tertiary">{formatTime(q.duration_ms)}</span>
                    </button>
                  ))
                )}
              </div>
            </div>
          )}
        </div>
      </div>
    </footer>
  );
}
