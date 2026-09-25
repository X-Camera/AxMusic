import type { SideVizKind, SideVizPalette, SideVizSettings } from "../../lib/types";

export const SIDE_VIZ_KINDS: { id: SideVizKind; label: string; hint: string }[] = [
  { id: "aurora", label: "柔光", hint: "光晕缓慢流动" },
  { id: "spectrum", label: "频谱", hint: "轻量声柱起伏" },
  { id: "particles", label: "粒子", hint: "漂浮光点" },
  { id: "radial-bars", label: "环柱", hint: "居中环形声柱" },
  { id: "radial-line", label: "环线", hint: "居中环形波形" },
];

export const SIDE_VIZ_PALETTES: { id: SideVizPalette; label: string; hint: string }[] = [
  { id: "mono", label: "素雅", hint: "白/灰，几乎不显色" },
  { id: "soft", label: "柔和", hint: "按主色轻微渐变" },
  { id: "vivid", label: "炫酷", hint: "主色 + 对比色渐变" },
];

export const SIDE_VIZ_DEFAULT_COLOR = "#82aaff";

export const SIDE_VIZ_DEFAULT: SideVizSettings = {
  enabled: false,
  kind: "aurora",
  palette: "soft",
  color: SIDE_VIZ_DEFAULT_COLOR,
  intensity: 0.45,
  opacity: 0.42,
  speed: 1,
};

/** 进程内缓存：歌词栏卸载重挂时立刻恢复，不闪默认、不丢已选效果 */
let vizCache: SideVizSettings | null = null;

export function getVizCache(): SideVizSettings | null {
  return vizCache;
}

export function setVizCache(v: SideVizSettings) {
  vizCache = clampViz(v);
}

/** 同一效果的风格预设（只改配色/强度等参数，不切换效果类型） */
export const SIDE_VIZ_STYLES: {
  id: "mono" | "soft" | "vivid";
  label: string;
  patch: Partial<SideVizSettings>;
}[] = [
  {
    id: "mono",
    label: "素雅",
    patch: { palette: "mono", intensity: 0.32, opacity: 0.3, speed: 0.85 },
  },
  {
    id: "soft",
    label: "标准",
    patch: { palette: "soft", intensity: 0.5, opacity: 0.42, speed: 1 },
  },
  {
    id: "vivid",
    label: "炫酷",
    patch: { palette: "vivid", intensity: 0.88, opacity: 0.68, speed: 1.35 },
  },
];

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

export function clampViz(v: SideVizSettings): SideVizSettings {
  // 兼容旧 snake_case / 未知值；识别不出的 kind 回退 aurora
  const raw = String(v.kind ?? "");
  const kind: SideVizKind =
    raw === "radial-bars" || raw === "radial_bars"
      ? "radial-bars"
      : raw === "radial-line" || raw === "radial_line"
        ? "radial-line"
        : raw === "spectrum"
          ? "spectrum"
          : raw === "particles"
            ? "particles"
            : raw === "aurora"
              ? "aurora"
              : "aurora";
  const palette: SideVizPalette =
    v.palette === "mono" || v.palette === "vivid" ? v.palette : "soft";
  return {
    enabled: !!v.enabled,
    kind,
    palette,
    color: normalizeHexColor(v.color),
    intensity: Math.min(1, Math.max(0, Number(v.intensity) || 0)),
    opacity: Math.min(1, Math.max(0, Number(v.opacity) || 0)),
    speed: Math.min(2, Math.max(0.2, Number(v.speed) || 1)),
  };
}

export type Rgb = [number, number, number];

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

function shiftHue(rgb: Rgb, deg: number): Rgb {
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

/** 配色：由主色 + palette 推导 a/b/c 三色 */
export function vizPaletteColors(
  palette: SideVizPalette,
  color = SIDE_VIZ_DEFAULT_COLOR,
): { a: Rgb; b: Rgb; c: Rgb } {
  const base = hexToRgb(color);
  if (palette === "mono") {
    // 素雅：压成中性白灰
    return { a: [235, 238, 245], b: [180, 188, 200], c: [140, 150, 165] };
  }
  if (palette === "vivid") {
    return { a: base, b: shiftHue(base, 70), c: shiftHue(base, -55) };
  }
  // soft：主色 + 轻微色相/明度变体
  return {
    a: base,
    b: shiftHue(base, 18),
    c: shiftHue(base, -14),
  };
}
