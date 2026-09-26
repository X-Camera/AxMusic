import { api } from "../../lib/api";
import type { Rgb, VizColors } from "./sideViz";
import { shiftHue } from "./sideViz";

/**
 * 封面纹理 + 主色提取。
 *
 * ⚠️ 红线：封面必须走 base64 data URL（track_cover_thumb），
 * 禁止换成 convertFileSrc/asset protocol —— 那会 taint canvas，
 * 导致 getImageData 抛 SecurityError、流体效果 drawImage 失败。
 *
 * 96px 缩略图对流体（最终大模糊）与取色都足够，且有磁盘+内存双缓存。
 */

export interface CoverArt {
  img: HTMLImageElement;
  /** 提取的三色；封面几乎全灰时为 null（调用方回退主色推导） */
  colors: VizColors | null;
}

const cache = new Map<string, Promise<CoverArt | null>>();

export function loadCoverArt(path: string): Promise<CoverArt | null> {
  if (!path) return Promise.resolve(null);
  let p = cache.get(path);
  if (!p) {
    p = doLoad(path);
    cache.set(path, p);
    // 上限兜底，防长跑膨胀
    if (cache.size > 30) {
      const first = cache.keys().next().value;
      if (first !== undefined) cache.delete(first);
    }
  }
  return p;
}

async function doLoad(path: string): Promise<CoverArt | null> {
  let dataUrl: string | null = null;
  try {
    dataUrl = await api.trackCoverThumb(path);
  } catch {
    return null;
  }
  if (!dataUrl) return null;
  const img = new Image();
  img.src = dataUrl;
  try {
    await img.decode();
  } catch {
    return null;
  }
  if (!img.width || !img.height) return null;
  return { img, colors: extractPalette(img) };
}

/** 48×48 降采样 → HSV 过滤 → 12 色相桶加权投票 → 互斥取三色 */
function extractPalette(img: HTMLImageElement): VizColors | null {
  const N = 48;
  const cv = document.createElement("canvas");
  cv.width = N;
  cv.height = N;
  const ctx = cv.getContext("2d", { willReadFrequently: true });
  if (!ctx) return null;
  ctx.drawImage(img, 0, 0, N, N);
  let data: Uint8ClampedArray;
  try {
    data = ctx.getImageData(0, 0, N, N).data;
  } catch {
    return null;
  }

  const BUCKETS = 12;
  const wSum = new Float64Array(BUCKETS);
  const rSum = new Float64Array(BUCKETS);
  const gSum = new Float64Array(BUCKETS);
  const bSum = new Float64Array(BUCKETS);
  let colorful = 0;

  for (let i = 0; i < N * N; i++) {
    const r = data[i * 4] / 255;
    const g = data[i * 4 + 1] / 255;
    const b = data[i * 4 + 2] / 255;
    const mx = Math.max(r, g, b);
    const mn = Math.min(r, g, b);
    const v = mx;
    const s = mx === 0 ? 0 : (mx - mn) / mx;
    // 滤掉灰白黑：只留有色且明暗适中的像素
    if (s < 0.22 || v < 0.15 || v > 0.92) continue;
    let h = 0;
    const d = mx - mn;
    if (mx === r) h = ((g - b) / d + 6) % 6;
    else if (mx === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
    h *= 60;
    // 权重：饱和度 × 明度居中系数（太暗太亮的低权）
    const w = s * (1 - Math.abs(v - 0.55) * 1.2);
    const bi = Math.min(BUCKETS - 1, Math.floor(h / 30));
    wSum[bi] += w;
    rSum[bi] += r * w;
    gSum[bi] += g * w;
    bSum[bi] += b * w;
    colorful++;
  }

  // 几乎全灰的封面：返回 null 让调用方回退主色
  if (colorful < N * N * 0.04) return null;

  // 互斥选取：每桶与已选桶色相距 ≥2 桶（60°）
  const picked: Rgb[] = [];
  const used = new Set<number>();
  for (let k = 0; k < 3; k++) {
    let best = -1;
    let bestW = 0;
    for (let i = 0; i < BUCKETS; i++) {
      if (used.has(i)) continue;
      let nearUsed = false;
      for (const u of used) {
        const dist = Math.min(Math.abs(i - u), BUCKETS - Math.abs(i - u));
        if (dist < 2) {
          nearUsed = true;
          break;
        }
      }
      if (nearUsed) continue;
      if (wSum[i] > bestW) {
        bestW = wSum[i];
        best = i;
      }
    }
    if (best < 0 || bestW <= 0) break;
    used.add(best);
    picked.push(vibrant([rSum[best] / bestW, gSum[best] / bestW, bSum[best] / bestW]));
  }

  if (picked.length === 0) return null;
  while (picked.length < 3) {
    picked.push(shiftHue(picked[picked.length - 1], 40));
  }
  return { a: picked[0], b: picked[1], c: picked[2] };
}

/** 取色后提亮增艳：发光类动效需要中高饱和、中高明度才好看 */
function vibrant(rgb: [number, number, number]): Rgb {
  const [r, g, b] = rgb;
  const mx = Math.max(r, g, b);
  const mn = Math.min(r, g, b);
  const l = (mx + mn) / 2;
  const d = mx - mn;
  let s = d === 0 ? 0 : d / (1 - Math.abs(2 * l - 1));
  s = Math.max(s, 0.52);
  const l2 = Math.min(0.66, Math.max(0.5, l));
  const c = (1 - Math.abs(2 * l2 - 1)) * s;
  const x = c * (1 - Math.abs((((hue(r, g, b) / 60) % 2) + 2) % 2 - 1));
  const m = l2 - c / 2;
  const h = hue(r, g, b);
  let rp = 0;
  let gp = 0;
  let bp = 0;
  if (h < 60) [rp, gp, bp] = [c, x, 0];
  else if (h < 120) [rp, gp, bp] = [x, c, 0];
  else if (h < 180) [rp, gp, bp] = [0, c, x];
  else if (h < 240) [rp, gp, bp] = [0, x, c];
  else if (h < 300) [rp, gp, bp] = [x, 0, c];
  else [rp, gp, bp] = [c, 0, x];
  return [
    Math.round((rp + m) * 255),
    Math.round((gp + m) * 255),
    Math.round((bp + m) * 255),
  ];
}

function hue(r: number, g: number, b: number): number {
  const mx = Math.max(r, g, b);
  const mn = Math.min(r, g, b);
  const d = mx - mn;
  if (d === 0) return 0;
  let h = 0;
  if (mx === r) h = ((g - b) / d + 6) % 6;
  else if (mx === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  return h * 60;
}
