import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";

import { WindowControls } from "../../components/WindowControls";
import { api } from "../../lib/api";
import type { AppSettings, SideVizSettings, VizCommonParams } from "../../lib/types";
import {
  clampViz,
  commonOf,
  defaultCommon,
  normalizeHexColor,
  SIDE_VIZ_COLOR_SOURCES,
  SIDE_VIZ_DEFAULT,
  SIDE_VIZ_KINDS,
  SIDE_VIZ_PALETTES,
  themeAccentColor,
  VIZ_PARAM_SCHEMA,
  VIZ_PRESET_COLORS,
} from "./sideViz";
import "./VizSettingsWindow.css";

const pct = (v: number, min: number, max: number) => `${((v - min) / (max - min)) * 100}%`;

/** 通用一行：左标签 + 右控件 */
function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="viz-row">
      <span className="viz-label">{label}</span>
      <div className="viz-ctrl">{children}</div>
    </div>
  );
}

function Toggle({
  on,
  disabled,
  onFlip,
}: {
  on: boolean;
  disabled?: boolean;
  onFlip: () => void;
}) {
  return (
    <button
      type="button"
      className={`viz-toggle${on ? " on" : ""}`}
      role="switch"
      aria-checked={on}
      disabled={disabled}
      onClick={onFlip}
    >
      {on ? "开" : "关"}
    </button>
  );
}

function Slider({
  label,
  min,
  max,
  step,
  value,
  disabled,
  format,
  onChange,
}: {
  label: string;
  min: number;
  max: number;
  step: number;
  value: number;
  disabled?: boolean;
  format: (v: number) => string;
  onChange: (v: number) => void;
}) {
  return (
    <Row label={label}>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        aria-label={label}
        onChange={(e) => onChange(Number(e.target.value))}
        style={{ ["--pct" as string]: pct(value, min, max) }}
      />
      <span className="viz-val mono">{format(value)}</span>
    </Row>
  );
}

/**
 * 主色来源 + 自选色：主题/封面/自选 一类（正交于色彩丰富程度）。
 * 自选/封面（无封面回退）时直排预设点 + hex 输入；素雅不看主色。
 * 读写当前效果的公共参数副本。
 */
function ColorSection({
  common,
  onPatch,
}: {
  common: VizCommonParams;
  onPatch: (p: Partial<VizCommonParams>) => void;
}) {
  const [hexDraft, setHexDraft] = useState(common.color);
  useEffect(() => setHexDraft(common.color), [common.color]);

  if (common.palette === "mono") {
    return (
      <Row label="主色">
        <span className="viz-hint">素雅为纯黑白灰，无需选色</span>
      </Row>
    );
  }

  const src = common.color_source;

  function commitHex() {
    const next = normalizeHexColor(hexDraft, common.color);
    setHexDraft(next);
    onPatch({ color: next });
  }

  return (
    <>
      <Row label="主色">
        <div className="viz-chips">
          {SIDE_VIZ_COLOR_SOURCES.map((o) => (
            <button
              key={o.id}
              type="button"
              className={`viz-chip${src === o.id ? " on" : ""}`}
              title={o.hint}
              onClick={() => onPatch({ color_source: o.id })}
            >
              {o.label}
            </button>
          ))}
        </div>
      </Row>
      {src === "theme" && (
        <Row label="主题色">
          <span
            className="viz-dot on"
            style={{ background: themeAccentColor() }}
            title={themeAccentColor()}
          />
          <span className="viz-hint">跟随主题强调色（{themeAccentColor()}）</span>
        </Row>
      )}
      {src === "cover" && (
        <Row label="封面色">
          <span className="viz-hint">取当前曲目封面色，无封面回退下方自选色</span>
        </Row>
      )}
      {src !== "theme" && (
        <>
          <Row label="自选色">
            <div className="viz-dots" role="listbox" aria-label="主色预设">
              {VIZ_PRESET_COLORS.map((c) => (
                <button
                  key={c}
                  type="button"
                  className={`viz-dot${common.color.toLowerCase() === c ? " on" : ""}`}
                  style={{ background: c }}
                  title={c}
                  aria-label={c}
                  onClick={() => onPatch({ color: c })}
                />
              ))}
            </div>
          </Row>
          <Row label="自定义">
            <input
              className="viz-hex-input mono"
              value={hexDraft}
              spellCheck={false}
              aria-label="十六进制颜色"
              onChange={(e) => setHexDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") commitHex();
              }}
            />
            <button type="button" className="viz-chip" onClick={commitHex}>
              确定
            </button>
          </Row>
        </>
      )}
    </>
  );
}

