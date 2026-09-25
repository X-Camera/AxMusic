/**
 * 同步歌词视图引擎（Apple Music 式观感；参考 amll，按本项目行级 LRC 简化）。
 *
 * 模型：行绝对定位，引擎每帧直写 style（不经 React 重渲染）——
 *   外层 .np-line      translateY（posY 弹簧）+ opacity/filter（CSS .4s 过渡平滑）
 *   内层 .np-line-inner scale（缩放弹簧，origin 左）
 * 滚动是自研 offset 引擎（非 scrollTop）：滚轮步进 / 拖拽跟手 + 惯性，
 * 交互期间冻结焦点挂起自动跟随，静止后弹回；点击句子回调 seek。
 */

import { useLayoutEffect, useEffect, useRef, useState } from "react";
import { findLrcIndex, type LrcLine } from "../lrc";
import { POS_Y_PARAMS, SCALE_PARAMS, Spring } from "./spring";

/** 焦点行中心对齐容器高度的比例（焦点略偏上，非正中） */
const ALIGN_POS = 0.35;
/** 阶梯级联：首行延迟 50ms 起，当前句之后每行衰减 ÷1.05 */
const STAGGER_BASE_S = 0.05;
const STAGGER_DECAY = 1 / 1.05;
/** 滚轮停止判定 / 自动跟随恢复 / 强制恢复 */
const WHEEL_IDLE_MS = 150;
const RESUME_AFTER_MS = 500;
const RESUME_FORCE_MS = 6000;
/** 拖拽意图阈值与惯性参数（摩擦按 60fps 帧归一） */
const INTENT_PX = 4;
const INERTIA_MIN_V = 0.05; // px/ms
const INERTIA_FRICTION = 0.95;
const MAX_FRAME_S = 0.1; // 挂起恢复首帧防过冲
/** 距离模糊：1+行距，封顶 5px；窗口 ≤1024px 打八折 */
const BLUR_MAX = 5;
const NARROW_WIDTH = 1024;

type Interaction = "none" | "wheel" | "drag" | "inertia";

interface EngineState {
  els: HTMLElement[];
  inners: (HTMLElement | null)[];
  springs: Spring[];
  scaleSprings: Spring[];
  heights: number[];
  prefix: number[];
  measured: boolean;
  boxH: number;
  offset: number;
  suspended: boolean;
  frozenFocus: number;
  interacting: Interaction;
  lastInteractEnd: number;
  lastIdx: number;
  noStaggerFrames: number;
  reducedMotion: boolean;
  drag: { startY: number; startOffset: number; lastY: number; lastT: number; v: number; moved: boolean; pointerId: number } | null;
  lastY: number[];
  lastScale: number[];
  lastBlur: number[];
}

function freshState(): EngineState {
  return {
    els: [],
    inners: [],
    springs: [],
    scaleSprings: [],
    heights: [],
    prefix: [],
    measured: false,
    boxH: 0,
    offset: 0,
    suspended: false,
    frozenFocus: 0,
    interacting: "none",
    lastInteractEnd: 0,
    lastIdx: -1,
    noStaggerFrames: 0,
    reducedMotion: false,
    drag: null,
    lastY: [],
    lastScale: [],
    lastBlur: [],
  };
}

export interface UseLyricsEngineOpts {
  lines: LrcLine[];
  synced: boolean;
  /** 播放时钟（含 scrub 预览与轮询间隙外推），由父组件提供 */
  getTimeMs: () => number;
  /** 点击某句（未拖动）时回调 */
  onSeekLine: (index: number, ms: number) => void;
  /** 字号/字体/行距等排版参数变化时重测行高（不触发载入飞入） */
  layoutKey?: string;
  /** 切歌/单曲循环重播：即使 lines 引用未变也整表重建，避免旧 DOM 残影叠加 */
  rebuildKey?: string;
  /** 距离模糊上限（px）。小字号容器请调低，默认 5 */
  maxBlur?: number;
}

