import type { SideVizSettings } from "../../../lib/types";
import type { VizColors } from "../sideViz";

/** 每帧分发给效果的统一数据包（宿主构建一次，对象复用避免 GC） */
export interface VizFrame {
  /** 2D 上下文；silk（WebGL）帧里为 null */
  ctx: CanvasRenderingContext2D | null;
  /** 舞台 CSS 像素尺寸 */
  w: number;
  h: number;
  /** 秒（单调时钟） */
  t: number;
  /** 距上一帧秒数 */
  dt: number;
  /** 64 bin 平滑频谱 0..1（真实 FFT 或模拟回退，已无缝混合） */
  levels: Float32Array;
  /** 低频能量 0..1（呼吸/脉动驱动） */
  bass: number;
  /** 节拍脉冲 0..1（onset 沿保持后快速衰减） */
  beat: number;
  /** 是否真实音频驱动（false = 模拟回退） */
  live: boolean;
  playing: boolean;
  colors: VizColors;
  settings: SideVizSettings;
  /** 舞台坐标系焦点（当前句槽位）；环形效果围绕它画 */
  focus: { x: number; y: number } | null;
  /** 当前封面纹理（无封面 null；data URL 不 taint） */
  cover: HTMLImageElement | null;
  /** 亮色主题：加亮/白热在白底上隐形，效果应改用压深保色相的「墨」色与正常合成 */
  isLight: boolean;
}

export interface VizEffect {
  /** 舞台尺寸或渲染缩放变化时调用 */
  resize?(w: number, h: number): void;
  draw(f: VizFrame): void;
  dispose?(): void;
}

export function rgba(c: readonly [number, number, number], a: number): string {
  return `rgba(${c[0]},${c[1]},${c[2]},${a})`;
}

/** 预渲染的径向渐变发光点 sprite（逐粒子建渐变是性能杀手，一律 drawImage sprite） */
export function makeGlowSprite(col: readonly [number, number, number]): HTMLCanvasElement {
  const cv = document.createElement("canvas");
  cv.width = 64;
  cv.height = 64;
  const c = cv.getContext("2d")!;
  const g = c.createRadialGradient(32, 32, 0, 32, 32, 32);
  g.addColorStop(0, rgba(col, 0.95));
  g.addColorStop(0.25, rgba(col, 0.5));
  g.addColorStop(0.6, rgba(col, 0.14));
  g.addColorStop(1, rgba(col, 0));
  c.fillStyle = g;
  c.fillRect(0, 0, 64, 64);
  return cv;
}

/** 环形半径：贴合歌词区较短边，留出歌词可读空间 */
export function radialBase(w: number, h: number): number {
  return Math.min(w, h) * 0.16;
}

/** 当前句固定槽位（舞台坐标）；无外部焦点时用 0.42 对齐歌词引擎 */
export function focusPoint(
  w: number,
  h: number,
  focus: { x: number; y: number } | null,
): { x: number; y: number } {
  const pad = radialBase(w, h) * 1.25;
  if (focus && Number.isFinite(focus.x) && Number.isFinite(focus.y)) {
    return {
      x: Math.min(w - pad * 0.5, Math.max(pad * 0.5, focus.x)),
      y: Math.min(h - pad * 0.35, Math.max(pad * 0.35, focus.y)),
    };
  }
  return { x: w * 0.5, y: h * 0.42 };
}
