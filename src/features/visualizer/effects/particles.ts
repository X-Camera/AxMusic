import type { Rgb } from "../sideViz";
import { lerpRgb } from "../sideViz";
import type { VizEffect, VizFrame } from "./types";
import { rgba } from "./types";

/** 确定性伪随机：同一序号永远同一粒子，面板重开观感不变 */
function frac(n: number) {
  return n - Math.floor(n);
}

interface Particle {
  x: number;
  y: number;
  vx: number;
  vy: number;
  r: number;
  ph: number;
}

const MAX_PARTS = 160;

/**
 * 粒子：发光 sprite（配色变化时预渲染）+ 可选 plexus 连线 + 节拍脉动。
 * 逐粒子 createRadialGradient 是性能杀手，一律 drawImage 预渲染 sprite。
 */
export function createParticles(): VizEffect {
  const parts: Particle[] = Array.from({ length: MAX_PARTS }, (_, i) => ({
    x: frac(i * 0.173 + 0.11),
    y: frac(i * 0.271 + 0.37),
    vx: (frac(i * 0.619 + 0.23) - 0.5) * 0.45 + 0.12,
    vy: (frac(i * 0.377 + 0.51) - 0.5) * 0.4 - 0.08,
    r: 1.2 + frac(i * 0.531 + 0.07) * 2.8,
    ph: frac(i * 0.827 + 0.63) * Math.PI * 2,
  }));
  let sprites: HTMLCanvasElement[] = [];
  let spriteKey = "";

  function buildSprites(cols: { a: Rgb; b: Rgb; c: Rgb }) {
    sprites = [cols.a, cols.b, cols.c].map((col) => {
      const cv = document.createElement("canvas");
      cv.width = 64;
      cv.height = 64;
      const c = cv.getContext("2d")!;
      const g = c.createRadialGradient(32, 32, 0, 32, 32, 32);
      g.addColorStop(0, rgba(col, 0.9));
      g.addColorStop(0.25, rgba(col, 0.5));
      g.addColorStop(0.6, rgba(col, 0.14));
      g.addColorStop(1, rgba(col, 0));
      c.fillStyle = g;
      c.fillRect(0, 0, 64, 64);
      return cv;
    });
  }

  return {
    draw(f: VizFrame) {
      const { ctx, w, h, t, colors: cols, settings: s, beat, bass, playing } = f;
      if (!ctx) return;
      const ex = s.particles_ex;
      const key = `${cols.a}|${cols.b}|${cols.c}`;
      if (key !== spriteKey) {
        spriteKey = key;
        buildSprites(cols);
      }

      const count = Math.min(MAX_PARTS, Math.round(ex.count));
      const pulse = beat * (playing ? 1 : 0.4);
      const speedBoost = 1 + pulse * 1.2 + bass * 0.5;
      const sizeK = 0.5 + ex.size * 1.6;

      // 位置推进 + 绘制光点
      for (let i = 0; i < count; i++) {
        const p = parts[i];
        p.x += p.vx * 0.004 * s.speed * speedBoost;
        p.y += p.vy * 0.004 * s.speed * speedBoost;
        if (p.x < -0.06) p.x = 1.06;
        if (p.x > 1.06) p.x = -0.06;
        if (p.y < -0.06) p.y = 1.06;
        if (p.y > 1.06) p.y = -0.06;

        const breathe = 0.55 + 0.45 * Math.sin(t * (1.1 + p.ph) * s.speed + p.ph);
        const r = p.r * sizeK * (1 + s.intensity * 0.9 + pulse * 0.6);
        const px = p.x * w;
        const py = p.y * h;
        const sprite = sprites[i % 3];
        const d = r * 4; // sprite 含光晕，半径放大 4 倍绘制
        ctx.globalAlpha = (0.2 + s.intensity * 0.45) * breathe * (0.7 + pulse * 0.3);
        ctx.drawImage(sprite, px - d / 2, py - d / 2, d, d);
      }

      // plexus 连线（近距点对；alpha 量化分桶成批 stroke，避免数百次状态切换）
      if (ex.links && count >= 2 && count <= 120) {
        const dist = Math.min(w, h) * (0.06 + ex.link_dist * 0.2);
        const d2max = dist * dist;
        const linkCol = lerpRgb(cols.a, cols.b, 0.5);
        const bucketPaths: Path2D[] = Array.from({ length: 6 }, () => new Path2D());
        for (let i = 0; i < count; i++) {
          const ax = parts[i].x * w;
          const ay = parts[i].y * h;
          for (let j = i + 1; j < count; j++) {
            const dx = parts[j].x * w - ax;
            const dy = parts[j].y * h - ay;
            const d2 = dx * dx + dy * dy;
            if (d2 > d2max) continue;
            const a = (1 - Math.sqrt(d2) / dist) * (0.1 + s.intensity * 0.22) * (1 + pulse * 0.5);
            const bi = Math.min(5, Math.max(0, Math.round(a * 25) - 1));
            if (bi < 0) continue;
            const p2d = bucketPaths[bi];
            p2d.moveTo(ax, ay);
            p2d.lineTo(parts[j].x * w, parts[j].y * h);
          }
        }
        ctx.globalAlpha = 1;
        ctx.lineWidth = 1;
        for (let bi = 0; bi < 6; bi++) {
          const a = (bi + 1) / 25;
          ctx.strokeStyle = rgba(linkCol, a);
          ctx.stroke(bucketPaths[bi]);
        }
      }
      ctx.globalAlpha = 1;
    },
  };
}
