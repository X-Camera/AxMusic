import { useEffect, useRef } from "react";

import type { SideVizSettings } from "../../lib/types";
import { vizPaletteColors } from "./sideViz";

/**
 * 主界面歌词区背景动效层。
 * - 单 canvas，按 kind 切换绘制；参数热更新不重挂
 * - color + palette 决定配色；开启后始终有可见动态
 */
export function VisualizerLayer({
  settings,
  playing,
}: {
  settings: SideVizSettings;
  playing: boolean;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const settingsRef = useRef(settings);
  const playingRef = useRef(playing);
  settingsRef.current = settings;
  playingRef.current = playing;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const parent = canvas.parentElement;
    if (!parent) return;
    // 关闭时卸掉绘制循环，避免歌词栏常开仍空转 rAF
    if (!settings.enabled) {
      const ctx0 = canvas.getContext("2d");
      ctx0?.clearRect(0, 0, canvas.width, canvas.height);
      return;
    }

    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    let w = 0;
    let h = 0;
    let raf = 0;
    let alive = true;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const bins = 24;
    const levels = new Float32Array(bins);
    const targets = new Float32Array(bins);
    const parts = Array.from({ length: 56 }, (_, i) => ({
      x: (i * 0.173) % 1,
      y: (i * 0.271) % 1,
      vx: (Math.random() - 0.5) * 0.45 + 0.12,
      vy: (Math.random() - 0.5) * 0.4 - 0.08,
      r: 1.2 + Math.random() * 2.8,
      ph: Math.random() * Math.PI * 2,
    }));

    function resize() {
      const rect = parent!.getBoundingClientRect();
      w = Math.max(1, Math.floor(rect.width));
      h = Math.max(1, Math.floor(rect.height));
      canvas!.width = Math.floor(w * dpr);
      canvas!.height = Math.floor(h * dpr);
      canvas!.style.width = `${w}px`;
      canvas!.style.height = `${h}px`;
      ctx!.setTransform(dpr, 0, 0, dpr, 0, 0);
    }

    const ro = new ResizeObserver(resize);
    ro.observe(parent);
    resize();

    function rgba(c: [number, number, number], a: number) {
      return `rgba(${c[0]},${c[1]},${c[2]},${a})`;
    }

    function energy(t: number) {
      const s = settingsRef.current;
      const live = playingRef.current;
      const beat = live
        ? Math.pow(Math.max(0, Math.sin(t * Math.PI * 2 * (112 / 60) * 0.5)), 3)
        : 0;
      const idle = 0.35 + 0.25 * Math.sin(t * 0.7);
      const swell = live ? 0.55 + 0.45 * Math.sin(t * 0.9) : idle;
      for (let i = 0; i < bins; i++) {
        const f = i / bins;
        const tilt =
          Math.pow(1 - f, 1.15) * (0.35 + beat * 0.65 * s.intensity + swell * 0.35);
        const wob = Math.sin(t * (1.4 + f * 4.5) + i * 0.7) * (0.12 + s.intensity * 0.18);
        targets[i] = Math.min(1, Math.max(0.05, tilt + wob));
        levels[i] += (targets[i] - levels[i]) * (live ? 0.28 : 0.12);
      }
      return beat;
    }

    function drawAurora(t: number, beat: number) {
      const s = settingsRef.current;
      const sp = s.speed;
      const cols = vizPaletteColors(s.palette, s.color);
      const g = ctx!.createLinearGradient(0, 0, w, h);
      const p = (Math.sin(t * 0.55 * sp) + 1) * 0.5;
      const q = (Math.cos(t * 0.38 * sp) + 1) * 0.5;
      const a1 = 0.14 + s.intensity * 0.28 + beat * 0.16 * s.intensity;
      const a2 = 0.1 + s.intensity * 0.22;
      g.addColorStop(0, rgba(cols.a, a1 * (0.55 + p * 0.55)));
      g.addColorStop(0.4 + q * 0.2, rgba(cols.b, a2 * (0.7 + q * 0.5)));
      g.addColorStop(1, rgba(cols.c, a1 * (0.5 + p * 0.5)));
      ctx!.fillStyle = g;
      ctx!.fillRect(0, 0, w, h);

      for (let i = 0; i < 3; i++) {
        const phase = t * (0.35 + i * 0.12) * sp + i * 2.1;
        const cx = w * (0.2 + i * 0.3) + Math.sin(phase) * w * 0.22;
        const cy = h * (0.3 + i * 0.22) + Math.cos(phase * 0.8 + i) * h * 0.2;
        const rad =
          Math.min(w, h) *
          (0.32 + s.intensity * 0.28 + beat * 0.1 * s.intensity + 0.06 * Math.sin(phase * 1.3));
        const rg = ctx!.createRadialGradient(cx, cy, 0, cx, cy, Math.max(20, rad));
        const alpha = (0.12 + s.intensity * 0.2 + beat * 0.12 * s.intensity) * (1 - i * 0.18);
        const base = i === 0 ? cols.a : i === 1 ? cols.b : cols.c;
        rg.addColorStop(0, rgba(base, alpha));
        rg.addColorStop(0.55, rgba(base, alpha * 0.35));
        rg.addColorStop(1, "rgba(0,0,0,0)");
        ctx!.fillStyle = rg;
        ctx!.fillRect(0, 0, w, h);
      }
    }

    function drawSpectrum(t: number, beat: number) {
      const s = settingsRef.current;
      const cols = vizPaletteColors(s.palette, s.color);
      const n = bins;
      const baseY = h * 0.94;
      const maxH = h * (0.22 + s.intensity * 0.5 + beat * 0.12 * s.intensity);
      const gap = 2;
      const bw = Math.max(2, (w * 0.78) / n - gap);
      const x0 = (w - (bw + gap) * n) / 2;
      for (let i = 0; i < n; i++) {
        const idle = 0.18 + 0.18 * Math.sin(t * (1.6 + i * 0.15) + i * 0.4);
        const v = Math.min(1, levels[i] * 0.75 + idle * 0.55);
        const bh = 6 + v * maxH;
        const x = x0 + i * (bw + gap);
        const mix = i / (n - 1);
        const col: [number, number, number] = [
          cols.a[0] + (cols.b[0] - cols.a[0]) * mix,
          cols.a[1] + (cols.b[1] - cols.a[1]) * mix,
          cols.a[2] + (cols.b[2] - cols.a[2]) * mix,
        ];
        const a = 0.18 + v * (0.35 + s.intensity * 0.55);
        ctx!.fillStyle = rgba(col, a);
        ctx!.fillRect(x, baseY - bh, bw, bh);
      }
    }

    function drawParticles(t: number, beat: number) {
      const s = settingsRef.current;
      const cols = vizPaletteColors(s.palette, s.color);
      const count = Math.floor(16 + s.intensity * 40 + beat * 10 * s.intensity);
      for (let i = 0; i < Math.min(count, parts.length); i++) {
        const p = parts[i];
        const boost = playingRef.current ? 1 + beat * 1.6 : 0.55;
        p.x += p.vx * 0.004 * s.speed * boost;
        p.y += p.vy * 0.004 * s.speed * boost;
        if (p.x < -0.06) p.x = 1.06;
        if (p.x > 1.06) p.x = -0.06;
        if (p.y < -0.06) p.y = 1.06;
        if (p.y > 1.06) p.y = -0.06;

        const breathe = 0.55 + 0.45 * Math.sin(t * (1.1 + p.ph) * s.speed + p.ph);
        const a = (0.14 + s.intensity * 0.4) * breathe * (0.65 + beat * 0.45);
        const r =
          p.r * (1 + s.intensity * 1.1 + beat * 0.7 * s.intensity + 0.15 * Math.sin(t * 2 + p.ph));
        const mix = (Math.sin(p.ph + t * 0.2) + 1) * 0.5;
        const col: [number, number, number] = [
          cols.a[0] + (cols.c[0] - cols.a[0]) * mix,
          cols.a[1] + (cols.c[1] - cols.a[1]) * mix,
          cols.a[2] + (cols.c[2] - cols.a[2]) * mix,
        ];
        ctx!.beginPath();
        ctx!.arc(p.x * w, p.y * h, r, 0, Math.PI * 2);
        ctx!.fillStyle = rgba(col, a);
        ctx!.fill();
      }
    }

    function paint(t: number) {
      const s = settingsRef.current;
      ctx!.clearRect(0, 0, w, h);
      if (!s.enabled) return;
      const beat = energy(t);
      if (s.kind === "aurora") drawAurora(t, beat);
      else if (s.kind === "spectrum") drawSpectrum(t, beat);
      else drawParticles(t, beat);
    }

    function frame(ts: number) {
      if (!alive) return;
      paint(ts * 0.001);
      raf = requestAnimationFrame(frame);
    }

    raf = requestAnimationFrame(frame);

    return () => {
      alive = false;
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [settings.enabled]);

  return (
    <canvas
      ref={canvasRef}
      className="side-viz-layer"
      aria-hidden="true"
      style={{ opacity: settings.enabled ? settings.opacity : 0 }}
    />
  );
}
