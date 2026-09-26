import type { VizEffect, VizFrame } from "./types";
import { rgba } from "./types";

/** 柔光：对角渐变铺底 + 3 个游动光晕；低频呼吸半径 */
export function createAurora(): VizEffect {
  return {
    draw(f: VizFrame) {
      const { ctx, w, h, t, colors: cols, settings: s, bass, beat } = f;
      if (!ctx) return;
      const sp = s.speed;
      const pulse = bass * 0.6 + beat * 0.4;
      const g = ctx.createLinearGradient(0, 0, w, h);
      const p = (Math.sin(t * 0.55 * sp) + 1) * 0.5;
      const q = (Math.cos(t * 0.38 * sp) + 1) * 0.5;
      const a1 = 0.14 + s.intensity * 0.28 + pulse * 0.16 * s.intensity;
      const a2 = 0.1 + s.intensity * 0.22;
      g.addColorStop(0, rgba(cols.a, a1 * (0.55 + p * 0.55)));
      g.addColorStop(0.4 + q * 0.2, rgba(cols.b, a2 * (0.7 + q * 0.5)));
      g.addColorStop(1, rgba(cols.c, a1 * (0.5 + p * 0.5)));
      ctx.fillStyle = g;
      ctx.fillRect(0, 0, w, h);

      for (let i = 0; i < 3; i++) {
        const phase = t * (0.35 + i * 0.12) * sp + i * 2.1;
        const cx = w * (0.2 + i * 0.3) + Math.sin(phase) * w * 0.22;
        const cy = h * (0.3 + i * 0.22) + Math.cos(phase * 0.8 + i) * h * 0.2;
        const rad =
          Math.min(w, h) *
          (0.32 +
            s.intensity * 0.28 +
            pulse * 0.14 * s.intensity +
            0.06 * Math.sin(phase * 1.3));
        const rg = ctx.createRadialGradient(cx, cy, 0, cx, cy, Math.max(20, rad));
        const alpha = (0.12 + s.intensity * 0.2 + pulse * 0.12 * s.intensity) * (1 - i * 0.18);
        const base = i === 0 ? cols.a : i === 1 ? cols.b : cols.c;
        rg.addColorStop(0, rgba(base, alpha));
        rg.addColorStop(0.55, rgba(base, alpha * 0.35));
        rg.addColorStop(1, "rgba(0,0,0,0)");
        ctx.fillStyle = rg;
        ctx.fillRect(0, 0, w, h);
      }
    },
  };
}
