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
import { findLrcIndex, pickLyrics, type LrcLine } from "./lrc";
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
  const listRef = useRef<HTMLDivElement>(null);
  /** 松手后对齐歌词用瞬时滚动，避免 smooth 造成回弹感 */
  const snapAlignRef = useRef(false);
  const dragRef = useRef<{
    y: number;
    startMs: number;
    startScroll: number;
    moved: boolean;
    lastMs: number;
  } | null>(null);
  const skipClickRef = useRef(false);

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

  // 拖动中 / 跳转未落盘时用预览进度，避免 active 行回跳
  const displayMs = seeking ? seekMs : pos;
  const activeIdx = useMemo(
    () => (synced ? findLrcIndex(lines, displayMs) : -1),
    [synced, lines, displayMs],
  );

  // 播放器位置跟上预览后，再退出 seeking
  useEffect(() => {
    if (!seeking || draggingLrc) return;
    if (Math.abs(pos - seekMs) < 400) setSeeking(false);
  }, [pos, seekMs, seeking, draggingLrc]);

  // 自动跟随（拖动中暂停；松手对齐用瞬时，之后恢复 smooth）
  useEffect(() => {
    if (!synced || activeIdx < 0 || draggingLrc) return;
    const box = listRef.current;
    if (!box) return;
    const el = box.querySelector<HTMLElement>(`[data-i="${activeIdx}"]`);
    if (!el) return;
    const boxRect = box.getBoundingClientRect();
    const elRect = el.getBoundingClientRect();
    const delta = elRect.top + elRect.height / 2 - (boxRect.top + boxRect.height / 2);
    const top = Math.max(0, box.scrollTop + delta);
    const snap = snapAlignRef.current;
    snapAlignRef.current = false;
    box.scrollTo({ top, behavior: snap ? "auto" : "smooth" });
  }, [activeIdx, synced, lines.length, draggingLrc]);

  function requestClose() {
    if (closing) return;
    setClosing(true);
    window.setTimeout(() => setFullPlayer(false), 280);
  }

  /** 纵向拖动歌词：列表跟手滚动 + 进度偏移（上拖前进）；点击句子跳转 */
  function onLrcPointerDown(e: React.MouseEvent) {
    const box = listRef.current;
    if (!box) return;
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

  function onLrcPointerUp() {
    const d = dragRef.current;
    dragRef.current = null;
    setDraggingLrc(false);
    if (d?.moved) {
      const ms = Math.round(d.lastMs);
      setSeekMs(ms);
      setSeeking(true);
      snapAlignRef.current = true;
      void seek(ms);
      // 不立刻 setSeeking(false)：等播放器位置跟上，避免进度/歌词回跳
    } else {
      setSeeking(false);
    }
    window.setTimeout(() => {
      skipClickRef.current = false;
    }, 0);
  }

  function seekToLine(ms: number) {
    if (skipClickRef.current) return;
    void seek(ms);
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
            onClick={(e) => {
              void seek(seekFromEvent(e));
              setSeeking(false);
            }}
            onMouseDown={() => setSeeking(true)}
            onMouseMove={(e) => {
              if (!seeking) return;
              setSeekMs(seekFromEvent(e));
            }}
            onMouseUp={() => setSeeking(false)}
            onMouseLeave={() => setSeeking(false)}
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
                  <div key={i} className="np-line plain">
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
                  const state = i === activeIdx ? "on" : i < activeIdx ? "past" : "next";
                  return (
                    <div
                      key={`${l.timeMs}-${i}`}
                      data-i={i}
                      className={`np-line ${state}`}
                      onClick={() => seekToLine(l.timeMs)}
                    >
                      <div className="np-main">{l.text || "⋯"}</div>
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
