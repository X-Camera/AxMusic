import type {
  SideVizColorSource,
  SideVizKind,
  SideVizPalette,
  SideVizSettings,
  VizCommonParams,
} from "../../lib/types";

export const SIDE_VIZ_KINDS: { id: SideVizKind; label: string; hint: string }[] = [
  { id: "fluid", label: "流体", hint: "封面晕染流转" },
  { id: "silk", label: "丝绸", hint: "极光流光" },
  { id: "aurora", label: "柔光", hint: "光晕缓慢流动" },
  { id: "spectrum", label: "频谱", hint: "声柱随音乐起伏" },
  { id: "particles", label: "粒子", hint: "漂浮光点连线" },
  { id: "radial-bars", label: "环柱", hint: "居中环形声柱" },
  { id: "radial-line", label: "环线", hint: "居中环形波形" },
];

/** 色彩丰富程度（与主色来源正交） */
export const SIDE_VIZ_PALETTES: { id: SideVizPalette; label: string; hint: string }[] = [
  { id: "mono", label: "素雅", hint: "纯黑白灰，不显色" },
  { id: "soft", label: "柔和", hint: "主色单色轻微渐变" },
  { id: "vivid", label: "炫酷", hint: "主色 + 对比色双色渐变" },
];

/** 主色来源（与色彩丰富程度正交） */
export const SIDE_VIZ_COLOR_SOURCES: { id: SideVizColorSource; label: string; hint: string }[] = [
  { id: "theme", label: "主题", hint: "跟随当前主题强调色" },
  { id: "cover", label: "封面", hint: "取当前曲目封面色" },
  { id: "custom", label: "自选", hint: "使用下方选定颜色" },
];

