import type { Rgb } from "../sideViz";
import { lerpRgb } from "../sideViz";
import { resampleLevels } from "./radial";
import type { VizEffect, VizFrame } from "./types";
import { rgba } from "./types";

/**
 * 频谱：圆角柱 + a→b→c 纵向渐变 + 可选发光/峰值滞留/上下镜像。
 * 布局撑满：左右边距 = 柱间空隙，非镜像贴底（底距 = 柱间空隙）；
 * 渐变锚定静态最大量程，尺寸/配色变化时才重建，不逐帧创建对象。
 */
export function createSpectrum(): VizEffect {
  let peaks = new Float32Array(0);
  let grad: CanvasGradient | null = null;
  let gradKey = "";
  let lastBars = 0;

  function keyOf(w: number, h: number, cols: { a: Rgb; b: Rgb; c: Rgb }, mirror: boolean) {
    return `${w}x${h}|${cols.a}|${cols.b}|${cols.c}|${mirror ? 1 : 0}`;
  }

  return {
    draw(f: VizFrame) {
      const { ctx, w, h, colors: cols, settings: s } = f;
      if (!ctx) return;
      const ex = s.spectrum_ex;
      const n = Math.round(ex.bars);
      if (n !== lastBars) {
        peaks = new Float32Array(n);
        lastBars = n;
      }
      // 边距 = 柱间空隙：n 柱 + (n-1) 间隙 + 2 边距
      const gap = Math.max(1.5, w / n / 6);
      const bw = Math.max(2, (w - (n + 1) * gap) / n);
      const x0 = gap;
      const baseY = ex.mirror ? h * 0.5 : h - gap;
      /** 静态最大量程：镜像=总高（上下各半），非镜像=向上高度 */
      const availH = (ex.mirror ? h - gap * 2 : h - gap) * (0.35 + s.intensity * 0.6);
      // 动态量程随低音呼吸（不高于静态量程）
      const maxH = availH * (0.55 + f.bass * 0.45);
      const rad = Math.min(bw / 2, 4);

      const key = keyOf(w, h, cols, ex.mirror);
      if (!grad || gradKey !== key) {
        gradKey = key;
        grad = ctx.createLinearGradient(
          0,
          ex.mirror ? baseY - availH / 2 : baseY - availH,
          0,
          ex.mirror ? baseY + availH / 2 : baseY,
        );
        if (ex.mirror) {
          grad.addColorStop(0, rgba(cols.c, 0.75));
          grad.addColorStop(0.5, rgba(cols.a, 1));
          grad.addColorStop(1, rgba(cols.c, 0.75));
        } else {
          grad.addColorStop(0, rgba(cols.b, 0.85));
          grad.addColorStop(0.55, rgba(cols.a, 1));
          grad.addColorStop(1, rgba(cols.c, 0.8));
        }
      }

      const glowOn = ex.glow > 0.02;
      if (glowOn) {
        ctx.shadowColor = rgba(cols.b, 0.5 * ex.glow + 0.15);
        ctx.shadowBlur = 3 + ex.glow * 11;
      }
      ctx.fillStyle = grad;
      ctx.globalAlpha = Math.min(1, 0.35 + s.intensity * 0.55);

      for (let i = 0; i < n; i++) {
        const v = resampleLevels(f.levels, i, n);
        const bh = 2 + v * maxH;
        const x = x0 + i * (bw + gap);
        ctx.beginPath();
        if (ex.mirror) {
          ctx.roundRect(x, baseY - bh / 2, bw, bh, rad);
        } else {
          ctx.roundRect(x, baseY - bh, bw, bh, [rad, rad, 0, 0]);
        }
        ctx.fill();
        if (ex.peaks) {
          peaks[i] = Math.max(peaks[i] - f.dt * 0.55, v);
          const capH = 2.5;
          ctx.globalAlpha = Math.min(1, 0.5 + peaks[i] * 0.5);
          ctx.fillStyle = rgba(lerpRgb(cols.b, cols.a, 0.5), 1);
          ctx.beginPath();
          if (ex.mirror) {
            const off = (2 + peaks[i] * maxH) / 2;
            ctx.roundRect(x, baseY - off - capH / 2, bw, capH, capH / 2);
            ctx.roundRect(x, baseY + off - capH / 2, bw, capH, capH / 2);
          } else {
            ctx.roundRect(x, baseY - (2 + peaks[i] * maxH) - capH, bw, capH, capH / 2);
          }
          ctx.fill();
          ctx.fillStyle = grad;
          ctx.globalAlpha = Math.min(1, 0.35 + s.intensity * 0.55);
        }
      }
      ctx.globalAlpha = 1;
      ctx.shadowBlur = 0;
    },
    dispose() {
      grad = null;
      gradKey = "";
    },
  };
}
