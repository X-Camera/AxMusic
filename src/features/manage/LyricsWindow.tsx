import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";

import type { LyricsTarget } from "../../lib/types";
import {
  onLyricsTarget,
  takePendingLyricsTarget,
} from "../../lib/lyricsWindow";
import { WindowControls } from "../../components/WindowControls";
import { LyricsPanel } from "./LyricsPanel";
import "../../components/TopBar.css";

/**
 * 「搜索歌词」独立子窗口入口（index.html?win=lyrics）— 无系统标题栏，自绘顶栏。
 * 曲目不进 URL：冷启动从 IPC 取 pending，后续切换走事件。
 */
export function LyricsWindow() {
  const [target, setTarget] = useState<LyricsTarget | null>(null);

  useEffect(() => {
    let alive = true;
    void takePendingLyricsTarget().then((t) => {
      if (alive && t && t.path) setTarget(t);
    });
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => onLyricsTarget((t) => setTarget(t)), []);

  const trackLabel = target
    ? [target.title || target.filename, target.artist]
        .filter(Boolean)
        .join(" — ")
    : "";

  return (
    <div className="lyr-window">
      <header className="lyr-win-bar" data-tauri-drag-region="deep">
        <h1 className="lyr-win-title" data-tauri-drag-region="deep">
          搜索歌词
        </h1>
        <p className="lyr-win-track tertiary" data-tauri-drag-region="deep">
          {trackLabel || "未指定曲目"}
        </p>
        <div className="lyr-win-controls">
          <WindowControls />
        </div>
      </header>

      {target ? (
        <LyricsPanel
          key={`${target.id}:${target.path}`}
          track={target}
          onClose={() => void getCurrentWindow().close()}
          onSaved={() => {
            /* 保存事件由 LyricsPanel 自己广播 */
          }}
        />
      ) : (
        <div className="lyr-window lyr-window-empty">
          <p className="tertiary">未指定曲目</p>
        </div>
      )}
    </div>
  );
}
