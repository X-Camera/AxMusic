import { createPortal } from "react-dom";
import { Search, SlidersHorizontal, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { api } from "../lib/api";
import type { LyricsCurrent, LyricsFont, LyricsPrefer } from "../lib/types";
import {
  clockNow,
  clockReanchor,
  clockSyncFromSnapshot,
  createPlayClock,
  type PlayClock,
} from "../lib/playClock";
import { lyricsDisplayVars } from "../lib/lyricsDisplay";
import { onLyricsSaved, openLyricsWindow } from "../lib/lyricsWindow";
import { useApp } from "../state/useApp";
import { LyricsStyleDialog } from "../features/player/LyricsStyleDialog";
import { LyricsView, type LyricsViewHandle } from "../features/player/LyricsView";
import { pickLyrics } from "../features/player/lrc";
import "./SideLyrics.css";

/** 主页歌词格：与满窗共用 LyricsView；居中排版，样式/右键/搜索独立设置 */
export function SideLyrics() {
  const player = useApp((s) => s.player);
  const seek = useApp((s) => s.seek);
  const setLyricsPanelOpen = useApp((s) => s.setLyricsPanelOpen);

  const path = player?.track?.path ?? "";
  const posMs = player?.position_ms ?? 0;
  const playing = player?.status === "Playing";

  const [lyricState, setLyricState] = useState<{
    path: string;
    info: LyricsCurrent | null;
  }>({ path: "", info: null });
  /** 渲染期派生：path 一变立刻视为空歌词，避免旧歌词残影/飞入叠在新歌上 */
  const info = lyricState.path === path ? lyricState.info : null;
  const [prefer, setPrefer] = useState<LyricsPrefer>("sidecar");
  /** 主界面歌词显示参数（设置页「主界面歌词」/ 右键「歌词样式」） */
  const [disp, setDisp] = useState({
    fontScale: 1,
    font: "display" as LyricsFont,
    lineHeight: 1.25,
  });
  const [trackGen, setTrackGen] = useState(0);
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number } | null>(null);
  const [styleOpen, setStyleOpen] = useState(false);
  const lastPathRef = useRef("");
  const lastPosRef = useRef(0);
  const clockRef = useRef<PlayClock>(createPlayClock());
  const lyricsViewRef = useRef<LyricsViewHandle>(null);

  useEffect(() => {
    let cancelled = false;
    void api
      .getSettings()
      .then((s) => {
        if (cancelled) return;
        setPrefer(s.lyrics_prefer);
        setDisp({
          fontScale: s.side_lyrics_font_scale ?? 1,
          font: s.side_lyrics_font ?? "display",
          lineHeight: s.side_lyrics_line_height ?? 1.25,
        });
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  // 切歌拉歌词
  useEffect(() => {
    let cancelled = false;
    setLyricState({ path, info: null });
    setTrackGen((g) => g + 1);
    lastPathRef.current = path;
    lastPosRef.current = 0;
    if (!path) return;
    void api
      .lyricsCurrent(null, path)
      .then((cur) => {
        if (!cancelled) setLyricState({ path, info: cur });
      })
      .catch(() => {
        if (!cancelled) setLyricState({ path, info: { embedded: null, sidecar: null } });
      });
    return () => {
      cancelled = true;
    };
  }, [path]);

  // 搜索歌词子窗口保存后立刻刷新本格
  useEffect(() => {
    if (!path) return;
    return onLyricsSaved((t) => {
      if (t.path && t.path !== path) return;
      void api
        .lyricsCurrent(null, path)
        .then((cur) => {
          setLyricState({ path, info: cur });
          // 空歌词 → 有歌词：升代强制整表重建，避免引擎停在空态
          setTrackGen((g) => g + 1);
        })
        .catch(() => undefined);
    });
  }, [path]);

  // 与满窗同源播放时钟：轮询锚点 + 墙钟外推
  useEffect(() => {
    const force = path !== lastPathRef.current || Math.abs(posMs - lastPosRef.current) > 2000;
    lastPathRef.current = path;
    clockSyncFromSnapshot(clockRef.current, posMs, playing, { force });
    lastPosRef.current = posMs;
  }, [posMs, playing, path]);

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

  function getTimeMs() {
    return clockNow(clockRef.current);
  }

  async function onSeekLine(_i: number, ms: number) {
    clockReanchor(clockRef.current, ms, true);
    lyricsViewRef.current?.resetScroll();
    try {
      await seek(ms);
    } finally {
      clockReanchor(clockRef.current, ms, playing);
      lyricsViewRef.current?.resetScroll();
    }
  }

  /** 打开独立「搜索歌词」子窗口 */
  async function openLyricsSearch() {
    if (!path) return;
    const filename = path.split(/[\\/]/).pop() || path;
    const title = player?.track?.title || filename;
    const artist = "";
    let id = 0;
    try {
      const row = await api.getTrackByPath(path);
      if (row) id = row.id;
    } catch {
      /* 无库 / 查询失败：按库外文件处理 */
    }
    await openLyricsWindow({ id, path, title, artist, filename });
  }

  function onContextMenu(e: React.MouseEvent) {
    e.preventDefault();
    setCtxMenu({ x: e.clientX, y: e.clientY });
  }

  /** 右键「歌词样式」：实时预览 + 落盘主界面歌词设置 */
  function patchDisp(next: Partial<typeof disp>) {
    const merged = { ...disp, ...next };
    setDisp(merged);
    void api
      .updateSettings({
        side_lyrics_font_scale: merged.fontScale,
        side_lyrics_font: merged.font,
        side_lyrics_line_height: merged.lineHeight,
      })
      .catch(() => void 0);
  }

  const { lines, plain, synced } = useMemo(
    () => pickLyrics(info?.embedded ?? null, info?.sidecar ?? null, prefer),
    [info, prefer],
  );

  return (
    <div className="side-lyrics" aria-label="歌词">
      <div className="side-lyrics-head">
        <span className="side-lyrics-title" title={player?.track?.title || "未在播放"}>
          歌词
        </span>
        <div className="side-lyrics-head-actions">
          {synced && playing && <span className="tertiary side-lyrics-live">同步</span>}
          <button
            className="queue-panel-close"
            title="收起歌词"
            aria-label="收起歌词"
            onClick={() => setLyricsPanelOpen(false)}
          >
            <X size={14} />
          </button>
        </div>
      </div>
      <div className="side-lyrics-stage" onContextMenu={onContextMenu}>
        <LyricsView
          key={path || "idle"}
          ref={lyricsViewRef}
          className="side-lyrics-body"
          compact
          lines={lines}
          plain={plain}
          synced={synced}
          getTimeMs={getTimeMs}
          onSeekLine={onSeekLine}
          style={lyricsDisplayVars(disp)}
          layoutKey={`${disp.fontScale}|${disp.font}|${disp.lineHeight}`}
          rebuildKey={`${path}|${trackGen}`}
          empty={
            !path ? (
              <div className="np-lyrics-empty tertiary">未在播放</div>
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
      </div>

      {ctxMenu &&
        createPortal(
          <div
            className="np-ctx-menu"
            style={{ left: ctxMenu.x, top: ctxMenu.y }}
            role="menu"
            onClick={(e) => e.stopPropagation()}
          >
            {path && (
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
          </div>,
          document.body,
        )}

      {styleOpen &&
        createPortal(
          <LyricsStyleDialog
            value={disp}
            onChange={patchDisp}
            onClose={() => setStyleOpen(false)}
          />,
          document.body,
        )}
    </div>
  );
}
