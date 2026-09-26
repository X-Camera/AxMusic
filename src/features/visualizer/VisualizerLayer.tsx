import { useEffect, useRef } from "react";

import type { SideVizKind, SideVizSettings } from "../../lib/types";
import { getVizAudioSource } from "./audioSource";
import type { CoverArt } from "./coverArt";
import { createAurora } from "./effects/aurora";
import { createFluidCover, fluidCssFilter } from "./effects/fluidCover";
import { createParticles } from "./effects/particles";
import { createRadialBars, createRadialLine } from "./effects/radial";
import { createSpectrum } from "./effects/spectrum";
import type { VizEffect, VizFrame } from "./effects/types";
import { vizPaletteColors } from "./sideViz";
import { SilkCanvas, type SilkHandle } from "./SilkCanvas";

const FACTORIES: Partial<Record<SideVizKind, () => VizEffect>> = {
  aurora: createAurora,
  spectrum: createSpectrum,
  particles: createParticles,
  "radial-bars": createRadialBars,
  "radial-line": createRadialLine,
  fluid: createFluidCover,
};

/**
 * 主界面歌词区背景动效宿主。
 * - 单一 rAF 循环驱动全部效果（含 WebGL 丝绸），fps 上限 / 渲染缩放 / 隐藏暂停单点生效
 * - 参数热更新不重挂；音频源订阅随 enabled 开关（关闭时后端分接零开销）
 * - 丝绸（WebGL）独占独立 canvas（context 模式锁定），其余效果共享 2D canvas
 */