/** 主色预设点（面板直排，单击即设） */
export const VIZ_PRESET_COLORS = [
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

export const SIDE_VIZ_DEFAULT_COLOR = "#82aaff";

/** 单个效果的公共参数默认值 */
export function defaultCommon(): VizCommonParams {
  return {
    palette: "soft",
    color_source: "custom",
    color: SIDE_VIZ_DEFAULT_COLOR,
    intensity: 0.45,
    opacity: 0.42,
    speed: 1,
    render_scale: 1,
    fps_cap: 60,
  };
}

function defaultCommons(): Record<SideVizKind, VizCommonParams> {
  return {
    aurora: defaultCommon(),
    spectrum: defaultCommon(),
    particles: defaultCommon(),
    "radial-bars": defaultCommon(),
    "radial-line": defaultCommon(),
    fluid: defaultCommon(),
    silk: defaultCommon(),
  };
}

/** 当前效果的公共参数（缺字段/旧配置时回退默认） */
export function commonOf(s: SideVizSettings): VizCommonParams {
  return s.commons?.[s.kind] ?? defaultCommon();
}

export const SIDE_VIZ_DEFAULT: SideVizSettings = {
  enabled: false,
  kind: "aurora",
  commons: defaultCommons(),
  fluid: { blur: 0.5, breathe: 0.5, spin: 0.5 },
  silk: { flow: 0.5, complexity: 0.5, brightness: 0.5 },
  spectrum_ex: { bars: 64, glow: 0.5, peaks: true, mirror: false },
  particles_ex: { count: 56, links: true, link_dist: 0.5, size: 0.5 },
  radial_ex: { radius: 0.5, out_len: 0.5, in_len: 0.5, emit: 40, sensitivity: 0.5 },
};

/** 每效果专属参数 schema：弹窗按当前 kind 动态渲染（slider/toggle） */
export interface VizParamDef {
  group: "fluid" | "silk" | "spectrum_ex" | "particles_ex" | "radial_ex";
  key: string;
  label: string;
  type: "slider" | "toggle";
  min?: number;
  max?: number;
  step?: number;
  format?: (v: number) => string;
}

export const VIZ_PARAM_SCHEMA: Partial<Record<SideVizKind, VizParamDef[]>> = {
  fluid: [
    { group: "fluid", key: "blur", label: "模糊", type: "slider", min: 0, max: 1, step: 0.05 },
    { group: "fluid", key: "breathe", label: "呼吸", type: "slider", min: 0, max: 1, step: 0.05 },
    { group: "fluid", key: "spin", label: "流转", type: "slider", min: 0, max: 1, step: 0.05 },
  ],
  silk: [
    { group: "silk", key: "flow", label: "流速", type: "slider", min: 0, max: 1, step: 0.05 },
    { group: "silk", key: "complexity", label: "层次", type: "slider", min: 0, max: 1, step: 0.05 },
    { group: "silk", key: "brightness", label: "亮度", type: "slider", min: 0, max: 1, step: 0.05 },
  ],
  spectrum: [
    {
      group: "spectrum_ex",
      key: "bars",
      label: "柱数",
      type: "slider",
      min: 16,
      max: 96,
      step: 16,
      format: (v) => `${v}`,
    },
    { group: "spectrum_ex", key: "glow", label: "发光", type: "slider", min: 0, max: 1, step: 0.05 },
    { group: "spectrum_ex", key: "peaks", label: "峰值点", type: "toggle" },
    { group: "spectrum_ex", key: "mirror", label: "镜像", type: "toggle" },
  ],
  particles: [
    {
      group: "particles_ex",
      key: "count",
      label: "数量",
      type: "slider",
      min: 8,
      max: 160,
      step: 8,
      format: (v) => `${v}`,
    },
    { group: "particles_ex", key: "size", label: "大小", type: "slider", min: 0, max: 1, step: 0.05 },
    { group: "particles_ex", key: "links", label: "连线", type: "toggle" },
    {
      group: "particles_ex",
      key: "link_dist",
      label: "线距",
      type: "slider",
      min: 0,
      max: 1,
      step: 0.05,
    },
  ],
  "radial-bars": [
    {
      group: "radial_ex",
      key: "radius",
      label: "主环大小",
      type: "slider",
      min: 0,
      max: 1,
      step: 0.05,
    },
    {
      group: "radial_ex",
      key: "out_len",
      label: "外伸长度",
      type: "slider",
      min: 0,
      max: 1,
      step: 0.05,
    },
    {
      group: "radial_ex",
      key: "in_len",
      label: "内伸长度",
      type: "slider",
      min: 0,
      max: 1,
      step: 0.05,
    },
  ],
  "radial-line": [
    {
      group: "radial_ex",
      key: "radius",
      label: "主环大小",
      type: "slider",
      min: 0,
      max: 1,
      step: 0.05,
    },
    {
      group: "radial_ex",
      key: "out_len",
      label: "波形幅度",
      type: "slider",
      min: 0,
      max: 1,
      step: 0.05,
    },
    {
      group: "radial_ex",
      key: "emit",
      label: "粒子数量",
      type: "slider",
      min: 0,
      max: 200,
      step: 10,
      format: (v) => (v === 0 ? "关" : `${v}`),
    },
    {
      group: "radial_ex",
      key: "sensitivity",
      label: "触发灵敏",
      type: "slider",
      min: 0,
      max: 1,
      step: 0.05,
    },
  ],
};

/** 进程内缓存：歌词栏卸载重挂时立刻恢复，不闪默认、不丢已选效果 */
let vizCache: SideVizSettings | null = null;

export function getVizCache(): SideVizSettings | null {
  return vizCache;
}

export function setVizCache(v: SideVizSettings) {
  vizCache = clampViz(v);
}

export function normalizeHexColor(input: string, fallback = SIDE_VIZ_DEFAULT_COLOR): string {
  let s = (input || "").trim().toLowerCase();
  if (!s.startsWith("#")) s = `#${s}`;
  const body = s.slice(1);
  if (/^[0-9a-f]{3}$/.test(body)) {
    return `#${body[0]}${body[0]}${body[1]}${body[1]}${body[2]}${body[2]}`;
  }
  if (/^[0-9a-f]{6}$/.test(body)) return `#${body}`;
  return fallback;
}

const KIND_WHITELIST: SideVizKind[] = [
  "aurora",
  "spectrum",
  "particles",
  "radial-bars",
  "radial-line",
  "fluid",
  "silk",
];

function clamp01(n: unknown, fallback = 0): number {
  const v = Number(n);
  if (!Number.isFinite(v)) return fallback;
  return Math.min(1, Math.max(0, v));
}

function clampInt(n: unknown, min: number, max: number, fallback: number): number {
  const v = Math.round(Number(n));
  if (!Number.isFinite(v)) return fallback;
  return Math.min(max, Math.max(min, v));
}

/** 嵌套参数组与默认值深合并（旧 settings.json 没有这些字段） */
function mergeParams<T extends object>(defaults: T, raw: unknown): T {
  const out = { ...defaults };
  if (!raw || typeof raw !== "object") return out;
  const src = raw as Record<string, unknown>;
  const defRec = defaults as Record<string, number | boolean>;
  const outMut = out as Record<string, number | boolean>;
  for (const k of Object.keys(defRec)) {
    const v = src[k];
    if (typeof defRec[k] === "boolean") {
      if (typeof v === "boolean") outMut[k] = v;
    } else if (typeof v === "number" && Number.isFinite(v)) {
      outMut[k] = v;
    }
  }
  return out;
}

function clampCommon(raw: unknown): VizCommonParams {
  const r = (raw ?? {}) as Partial<VizCommonParams>;
  const palette: SideVizPalette =
    r.palette === "mono" || r.palette === "vivid" ? r.palette : "soft";
  const color_source: SideVizColorSource =
    r.color_source === "theme" || r.color_source === "cover" ? r.color_source : "custom";
  const scaleRaw = Number(r.render_scale);
  return {
    palette,
    color_source,
    color: normalizeHexColor(r.color ?? SIDE_VIZ_DEFAULT_COLOR),
    intensity: clamp01(r.intensity, 0.45),
    opacity: clamp01(r.opacity, 0.42),
    speed: Math.min(2, Math.max(0.2, Number(r.speed) || 1)),
    render_scale: scaleRaw <= 0.62 ? 0.5 : scaleRaw <= 0.87 ? 0.75 : 1,
    fps_cap: Number(r.fps_cap) <= 45 ? 30 : 60,
  };
}

export function clampViz(v: SideVizSettings): SideVizSettings {
  // 兼容旧 snake_case / 未知值；识别不出的 kind 回退 aurora
  const raw = String(v.kind ?? "").replace(/_/g, "-") as SideVizKind;
  const kind: SideVizKind = KIND_WHITELIST.includes(raw) ? raw : "aurora";
  const d = SIDE_VIZ_DEFAULT;
  const rawCommons = (v.commons ?? {}) as Partial<Record<SideVizKind, unknown>>;
  const commons = {} as Record<SideVizKind, VizCommonParams>;
  for (const k of KIND_WHITELIST) commons[k] = clampCommon(rawCommons[k]);
  // 旧配置兜底迁移：顶层散落的公共参数 → 当前 kind（Rust 侧已迁，这里兜进程缓存）
  if (!v.commons) {
    commons[kind] = clampCommon(v as unknown as Partial<VizCommonParams>);
  }
  return {
    enabled: !!v.enabled,
    kind,
    commons,
    fluid: (() => {
      const p = mergeParams(d.fluid, v.fluid);
      return { blur: clamp01(p.blur, 0.5), breathe: clamp01(p.breathe, 0.5), spin: clamp01(p.spin, 0.5) };
    })(),
    silk: (() => {
      const p = mergeParams(d.silk, v.silk);
      return {
        flow: clamp01(p.flow, 0.5),
        complexity: clamp01(p.complexity, 0.5),
        brightness: clamp01(p.brightness, 0.5),
      };
    })(),
    spectrum_ex: (() => {
      const p = mergeParams(d.spectrum_ex, v.spectrum_ex);
      return {
        bars: clampInt(p.bars, 16, 128, 64),
        glow: clamp01(p.glow, 0.5),
        peaks: p.peaks,
        mirror: p.mirror,
      };
    })(),
    particles_ex: (() => {
      const p = mergeParams(d.particles_ex, v.particles_ex);
      return {
        count: clampInt(p.count, 8, 160, 56),
        links: p.links,
        link_dist: clamp01(p.link_dist, 0.5),
        size: clamp01(p.size, 0.5),
      };
    })(),
    radial_ex: (() => {
      const p = mergeParams(d.radial_ex, v.radial_ex);
      return {
        radius: clamp01(p.radius, 0.5),
        out_len: clamp01(p.out_len, 0.5),
        in_len: clamp01(p.in_len, 0.5),
        emit: clampInt(p.emit, 0, 200, 40),
        sensitivity: clamp01(p.sensitivity, 0.5),
      };
    })(),
  };
}

export type Rgb = [number, number, number];

export type VizColors = { a: Rgb; b: Rgb; c: Rgb };

export function hexToRgb(hex: string): Rgb {
  const h = normalizeHexColor(hex).slice(1);
  return [
    parseInt(h.slice(0, 2), 16),
    parseInt(h.slice(2, 4), 16),
    parseInt(h.slice(4, 6), 16),
  ];
}

function clamp255(n: number): number {
  return Math.max(0, Math.min(255, Math.round(n)));
}

export function lerpRgb(a: Rgb, b: Rgb, t: number): Rgb {
  return [
    clamp255(a[0] + (b[0] - a[0]) * t),
    clamp255(a[1] + (b[1] - a[1]) * t),
    clamp255(a[2] + (b[2] - a[2]) * t),
  ];
}

/** 亮主题用：保色相、压明度——白底上浅色/白热不可见，压成「墨」色才有对比 */
export function inkify(c: Rgb, k = 0.5): Rgb {
  return lerpRgb(c, [16, 20, 28], k);
}

export function shiftHue(rgb: Rgb, deg: number): Rgb {
  // 近似：在 RGB 上做色相旋转（够视觉用，无需完整 HSL 往返）
  const [r, g, b] = rgb.map((v) => v / 255) as [number, number, number];
  const angle = ((deg % 360) + 360) % 360;
  const cos = Math.cos((angle * Math.PI) / 180);
  const sin = Math.sin((angle * Math.PI) / 180);
  const m = [
    0.213 + cos * 0.787 - sin * 0.213,
    0.715 - cos * 0.715 - sin * 0.715,
    0.072 - cos * 0.072 + sin * 0.928,
    0.213 - cos * 0.213 + sin * 0.143,
    0.715 + cos * 0.285 + sin * 0.14,
    0.072 - cos * 0.072 - sin * 0.283,
    0.213 - cos * 0.213 - sin * 0.787,
    0.715 - cos * 0.715 + sin * 0.715,
    0.072 + cos * 0.928 + sin * 0.072,
  ];
  return [
    clamp255((m[0] * r + m[1] * g + m[2] * b) * 255),
    clamp255((m[3] * r + m[4] * g + m[5] * b) * 255),
    clamp255((m[6] * r + m[7] * g + m[8] * b) * 255),
  ];
}

/**
 * source === "theme" 的主色：当前主题的 --accent（CSS 变量）。
 * 按 主题×皮肤 缓存，切主题/皮肤自动失效重读。
 */
let themeAccentCache: { key: string; hex: string } | null = null;

export function themeAccentColor(fallback = SIDE_VIZ_DEFAULT_COLOR): string {
  const el = document.documentElement;
  const key = `${el.dataset.theme ?? ""}|${el.dataset.colorScheme ?? ""}`;
  if (themeAccentCache?.key === key) return themeAccentCache.hex;
  const raw = getComputedStyle(el).getPropertyValue("--accent").trim();
  const hex = raw ? normalizeHexColor(raw, fallback) : fallback;
  themeAccentCache = { key, hex };
  return hex;
}

/**
 * 配色推导：palette（丰富程度）× color_source（主色来源）两个正交维度。
 * - mono：纯黑白灰，不看主色
 * - soft：基色 + 轻微色相变体（单色渐变）
 * - vivid：基色 + 对比色（双色渐变）；封面来源时直接用封面提取三色
 * 基色解析：theme → 主题强调色；cover → 封面主色（无封面回退 color）；custom → color
 */
export function vizPaletteColors(
  palette: SideVizPalette,
  source: SideVizColorSource,
  color = SIDE_VIZ_DEFAULT_COLOR,
  coverColors?: VizColors | null,
): VizColors {
  if (palette === "mono") {
    // 素雅：压成中性白灰
    return { a: [235, 238, 245], b: [180, 188, 200], c: [140, 150, 165] };
  }
  if (source === "cover" && coverColors) {
    if (palette === "vivid") return coverColors;
    const base = coverColors.a;
    return { a: base, b: shiftHue(base, 18), c: shiftHue(base, -14) };
  }
  const base = hexToRgb(source === "theme" ? themeAccentColor(color) : color);
  if (palette === "vivid") {
    return { a: base, b: shiftHue(base, 70), c: shiftHue(base, -55) };
  }
  return {
    a: base,
    b: shiftHue(base, 18),
    c: shiftHue(base, -14),
  };
}
