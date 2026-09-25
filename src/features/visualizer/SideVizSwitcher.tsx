import { useEffect, useRef, useState } from "react";

import type { SideVizKind, SideVizSettings } from "../../lib/types";
import { SIDE_VIZ_KINDS } from "./sideViz";

/** 效果图标（内联 SVG，随主题取色） */
function KindIcon({ kind }: { kind: SideVizKind | "off" | "idle" }) {
  if (kind === "idle") {
    // 未开启：低矮均衡器，表示「动效入口」而非关闭
    return (
      <svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true">
        <rect x="2.5" y="9" width="2" height="3" rx="1" fill="currentColor" opacity="0.55" />
        <rect x="6" y="7.5" width="2" height="4.5" rx="1" fill="currentColor" opacity="0.7" />
        <rect x="9.5" y="8.2" width="2" height="3.8" rx="1" fill="currentColor" opacity="0.55" />
        <rect x="13" y="7" width="1.5" height="5" rx="0.75" fill="currentColor" opacity="0.7" />
      </svg>
    );
  }
  if (kind === "off") {
    return (
      <svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true">
        <path
          d="M4 4l8 8M12 4l-8 8"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
        />
      </svg>
    );
  }
  if (kind === "aurora") {
    return (
      <svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true">
        <path
          d="M2 10c2-3 4-3 6 0s4 3 6 0"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          fill="none"
        />
        <path
          d="M2 6c2-3 4-3 6 0s4 3 6 0"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          fill="none"
          opacity="0.5"
        />
      </svg>
    );
  }
  if (kind === "spectrum") {
    return (
      <svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true">
        <rect x="2" y="7" width="2" height="5" rx="1" fill="currentColor" opacity="0.55" />
        <rect x="5.5" y="4" width="2" height="8" rx="1" fill="currentColor" />
        <rect x="9" y="6" width="2" height="6" rx="1" fill="currentColor" opacity="0.8" />
        <rect x="12.5" y="3" width="2" height="9" rx="1" fill="currentColor" />
      </svg>
    );
  }
  return (
    <svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="4" cy="5" r="1.4" fill="currentColor" opacity="0.55" />
      <circle cx="9" cy="3.5" r="1.2" fill="currentColor" />
      <circle cx="12.5" cy="7" r="1.5" fill="currentColor" opacity="0.85" />
      <circle cx="5.5" cy="11" r="1.3" fill="currentColor" />
      <circle cx="11" cy="12" r="1.1" fill="currentColor" opacity="0.7" />
    </svg>
  );
}

/**
 * 歌词区内浮层开关：默认一颗图标，点开一排效果切换 / 关闭。
 * 点外部收起；双击主按钮打开详细调参弹窗。
 */
export function SideVizSwitcher({
  value,
  onChange,
  onOpenSettings,
}: {
  value: SideVizSettings;
  onChange: (next: Partial<SideVizSettings>) => void;
  onOpenSettings: () => void;
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("pointerdown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div ref={rootRef} className={`side-viz-switcher${open ? " open" : ""}`}>
      {!open ? (
        <button
          type="button"
          className={`side-viz-fab${value.enabled ? " on" : ""}`}
          title={value.enabled ? "背景动效（点开切换）" : "背景动效（未开启，点开选择）"}
          aria-label="背景动效"
          aria-expanded={false}
          onClick={() => setOpen(true)}
          onDoubleClick={onOpenSettings}
        >
          <KindIcon kind={value.enabled ? value.kind : "idle"} />
        </button>
      ) : (
        <div className="side-viz-row" role="toolbar" aria-label="背景动效">
          <button
            type="button"
            className={`side-viz-chip${!value.enabled ? " on" : ""}`}
            title="关闭效果"
            aria-label="关闭效果"
            onClick={() => {
              onChange({ enabled: false });
              setOpen(false);
            }}
          >
            <KindIcon kind="off" />
          </button>
          {SIDE_VIZ_KINDS.map((k) => (
            <button
              key={k.id}
              type="button"
              className={`side-viz-chip${value.enabled && value.kind === k.id ? " on" : ""}`}
              title={k.label}
              aria-label={k.label}
              aria-pressed={value.enabled && value.kind === k.id}
              onClick={() => onChange({ enabled: true, kind: k.id })}
            >
              <KindIcon kind={k.id} />
            </button>
          ))}
          <button
            type="button"
            className="side-viz-chip side-viz-tune"
            title="详细参数"
            aria-label="详细参数"
            onClick={() => {
              setOpen(false);
              onOpenSettings();
            }}
          >
            <svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true">
              <path
                d="M3 4.5h10M3 8h10M3 11.5h10"
                stroke="currentColor"
                strokeWidth="1.4"
                strokeLinecap="round"
              />
              <circle cx="6" cy="4.5" r="1.4" fill="currentColor" />
              <circle cx="10.5" cy="8" r="1.4" fill="currentColor" />
              <circle cx="5" cy="11.5" r="1.4" fill="currentColor" />
            </svg>
          </button>
        </div>
      )}
    </div>
  );
}