export function VisualizerLayer({
  settings,
  playing,
  getFocus,
  cover,
}: {
  settings: SideVizSettings;
  playing: boolean;
  /** 舞台坐标系下的焦点中心（当前句歌词）；环形效果围绕它画 */
  getFocus?: () => { x: number; y: number } | null;
  /** 当前封面（纹理 + 提取色）；无封面 null */
  cover?: CoverArt | null;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const silkRef = useRef<SilkHandle>(null);
  const settingsRef = useRef(settings);
  const playingRef = useRef(playing);
  const focusRef = useRef(getFocus);
  const coverRef = useRef(cover ?? null);
  settingsRef.current = settings;
  playingRef.current = playing;
  focusRef.current = getFocus;
  coverRef.current = cover ?? null;

  const silkActive = settings.enabled && settings.kind === "silk";

  // 订阅生命周期：开启时接 Rust 频谱推送，关闭/卸载即断（后端随即静默）
  useEffect(() => {
    getVizAudioSource().setSubscribed(settings.enabled);
    if (!settings.enabled) return;
    return () => getVizAudioSource().setSubscribed(false);
  }, [settings.enabled]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const parent = canvas.parentElement;
    if (!parent) return;
    // 关闭时卸掉绘制循环，避免歌词栏常开仍空转 rAF
    if (!settings.enabled) {
      canvas.getContext("2d")?.clearRect(0, 0, canvas.width, canvas.height);
      canvas.style.filter = "";
      return;
    }

    let w = 1;
    let h = 1;
    let appliedScale = 0;
    let raf = 0;
    let alive = true;
    let lastTs = 0;
    let lastFrameT = 0;
    let lastFilter = "";

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const effects = new Map<SideVizKind, VizEffect>();
    const audio = getVizAudioSource();
    // 帧对象复用，避免 60Hz 造垃圾
    const frame: VizFrame = {
      ctx,
      w: 1,
      h: 1,
      t: 0,
      dt: 0.016,
      levels: audio.levels,
      bass: 0,
      beat: 0,
      live: false,
      playing: false,
      colors: vizPaletteColors("soft"),
      settings,
      focus: null,
      cover: null,
      isLight: false,
    };

    function resize() {
      const rect = parent!.getBoundingClientRect();
      w = Math.max(1, Math.floor(rect.width));
      h = Math.max(1, Math.floor(rect.height));
      const rs = settingsRef.current.render_scale || 1;
      appliedScale = rs;
      const dpr = Math.min(window.devicePixelRatio || 1, 2) * rs;
      canvas!.width = Math.max(1, Math.floor(w * dpr));
      canvas!.height = Math.max(1, Math.floor(h * dpr));
      canvas!.style.width = `${w}px`;
      canvas!.style.height = `${h}px`;
      ctx!.setTransform(dpr, 0, 0, dpr, 0, 0);
      for (const e of effects.values()) e.resize?.(w, h);
      silkRef.current?.resize(w, h, rs);
    }

    const ro = new ResizeObserver(resize);
    ro.observe(parent);
    resize();

    function effectFor(kind: SideVizKind): VizEffect | null {
      let e = effects.get(kind);
      if (!e) {
        const factory = FACTORIES[kind];
        if (!factory) return null;
        e = factory();
        e.resize?.(w, h);
        effects.set(kind, e);
      }
      return e;
    }

    function paint(tsMs: number) {
      const s = settingsRef.current;
      const t = tsMs * 0.001;
      const dt = Math.min(0.1, Math.max(0.001, t - lastFrameT || 0.016));
      lastFrameT = t;

      // render_scale 热改 → 重建 backing store
      if ((s.render_scale || 1) !== appliedScale) resize();

      const au = audio.frame(t, dt, playingRef.current, s.intensity);
      frame.t = t;
      frame.dt = dt;
      frame.w = w;
      frame.h = h;
      frame.bass = au.bass;
      frame.beat = au.beat;
      frame.live = au.live;
      frame.playing = playingRef.current;
      frame.colors = vizPaletteColors(s.palette, s.color, coverRef.current?.colors ?? null);
      frame.settings = s;
      frame.focus = focusRef.current?.() ?? null;
      frame.cover = coverRef.current?.img ?? null;
      // 主题热切换即时生效（读 attribute 开销可忽略）
      frame.isLight = document.documentElement.dataset.theme === "light";

      if (s.kind === "silk") {
        // WebGL 独占另一 canvas；本 canvas 不画（resize 幂等，每帧确保同步）
        if (lastFilter) {
          canvas!.style.filter = "";
          lastFilter = "";
        }
        silkRef.current?.resize(w, h, appliedScale);
        silkRef.current?.render(frame);
        return;
      }

      ctx!.clearRect(0, 0, w, h);
      effectFor(s.kind)?.draw(frame);

      // 流体的重度模糊/调色走 canvas 元素的 CSS filter（GPU 合成，圆整去抖）
      if (s.kind === "fluid") {
        const f = fluidCssFilter(s, frame.bass);
        if (f !== lastFilter) {
          canvas!.style.filter = f;
          lastFilter = f;
        }
      } else if (lastFilter) {
        canvas!.style.filter = "";
        lastFilter = "";
      }
    }

    function loop(tsMs: number) {
      if (!alive) return;
      raf = requestAnimationFrame(loop);
      if (document.hidden) return;
      const cap = settingsRef.current.fps_cap || 60;
      if (cap < 60 && tsMs - lastTs < 1000 / cap - 0.5) return;
      lastTs = tsMs;
      paint(tsMs);
    }

    raf = requestAnimationFrame(loop);

    return () => {
      alive = false;
      cancelAnimationFrame(raf);
      ro.disconnect();
      for (const e of effects.values()) e.dispose?.();
      effects.clear();
      canvas.style.filter = "";
    };
  }, [settings.enabled]);

  const opacity = settings.enabled ? settings.opacity : 0;
  return (
    <>
      <canvas
        ref={canvasRef}
        className="side-viz-layer"
        aria-hidden="true"
        style={{ opacity, visibility: silkActive ? "hidden" : undefined }}
      />
      {silkActive && <SilkCanvas ref={silkRef} opacity={opacity} />}
    </>
  );
}
