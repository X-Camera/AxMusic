import { useEffect, useRef, useState } from "react";

import { LYRICS_FONTS } from "../../lib/lyricsDisplay";
import type { LyricsFont } from "../../lib/types";

export interface LyricsDisplayStyle {
  fontScale: number;
  font: LyricsFont;
  lineHeight: number;
}

/** 满窗右键「歌词样式」弹窗：调字号/字体/行距，可拖动；父级实时套到歌词并落盘 */
export function LyricsStyleDialog({
  value,
  onChange,
  onClose,
}: {
  value: LyricsDisplayStyle;
  onChange: (next: Partial<LyricsDisplayStyle>) => void;
  onClose: () => void;
}) {
  const panelRef = useRef<HTMLDivElement>(null);
  const titleRef = useRef<HTMLDivElement>(null);
  /** 相对初始居中位置的拖拽偏移 */
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const dragRef = useRef<{
    pointerId: number;
    startX: number;
    startY: number;
    origX: number;
    origY: number;
  } | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    const onDown = (e: PointerEvent) => {
      if (panelRef.current && !panelRef.current.contains(e.target as Node)) onClose();
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onDown);
    };
  }, [onClose]);

  /** 标题栏拖动（指针捕获，拖出面板仍跟手） */
  function onTitlePointerDown(e: React.PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    dragRef.current = {
      pointerId: e.pointerId,
      startX: e.clientX,
      startY: e.clientY,
      origX: offset.x,
      origY: offset.y,
    };
    try {
      titleRef.current?.setPointerCapture(e.pointerId);
    } catch {
      /* 指针已释放等极端情况，忽略 */
    }
  }

  function onTitlePointerMove(e: React.PointerEvent) {
    const d = dragRef.current;
    if (!d || d.pointerId !== e.pointerId) return;
    setOffset({
      x: d.origX + (e.clientX - d.startX),
      y: d.origY + (e.clientY - d.startY),
    });
  }

  function onTitlePointerUp(e: React.PointerEvent) {
    const d = dragRef.current;
    if (!d || d.pointerId !== e.pointerId) return;
    dragRef.current = null;
    try {
      titleRef.current?.releasePointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }
  }

  return (
    <div className="np-style-overlay" role="presentation">
      <div
        ref={panelRef}
        className="np-style-panel"
        role="dialog"
        aria-label="歌词样式"
        style={{ transform: `translate(${offset.x}px, ${offset.y}px)` }}
        onClick={(e) => e.stopPropagation()}
      >
        <div
          ref={titleRef}
          className="np-style-title"
          onPointerDown={onTitlePointerDown}
          onPointerMove={onTitlePointerMove}
          onPointerUp={onTitlePointerUp}
          onPointerCancel={onTitlePointerUp}
        >
          歌词样式
        </div>

        <div className="np-style-row">
          <span className="np-style-label">字号</span>
          <div className="np-style-ctrl">
            <input
              type="range"
              min={0.75}
              max={1.5}
              step={0.05}
              value={value.fontScale}
              aria-label="字号"
              onChange={(e) => onChange({ fontScale: Number(e.target.value) })}
              style={{
                ["--pct" as string]: `${((value.fontScale - 0.75) / 0.75) * 100}%`,
              }}
            />
            <span className="np-style-val mono">{Math.round(value.fontScale * 100)}%</span>
          </div>
        </div>

        <div className="np-style-row">
          <span className="np-style-label">字体</span>
          <div className="np-style-ctrl">
            <select
              className="np-style-select"
              aria-label="字体"
              value={value.font}
              onChange={(e) => onChange({ font: e.target.value as LyricsFont })}
            >
              {LYRICS_FONTS.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.label}
                </option>
              ))}
            </select>
          </div>
        </div>

        <div className="np-style-row">
          <span className="np-style-label">行距</span>
          <div className="np-style-ctrl">
            <input
              type="range"
              min={1}
              max={2}
              step={0.05}
              value={value.lineHeight}
              aria-label="行距"
              onChange={(e) => onChange({ lineHeight: Number(e.target.value) })}
              style={{
                ["--pct" as string]: `${((value.lineHeight - 1) / 1) * 100}%`,
              }}
            />
            <span className="np-style-val mono">{value.lineHeight.toFixed(2)}</span>
          </div>
        </div>

        <div className="np-style-actions">
          <button type="button" className="np-style-done" onClick={onClose}>
            完成
          </button>
        </div>
      </div>
    </div>
  );
}