export function useLyricsEngine(opts: UseLyricsEngineOpts) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [activeIdx, setActiveIdx] = useState(-1);
  const stRef = useRef<EngineState>(freshState());
  const optsRef = useRef(opts);
  optsRef.current = opts;
  const inertiaRafRef = useRef(0);
  const wheelTimerRef = useRef(0);

  const linesKey = opts.lines;

  /** 测量行高 + 前缀和并缓存行元素；行数对不上（React 还没渲染完）返回 false */
  function measure(): boolean {
    const box = containerRef.current;
    if (!box) return false;
    const st = stRef.current;
    // 只要直接子级，按 data-i 排序——防止残留/嵌套节点混进缓存
    const els = Array.from(box.querySelectorAll<HTMLElement>(":scope > [data-i]"));
    els.sort((a, b) => Number(a.dataset.i) - Number(b.dataset.i));
    if (els.length !== linesKey.length || linesKey.length === 0) {
      box.setAttribute("data-lyrics-ready", "0");
      return false;
    }
    const h: number[] = [];
    const prefix: number[] = [0];
    els.forEach((el, i) => {
      h[i] = el.offsetHeight;
      prefix[i + 1] = prefix[i] + h[i];
    });
    st.els = els;
    st.inners = els.map((el) => el.firstElementChild as HTMLElement | null);
    st.heights = h;
    st.prefix = prefix;
    st.boxH = box.clientHeight;
    st.measured = true;
    box.setAttribute("data-lyrics-ready", "1");
    return true;
  }

  function clampOffset(st: EngineState, offset: number, focus: number): number {
    const n = st.heights.length;
    if (n === 0) return 0;
    const f = Math.min(Math.max(focus, 0), n - 1);
    const focalCenter = st.prefix[f] + st.heights[f] / 2;
    const firstCenter = st.prefix[0] + st.heights[0] / 2;
    const lastCenter = st.prefix[n - 1] + st.heights[n - 1] / 2;
    return Math.min(lastCenter - focalCenter, Math.max(firstCenter - focalCenter, offset));
  }

  function stopInertia() {
    if (inertiaRafRef.current) {
      cancelAnimationFrame(inertiaRafRef.current);
      inertiaRafRef.current = 0;
    }
  }

  function endInteraction(st: EngineState) {
    st.interacting = "none";
    st.lastInteractEnd = performance.now();
    containerRef.current?.classList.remove("grabbing");
  }

  function interactionStart(st: EngineState, type: Interaction) {
    if (st.interacting === "none") {
      // 冻结当前焦点，挂起自动跟随
      st.suspended = true;
      st.frozenFocus = Math.max(0, st.lastIdx);
    }
    st.interacting = type;
  }

  function resetScroll() {
    const st = stRef.current;
    stopInertia();
    window.clearTimeout(wheelTimerRef.current);
    st.offset = 0;
    st.suspended = false;
    st.interacting = "none";
    st.drag = null;
    // seek 后那一两帧不做阶梯延迟（seek 后禁用级联）
    st.noStaggerFrames = 2;
    containerRef.current?.classList.remove("grabbing");
  }

  // 歌词变化：重建弹簧与测量，行从底部远处飞入（载入飞入）
  useLayoutEffect(() => {
    const st = stRef.current;
    st.measured = false;
    stopInertia();
    window.clearTimeout(wheelTimerRef.current);
    st.offset = 0;
    st.suspended = false;
    st.interacting = "none";
    st.lastIdx = -1;
    // 先丢掉旧元素缓存：切歌后 React 已换 DOM，继续写旧节点会叠出双份歌词
    st.els = [];
    st.inners = [];
    st.springs = [];
    st.scaleSprings = [];
    st.heights = [];
    st.prefix = [0];
    st.lastY = [];
    st.lastScale = [];
    st.lastBlur = [];
    st.reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    containerRef.current?.setAttribute("data-lyrics-ready", "0");
    setActiveIdx(-1);
    if (!optsRef.current.synced || linesKey.length === 0) return;
    if (!measure()) return;
    const n = linesKey.length;
    // 首帧同步排版：直接算出焦点基线，避免渲染出一帧堆叠在顶部的行
    const idx = findLrcIndex(linesKey, optsRef.current.getTimeMs());
    st.lastIdx = idx;
    setActiveIdx(idx);
    const focus = Math.min(Math.max(0, idx), n - 1);
    const base = st.boxH * ALIGN_POS - (st.prefix[focus] + st.heights[focus] / 2);
    st.springs = linesKey.map((_, i) => {
      const s = new Spring(0, POS_Y_PARAMS);
      // 正常：从底部远处弹簧飞入（载入飞入）；减少动态：直接到位
      const start = st.reducedMotion ? base + st.prefix[i] : st.boxH * 1.5 + i * 40;
      s.setPosition(start);
      const el = st.els[i];
      if (el) el.style.transform = `translateY(${start.toFixed(1)}px)`;
      return s;
    });
    st.scaleSprings = linesKey.map(() => new Spring(1, SCALE_PARAMS));
    st.lastY = linesKey.map((_, i) => st.springs[i].getCurrentPosition());
    st.lastScale = linesKey.map(() => 1);
    st.lastBlur = linesKey.map(() => Number.NaN);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [linesKey, opts.synced, opts.rebuildKey]);

  // 排版参数（字号/字体/行距）变化：只重测行高与前缀和，不重放载入动效
  useLayoutEffect(() => {
    const st = stRef.current;
    if (!opts.synced || linesKey.length === 0 || !st.measured) return;
    if (!measure()) return;
    st.offset = clampOffset(
      st,
      st.offset,
      st.suspended ? st.frozenFocus : Math.max(0, st.lastIdx),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [opts.layoutKey]);

  // 容器尺寸变化：重测量（换行高度会变）+ 重新钳制 offset
  useEffect(() => {
    const box = containerRef.current;
    if (!box) return;
    const st = stRef.current;
    const ro = new ResizeObserver(() => {
      if (!st.measured) return;
      const oldH = st.boxH;
      measure();
      if (st.boxH !== oldH) {
        st.offset = clampOffset(st, st.offset, st.suspended ? st.frozenFocus : Math.max(0, st.lastIdx));
      }
    });
    ro.observe(box);
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [linesKey, opts.synced]);

  // 滚动引擎：滚轮 / 拖拽 / 惯性
  useEffect(() => {
    const box = containerRef.current;
    if (!box || !opts.synced) return;
    const st = stRef.current;

    const onWheel = (e: WheelEvent) => {
      if (!st.measured) return;
      e.preventDefault();
      window.clearTimeout(wheelTimerRef.current);
      stopInertia();
      interactionStart(st, "wheel");
      const delta = e.deltaMode === WheelEvent.DOM_DELTA_PIXEL ? e.deltaY : e.deltaY * 50;
      st.offset = clampOffset(st, st.offset + delta, st.frozenFocus);
      wheelTimerRef.current = window.setTimeout(() => {
        wheelTimerRef.current = 0;
        endInteraction(st);
      }, WHEEL_IDLE_MS);
    };

    const onPointerDown = (e: PointerEvent) => {
      if (e.button !== 0 || !st.measured) return;
      stopInertia();
      window.clearTimeout(wheelTimerRef.current);
      st.drag = {
        startY: e.clientY,
        startOffset: st.offset,
        lastY: e.clientY,
        lastT: performance.now(),
        v: 0,
        moved: false,
        pointerId: e.pointerId,
      };
      // 注意：这里不能立即 setPointerCapture——捕获后 pointerup 的 target
      // 会被重定向到容器，点击句子的 closest("[data-i]") 就永远落空了
    };

    const onPointerMove = (e: PointerEvent) => {
      const d = st.drag;
      if (!d) return;
      const dy = d.startY - e.clientY;
      if (!d.moved) {
        if (Math.abs(dy) < INTENT_PX) return;
        d.moved = true;
        interactionStart(st, "drag");
        box.classList.add("grabbing");
        // 确认是拖拽后才捕获指针，保证移出窗口仍能跟踪；点击路径不受影响
        try {
          box.setPointerCapture(d.pointerId);
        } catch {
          /* 指针已释放等极端情况，忽略 */
        }
      }
      st.offset = clampOffset(st, d.startOffset + dy, st.frozenFocus);
      const now = performance.now();
      const dt = now - d.lastT;
      if (dt > 0) d.v = (e.clientY - d.lastY) / dt;
      d.lastY = e.clientY;
      d.lastT = now;
    };

    const onPointerUp = (e: PointerEvent) => {
      const d = st.drag;
      if (!d) return;
      st.drag = null;
      if (!d.moved) {
        // 点击句子 → 跳转（AM 操作逻辑）
        const lineEl = (e.target as Element | null)?.closest?.("[data-i]");
        if (lineEl && box.contains(lineEl)) {
          const i = Number(lineEl.getAttribute("data-i"));
          const line = optsRef.current.lines[i];
          if (line) optsRef.current.onSeekLine(i, line.timeMs);
        }
        return;
      }
      // 松手惯性：初速够才起滑，摩擦按帧归一衰减
      if (Math.abs(d.v) > INERTIA_MIN_V) {
        st.interacting = "inertia";
        let v = d.v;
        let lastT = performance.now();
        const step = (now: number) => {
          const dt = now - lastT;
          lastT = now;
          if (dt <= 0 || dt > 100) {
            inertiaRafRef.current = requestAnimationFrame(step);
            return;
          }
          if (Math.abs(v) > INERTIA_MIN_V) {
            st.offset = clampOffset(st, st.offset - v * dt, st.frozenFocus);
            v *= INERTIA_FRICTION ** (dt / (1000 / 60));
            inertiaRafRef.current = requestAnimationFrame(step);
          } else {
            inertiaRafRef.current = 0;
            endInteraction(st);
          }
        };
        inertiaRafRef.current = requestAnimationFrame(step);
      } else {
        endInteraction(st);
      }
    };

    /** 系统打断（触摸被抢/窗口失焦）：只收尾，绝不触发点击跳转 */
    const onPointerCancel = () => {
      const d = st.drag;
      st.drag = null;
      if (d?.moved) endInteraction(st);
    };

    box.addEventListener("wheel", onWheel, { passive: false });
    box.addEventListener("pointerdown", onPointerDown);
    box.addEventListener("pointermove", onPointerMove);
    box.addEventListener("pointerup", onPointerUp);
    box.addEventListener("pointercancel", onPointerCancel);
    return () => {
      box.removeEventListener("wheel", onWheel);
      box.removeEventListener("pointerdown", onPointerDown);
      box.removeEventListener("pointermove", onPointerMove);
      box.removeEventListener("pointerup", onPointerUp);
      box.removeEventListener("pointercancel", onPointerCancel);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [linesKey, opts.synced]);

  // 主循环：时钟 → 焦点 → 排版目标 → 弹簧 → 直写 style
  useEffect(() => {
    if (!opts.synced || linesKey.length === 0) return;
    const st = stRef.current;
    let raf = 0;
    let lastT = performance.now();

    const tick = (now: number) => {
      raf = requestAnimationFrame(tick);
      const dtS = Math.min((now - lastT) / 1000, MAX_FRAME_S);
      lastT = now;
      if (dtS <= 0 || !st.measured) return;
      const box = containerRef.current;
      if (!box) return;

      const { getTimeMs } = optsRef.current;
      const lines = optsRef.current.lines;
      // 以已测量缓存为准，防止 lines 与 st.els 短暂不一致时写到残留节点
      const n = st.els.length;
      const ms = getTimeMs();
      const idx = findLrcIndex(lines, ms);
      if (idx !== st.lastIdx) {
        st.lastIdx = idx;
        setActiveIdx(idx);
      }

      // 自动跟随恢复：静止够久且唱到新句，或静止超 6s 强制回跟
      if (st.suspended && st.interacting === "none") {
        const idle = now - st.lastInteractEnd;
        if ((idle >= RESUME_AFTER_MS && idx !== st.frozenFocus) || idle >= RESUME_FORCE_MS) {
          st.suspended = false;
          st.offset = 0;
        }
      }

      const focus = st.suspended
        ? Math.min(st.frozenFocus, n - 1)
        : Math.min(Math.max(0, idx), n - 1);
      st.offset = clampOffset(st, st.offset, focus);

      const H = st.boxH || box.clientHeight;
      const focalCenter = st.prefix[focus] + st.heights[focus] / 2;
      const base = H * ALIGN_POS - focalCenter - st.offset;

      const continuous = st.interacting === "drag" || st.interacting === "inertia";
      // 只有纯播放推进才开阶梯级联；滚轮/拖拽/seek 后都禁用
      const stagger = st.interacting === "none" && st.noStaggerFrames <= 0 && !st.reducedMotion;
      if (st.noStaggerFrames > 0) st.noStaggerFrames--;

      const narrow = window.innerWidth <= NARROW_WIDTH;
      const blurBase = continuous ? 0 : -1; // 拖拽/惯性中全部去模糊

      let delay = 0;
      let stepDelay = stagger ? STAGGER_BASE_S : 0;

      for (let i = 0; i < n; i++) {
        const el = st.els[i];
        if (!el) continue;
        const spring = st.springs[i];
        const targetY = base + st.prefix[i];

        if (continuous || st.reducedMotion) {
          spring.setPosition(targetY);
        } else if (Math.abs(spring.getTargetPosition() - targetY) >= 0.5) {
          spring.setTarget(targetY, delay);
        }
        spring.update(dtS);
        const y = spring.getCurrentPosition();
        if (Math.abs(y - st.lastY[i]) >= 0.01 || Number.isNaN(st.lastY[i])) {
          st.lastY[i] = y;
          el.style.transform = `translateY(${y.toFixed(1)}px)`;
        }

        // 缩放：仅看是否当前句。暂停不再整表回 1，避免播放/暂停时行尺寸跳动
        const scaleSpring = st.scaleSprings[i];
        const targetScale = i === idx ? 1 : 0.97;
        if (st.reducedMotion) scaleSpring.setPosition(targetScale);
        else if (Math.abs(scaleSpring.getTargetPosition() - targetScale) >= 1e-4)
          scaleSpring.setTarget(targetScale);
        scaleSpring.update(dtS);
        const s = scaleSpring.getCurrentPosition();
        if (Math.abs(s - st.lastScale[i]) >= 0.0005 || Number.isNaN(st.lastScale[i])) {
          st.lastScale[i] = s;
          const inner = st.inners[i];
          if (inner) inner.style.transform = `scale(${s.toFixed(4)})`;
        }

        // 距离模糊：焦点 0，其余 1+行距封顶 maxBlur；拖拽中全 0；窄窗八折
        let blur: number;
        if (blurBase === 0 || i === idx) blur = 0;
        else {
          const dist = Math.abs(i - Math.max(0, idx));
          const maxBlur = optsRef.current.maxBlur ?? BLUR_MAX;
          blur = Math.min(maxBlur, 1 + dist) * (narrow ? 0.8 : 1);
        }
        if (Math.abs(blur - st.lastBlur[i]) >= 0.05 || Number.isNaN(st.lastBlur[i])) {
          st.lastBlur[i] = blur;
          el.style.filter = blur > 0.01 ? `blur(${blur.toFixed(2)}px)` : "none";
        }

        // 阶梯延迟：行底过了顶边才累加；当前句之后逐渐收紧
        if (targetY + st.heights[i] >= 0) {
          delay += stepDelay;
          if (i >= idx) stepDelay *= STAGGER_DECAY;
        }
      }
    };

    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [linesKey, opts.synced]);

  // 卸载清理
  useEffect(
    () => () => {
      stopInertia();
      window.clearTimeout(wheelTimerRef.current);
    },
    [],
  );

  return { containerRef, activeIdx, resetScroll };
}
