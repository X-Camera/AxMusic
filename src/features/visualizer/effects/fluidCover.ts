import type { VizEffect, VizFrame } from "./types";
import { rgba } from "./types";

/**
 * 封面流体（参考 AMLL bg-render 的构成思路，零依赖 canvas 2D 实现）：
 * - 离屏 1/4 分辨率画 4 层封面：异速反向旋转 + 利萨茹轨道漂移 + "lighter" 叠亮
 * - 主 canvas 放大铺满（低分辨率放大本身就是一层免费柔化），低音驱动呼吸缩放
 * - 重度模糊/调色由宿主施加在 canvas 元素上的 CSS filter 完成（GPU 合成）
 * - 无封面：退化为配色径向渐变团，观感仍成立
 */

const LAYERS = [
  { scale: 1.7, dir: 1, rate: 0.05, alpha: 0.5, orbit: 0.1 },
  { scale: 2.3, dir: -1, rate: 0.082, alpha: 0.36, orbit: 0.15 },
  { scale: 3.0, dir: 1, rate: 0.064, alpha: 0.27, orbit: 0.2 },
  { scale: 3.8, dir: -1, rate: 0.096, alpha: 0.2, orbit: 0.26 },
];

export function createFluidCover(): VizEffect {
  let off: HTMLCanvasElement | null = null;
  let offCtx: CanvasRenderingContext2D | null = null;

  return {
    draw(f: VizFrame) {
      const { ctx, w, h, t, colors: cols, settings: s, bass } = f;
      if (!ctx) return;
      const p = s.fluid;

      if (!off) {
        off = document.createElement("canvas");
        offCtx = off.getContext("2d");
      }
      const octx = offCtx;
      if (!off || !octx) return;
      // 离屏分辨率再降一档：旋转+放大后细节无差，省 3/4 像素填充
      const qw = Math.max(2, Math.floor(w / 4));
      const qh = Math.max(2, Math.floor(h / 4));
      if (off.width !== qw || off.height !== qh) {
        off.width = qw;
        off.height = qh;
      }
      octx.clearRect(0, 0, qw, qh);
      octx.globalCompositeOperation = "lighter";

      const sp = s.speed * (0.35 + p.spin * 1.3);
      const vis = 0.5 + s.intensity * 0.5;

      if (f.cover) {
        const img = f.cover;
        const fit = Math.max(qw / img.width, qh / img.height);
        for (let i = 0; i < LAYERS.length; i++) {
          const L = LAYERS[i];
          const rot = t * L.rate * sp * L.dir;
          const cx = qw / 2 + Math.sin(t * 0.11 * sp + i * 2.2) * qw * L.orbit;
          const cy = qh / 2 + Math.cos(t * 0.09 * sp + i * 1.7) * qh * L.orbit;
          const dw = img.width * fit * L.scale;
          const dh = img.height * fit * L.scale;
          octx.globalAlpha = L.alpha * vis;
          octx.save();
          octx.translate(cx, cy);
          octx.rotate(rot);
          octx.drawImage(img, -dw / 2, -dh / 2, dw, dh);
          octx.restore();
        }
      } else {
        // 无封面：三个配色光团漂移叠亮
        const colors = [cols.a, cols.b, cols.c];
        for (let i = 0; i < 3; i++) {
          const cx = qw / 2 + Math.sin(t * 0.13 * sp + i * 2.2) * qw * 0.22;
          const cy = qh / 2 + Math.cos(t * 0.1 * sp + i * 1.7) * qh * 0.22;
          const rad = (Math.min(qw, qh) / 2) * (0.9 - i * 0.18);
          const g = octx.createRadialGradient(cx, cy, 0, cx, cy, rad);
          g.addColorStop(0, rgba(colors[i], 0.5 * vis));
          g.addColorStop(0.6, rgba(colors[i], 0.22 * vis));
          g.addColorStop(1, "rgba(0,0,0,0)");
          octx.globalAlpha = 1;
          octx.fillStyle = g;
          octx.fillRect(0, 0, qw, qh);
        }
      }
      octx.globalAlpha = 1;
      octx.globalCompositeOperation = "source-over";

      // 上屏：低音呼吸（缩放脉动），亮度脉动由宿主的 CSS filter 承担
      const breatheScale = 1 + bass * 0.1 * p.breathe * (0.3 + s.intensity * 0.7);
      ctx.save();
      ctx.imageSmoothingEnabled = true;
      ctx.imageSmoothingQuality = "high";
      ctx.translate(w / 2, h / 2);
      ctx.scale(breatheScale, breatheScale);
      ctx.drawImage(off, -w / 2, -h / 2, w, h);
      ctx.restore();
    },
    dispose() {
      off = null;
      offCtx = null;
    },
  };
}

/** 流体帧的 CSS filter（宿主每帧设置；数值圆整去抖，避免样式空转） */
export function fluidCssFilter(
  s: { fluid: { blur: number; breathe: number }; intensity: number },
  bass: number,
): string {
  const blurPx = Math.round(6 + s.fluid.blur * 26);
  const sat = (1.05 + 0.45 * s.intensity).toFixed(2);
  const bright = (0.92 + bass * 0.22 * s.fluid.breathe).toFixed(2);
  return `blur(${blurPx}px) saturate(${sat}) brightness(${bright})`;
}
