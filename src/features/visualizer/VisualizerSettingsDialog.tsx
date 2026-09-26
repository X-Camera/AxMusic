import { useEffect, useRef, useState } from "react";

import type { SideVizKind, SideVizPalette, SideVizSettings } from "../../lib/types";
import {
  normalizeHexColor,
  SIDE_VIZ_KINDS,
  SIDE_VIZ_PALETTES,
  SIDE_VIZ_STYLES,
  VIZ_PARAM_SCHEMA,
} from "./sideViz";

export type VisualizerSettingsValue = SideVizSettings;

const PRESET_COLORS = [
  "#82aaff",
  "#6ec8ff",
  "#5b8cff",
  "#9b7bff",
  "#c44cff",
  "#ff6b9d",
  "#ff8a5c",
  "#ffc857",
  "#5ddea0",
  "#4cc9f0",
  "#e8eef7",
  "#9aa7b8",
];

/** 紧凑取色：圆形色钮 + 预设点 + hex，不用系统大白块 */
function ColorField({
  value,
  disabled,
  onChange,
}: {
  value: string;
  disabled?: boolean;
  onChange: (hex: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [hexDraft, setHexDraft] = useState(value);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => setHexDraft(value), [value]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("pointerdown", onDown);
    return () => window.removeEventListener("pointerdown", onDown);
  }, [open]);

  function commitHex() {
    const next = normalizeHexColor(hexDraft, value);
    onChange(next);
    setHexDraft(next);
    setOpen(false);
  }

  return (
    <div ref={rootRef} className="np-color-field">
      <button
        type="button"
        className="np-color-swatch"
        disabled={disabled}
        aria-label="选择主色"
        aria-expanded={open}
        title="主色"
        onClick={() => setOpen((v) => !v)}
        style={{ background: value }}
      />
      <span className="np-color-hex mono">{value}</span>
      {open && (
        <div className="np-color-pop" role="listbox" aria-label="主色预设">
          <div className="np-color-grid">
            {PRESET_COLORS.map((c) => (
              <button
                key={c}
                type="button"
                className={`np-color-dot${value.toLowerCase() === c ? " on" : ""}`}
                style={{ background: c }}
                title={c}
                aria-label={c}
                onClick={() => {
                  onChange(c);
                  setOpen(false);
                }}
              />
            ))}
          </div>
          <div className="np-color-hex-row">
            <input
              className="np-color-input mono"
              value={hexDraft}
              spellCheck={false}
              aria-label="十六进制颜色"
              onChange={(e) => setHexDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") commitHex();
              }}
            />
            <button type="button" className="np-color-ok" onClick={commitHex}>
              确定
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * 「背景动效」弹窗：效果类型 + 同一效果的风格（素雅/标准/炫酷 = 只改参数）。
 * 可拖动；父级实时预览并落盘。
 */
export function VisualizerSettingsDialog({
  value,
  onChange,
  onClose,
}: {
  value: VisualizerSettingsValue;
  onChange: (next: Partial<VisualizerSettingsValue>) => void;
  onClose: () => void;
}) {
  const panelRef = useRef<HTMLDivElement>(null);
  const titleRef = useRef<HTMLDivElement>(null);
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
      /* ignore */
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

  const pct = (v: number, min: number, max: number) => `${((v - min) / (max - min)) * 100}%`;

  return (
    <div className="np-style-overlay" role="presentation">
      <div
        ref={panelRef}
        className="np-style-panel"
        role="dialog"
        aria-label="背景动效"
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
          背景动效
        </div>

        <div className="np-style-row">
          <span className="np-style-label">开关</span>
          <div className="np-style-ctrl">
            <button
              type="button"
              className={`np-style-toggle ${value.enabled ? "on" : ""}`}
              role="switch"
              aria-checked={value.enabled}
              onClick={() => onChange({ enabled: !value.enabled })}
            >
              {value.enabled ? "开" : "关"}
            </button>
          </div>
        </div>

        <div className="np-style-row">
          <span className="np-style-label">效果</span>
          <div className="np-style-ctrl">
            <select
              className="np-style-select"
              aria-label="效果"
              value={value.kind}
              onChange={(e) => onChange({ kind: e.target.value as SideVizKind })}
            >
              {SIDE_VIZ_KINDS.map((k) => (
                <option key={k.id} value={k.id}>
                  {k.label} — {k.hint}
                </option>
              ))}
            </select>
          </div>
        </div>

        <div className="np-style-row">
          <span className="np-style-label">配色</span>
          <div className="np-style-ctrl">
            <select
              className="np-style-select"
              aria-label="配色"
              value={value.palette}
              disabled={!value.enabled}
              onChange={(e) => onChange({ palette: e.target.value as SideVizPalette })}
            >
              {SIDE_VIZ_PALETTES.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label} — {p.hint}
                </option>
              ))}
            </select>
          </div>
        </div>

        <div className="np-style-row">
          <span className="np-style-label">主色</span>
          <div className="np-style-ctrl np-style-color-row">
            <ColorField
              value={value.color}
              disabled={!value.enabled || value.palette === "mono"}
              onChange={(color) => onChange({ color })}
            />
            {value.palette === "mono" && (
              <span className="np-style-color-hint">素雅下不显色</span>
            )}
            {value.palette === "cover" && (
              <span className="np-style-color-hint">封面取色优先，无封面回退主色</span>
            )}
          </div>
        </div>

        <div className="np-style-row">
          <span className="np-style-label">风格</span>
          <div className="np-style-ctrl np-style-style-row">
            {SIDE_VIZ_STYLES.map((st) => {
              const active =
                value.palette === st.patch.palette &&
                Math.abs(value.intensity - (st.patch.intensity ?? 0)) < 0.06;
              return (
                <button
                  key={st.id}
                  type="button"
                  className={`np-style-preset${active ? " on" : ""}`}
                  disabled={!value.enabled}
                  title={`套用到当前「${SIDE_VIZ_KINDS.find((k) => k.id === value.kind)?.label ?? ""}」`}
                  onClick={() => onChange(st.patch)}
                >
                  {st.label}
                </button>
              );
            })}
          </div>
        </div>

        <div className="np-style-row">
          <span className="np-style-label">强度</span>
          <div className="np-style-ctrl">
            <input
              type="range"
              min={0}
              max={1}
              step={0.05}
              value={value.intensity}
              disabled={!value.enabled}
              aria-label="强度"
              onChange={(e) => onChange({ intensity: Number(e.target.value) })}
              style={{ ["--pct" as string]: pct(value.intensity, 0, 1) }}
            />
            <span className="np-style-val mono">{Math.round(value.intensity * 100)}%</span>
          </div>
        </div>

        <div className="np-style-row">
          <span className="np-style-label">不透明度</span>
          <div className="np-style-ctrl">
            <input
              type="range"
              min={0.05}
              max={1}
              step={0.05}
              value={value.opacity}
              disabled={!value.enabled}
              aria-label="不透明度"
              onChange={(e) => onChange({ opacity: Number(e.target.value) })}
              style={{ ["--pct" as string]: pct(value.opacity, 0.05, 1) }}
            />
            <span className="np-style-val mono">{Math.round(value.opacity * 100)}%</span>
          </div>
        </div>

        <div className="np-style-row">
          <span className="np-style-label">速度</span>
          <div className="np-style-ctrl">
            <input
              type="range"
              min={0.2}
              max={2}
              step={0.05}
              value={value.speed}
              disabled={!value.enabled}
              aria-label="速度"
              onChange={(e) => onChange({ speed: Number(e.target.value) })}
              style={{ ["--pct" as string]: pct(value.speed, 0.2, 2) }}
            />
            <span className="np-style-val mono">{value.speed.toFixed(2)}×</span>
          </div>
        </div>

        {(VIZ_PARAM_SCHEMA[value.kind]?.length ?? 0) > 0 && (
          <>
            <div className="np-style-row">
              <span className="np-style-label">效果参数</span>
              <div className="np-style-ctrl" />
            </div>
            {VIZ_PARAM_SCHEMA[value.kind]!.map((def) => {
              const group = value[def.group] as unknown as Record<string, number | boolean>;
              const v = group[def.key];
              if (def.type === "toggle") {
                return (
                  <div className="np-style-row" key={`${def.group}.${def.key}`}>
                    <span className="np-style-label">{def.label}</span>
                    <div className="np-style-ctrl">
                      <button
                        type="button"
                        className={`np-style-toggle ${v ? "on" : ""}`}
                        role="switch"
                        aria-checked={!!v}
                        disabled={!value.enabled}
                        onClick={() => onChange({ [def.group]: { ...group, [def.key]: !v } })}
                      >
                        {v ? "开" : "关"}
                      </button>
                    </div>
                  </div>
                );
              }
              const num = typeof v === "number" ? v : (def.min ?? 0);
              return (
                <div className="np-style-row" key={`${def.group}.${def.key}`}>
                  <span className="np-style-label">{def.label}</span>
                  <div className="np-style-ctrl">
                    <input
                      type="range"
                      min={def.min ?? 0}
                      max={def.max ?? 1}
                      step={def.step ?? 0.05}
                      value={num}
                      disabled={!value.enabled}
                      aria-label={def.label}
                      onChange={(e) =>
                        onChange({ [def.group]: { ...group, [def.key]: Number(e.target.value) } })
                      }
                      style={{
                        ["--pct" as string]: pct(num, def.min ?? 0, def.max ?? 1),
                      }}
                    />
                    <span className="np-style-val mono">
                      {def.format ? def.format(num) : `${Math.round(num * 100)}%`}
                    </span>
                  </div>
                </div>
              );
            })}
          </>
        )}

        <div className="np-style-row">
          <span className="np-style-label">画质</span>
          <div className="np-style-ctrl np-style-style-row">
            {([0.5, 0.75, 1] as const).map((rs) => (
              <button
                key={rs}
                type="button"
                className={`np-style-preset${value.render_scale === rs ? " on" : ""}`}
                disabled={!value.enabled}
                title={`渲染缩放 ${Math.round(rs * 100)}%（省 GPU）`}
                onClick={() => onChange({ render_scale: rs })}
              >
                {Math.round(rs * 100)}%
              </button>
            ))}
          </div>
        </div>

        <div className="np-style-row">
          <span className="np-style-label">帧率</span>
          <div className="np-style-ctrl np-style-style-row">
            {([30, 60] as const).map((fps) => (
              <button
                key={fps}
                type="button"
                className={`np-style-preset${value.fps_cap === fps ? " on" : ""}`}
                disabled={!value.enabled}
                title={`帧率上限 ${fps}fps`}
                onClick={() => onChange({ fps_cap: fps })}
              >
                {fps}fps
              </button>
            ))}
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