/**
 * 「背景动效」独立设置子窗口（index.html?win=viz-settings）：
 * 效果标签页直达 + 公共参数 / 效果私有参数分区；主题跟随（token），可拖出主窗口。
 * 改动经 updateSettings 落盘并广播 settings://changed，主窗口歌词区实时预览。
 */
export function VizSettingsWindow() {
  const [viz, setViz] = useState<SideVizSettings | null>(null);

  useEffect(() => {
    let cancelled = false;
    void api.getSettings().then((s) => {
      if (!cancelled) setViz(clampViz(s.side_viz ?? SIDE_VIZ_DEFAULT));
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // 主窗口侧改动（浮动开关切效果/关）同步进来；自身 patch 的回声是等值重放，幂等
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listen<AppSettings>("settings://changed", (e) => {
      if (!cancelled && e.payload.side_viz) setViz(clampViz(e.payload.side_viz));
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  function patch(p: Partial<SideVizSettings>) {
    setViz((v) => {
      if (!v) return v;
      const merged = clampViz({ ...v, ...p });
      void api.updateSettings({ side_viz: merged }).catch(() => void 0);
      return merged;
    });
  }

  /** 公共参数写当前效果的独立副本 */
  function patchCommon(p: Partial<VizCommonParams>) {
    setViz((v) => {
      if (!v) return v;
      const merged = clampViz({
        ...v,
        commons: { ...v.commons, [v.kind]: { ...commonOf(v), ...p } },
      });
      void api.updateSettings({ side_viz: merged }).catch(() => void 0);
      return merged;
    });
  }

  /** 恢复当前效果的公共参数 + 私有参数为默认 */
  function resetCurrent() {
    setViz((v) => {
      if (!v) return v;
      const next: SideVizSettings = {
        ...v,
        commons: { ...v.commons, [v.kind]: defaultCommon() },
      };
      for (const g of new Set((VIZ_PARAM_SCHEMA[v.kind] ?? []).map((d) => d.group))) {
        (next as unknown as Record<string, unknown>)[g] = SIDE_VIZ_DEFAULT[g];
      }
      const merged = clampViz(next);
      void api.updateSettings({ side_viz: merged }).catch(() => void 0);
      return merged;
    });
  }

  if (!viz) {
    return (
      <div className="viz-win">
        <header className="viz-win-bar" data-tauri-drag-region="deep">
          <h1 className="viz-win-title" data-tauri-drag-region="deep">
            背景动效
          </h1>
          <div className="viz-win-controls">
            <WindowControls onlyClose />
          </div>
        </header>
        <div className="viz-body">
          <p className="tertiary">加载中…</p>
        </div>
      </div>
    );
  }

  const off = !viz.enabled;
  const common = commonOf(viz);
  const schema = VIZ_PARAM_SCHEMA[viz.kind] ?? [];
  const kindLabel = SIDE_VIZ_KINDS.find((k) => k.id === viz.kind)?.label ?? "";

  return (
    <div className="viz-win">
      <header className="viz-win-bar" data-tauri-drag-region="deep">
        <h1 className="viz-win-title" data-tauri-drag-region="deep">
          背景动效
        </h1>
        <div className="viz-win-controls">
          <WindowControls onlyClose />
        </div>
      </header>

      {/* 效果标签页：点一下直达；「无」= 关闭动效（隐藏全部参数） */}
      <nav className="viz-tabs" role="tablist" aria-label="效果">
        <button
          type="button"
          role="tab"
          aria-selected={off}
          className={`viz-tab${off ? " on" : ""}`}
          title="关闭背景动效"
          onClick={() => patch({ enabled: false })}
        >
          无
        </button>
        {SIDE_VIZ_KINDS.map((k) => (
          <button
            key={k.id}
            type="button"
            role="tab"
            aria-selected={viz.enabled && viz.kind === k.id}
            className={`viz-tab${viz.enabled && viz.kind === k.id ? " on" : ""}`}
            title={k.hint}
            onClick={() => patch({ kind: k.id, enabled: true })}
          >
            {k.label}
          </button>
        ))}
      </nav>

      <div className="viz-body">
        {off ? (
          <p className="viz-off-hint tertiary">
            未开启动效——上方选择效果即开启；每个效果的参数独立保存
          </p>
        ) : (
          <>
            <section className="viz-sec">
              <h2 className="viz-sec-title">公共（{kindLabel}）</h2>

              <Row label="色彩">
                <div className="viz-chips">
                  {SIDE_VIZ_PALETTES.map((p) => (
                    <button
                      key={p.id}
                      type="button"
                      className={`viz-chip${common.palette === p.id ? " on" : ""}`}
                      title={p.hint}
                      onClick={() => patchCommon({ palette: p.id })}
                    >
                      {p.label}
                    </button>
                  ))}
                </div>
              </Row>

              <ColorSection common={common} onPatch={patchCommon} />

              <Slider
                label="强度"
                min={0}
                max={1}
                step={0.05}
                value={common.intensity}
                format={(v) => `${Math.round(v * 100)}%`}
                onChange={(intensity) => patchCommon({ intensity })}
              />
              <Slider
                label="不透明度"
                min={0.05}
                max={1}
                step={0.05}
                value={common.opacity}
                format={(v) => `${Math.round(v * 100)}%`}
                onChange={(opacity) => patchCommon({ opacity })}
              />
              <Slider
                label="速度"
                min={0.2}
                max={2}
                step={0.05}
                value={common.speed}
                format={(v) => `${v.toFixed(2)}×`}
                onChange={(speed) => patchCommon({ speed })}
              />

              <Row label="画质">
                <div className="viz-chips">
                  {([0.5, 0.75, 1] as const).map((rs) => (
                    <button
                      key={rs}
                      type="button"
                      className={`viz-chip${common.render_scale === rs ? " on" : ""}`}
                      title={`渲染缩放 ${Math.round(rs * 100)}%（省 GPU）`}
                      onClick={() => patchCommon({ render_scale: rs })}
                    >
                      {Math.round(rs * 100)}%
                    </button>
                  ))}
                </div>
              </Row>

              <Row label="帧率">
                <div className="viz-chips">
                  {([30, 60] as const).map((fps) => (
                    <button
                      key={fps}
                      type="button"
                      className={`viz-chip${common.fps_cap === fps ? " on" : ""}`}
                      title={`帧率上限 ${fps}fps`}
                      onClick={() => patchCommon({ fps_cap: fps })}
                    >
                      {fps}fps
                    </button>
                  ))}
                </div>
              </Row>
            </section>

            {schema.length > 0 && (
              <section className="viz-sec">
                <h2 className="viz-sec-title">{kindLabel}参数</h2>
                {schema.map((def) => {
                  const group = viz[def.group] as unknown as Record<string, number | boolean>;
                  const v = group[def.key];
                  if (def.type === "toggle") {
                    return (
                      <Row key={`${def.group}.${def.key}`} label={def.label}>
                        <Toggle
                          on={!!v}
                          onFlip={() => patch({ [def.group]: { ...group, [def.key]: !v } })}
                        />
                      </Row>
                    );
                  }
                  const num = typeof v === "number" ? v : (def.min ?? 0);
                  return (
                    <Slider
                      key={`${def.group}.${def.key}`}
                      label={def.label}
                      min={def.min ?? 0}
                      max={def.max ?? 1}
                      step={def.step ?? 0.05}
                      value={num}
                      format={def.format ?? ((x) => `${Math.round(x * 100)}%`)}
                      onChange={(nv) => patch({ [def.group]: { ...group, [def.key]: nv } })}
                    />
                  );
                })}
              </section>
            )}
          </>
        )}
      </div>

      {/* 底栏固定，不随内容多少浮动 */}
      <footer className="viz-foot">
        <button
          type="button"
          className="viz-reset"
          disabled={off}
          title="恢复当前效果（含公共参数）为默认值"
          onClick={resetCurrent}
        >
          恢复默认
        </button>
        <button
          type="button"
          className="viz-done"
          onClick={() => void getCurrentWindow().close()}
        >
          完成
        </button>
      </footer>
    </div>
  );
}
