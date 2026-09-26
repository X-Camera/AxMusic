import { lerpRgb } from "../sideViz";
import type { VizEffect, VizFrame } from "./types";
import { focusPoint, makeGlowSprite, rgba } from "./types";

/** 从 64 bin 平滑重采样到 n 档（线性插值） */
export function resampleLevels(levels: Float32Array, i: number, n: number): number {
  const pos = (i / Math.max(1, n - 1)) * (levels.length - 1);
  const j = Math.min(levels.length - 2, Math.floor(pos));
  const frac = pos - j;
  return levels[j] * (1 - frac) + levels[j + 1] * frac;
}

/**
 * 环形镜像采样：圆周 u∈[0,1) 映射频谱位置 0→1→0，
 * 闭合处（u=0/1）同为低频 bin0，左右对称、首尾连续无接缝；
 * idle 微波动也用镜像位置 p 驱动，保证 u 与 1-u 处一致。
 */
function ringLevel(f: VizFrame, u: number): number {
  const p = u < 0.5 ? u * 2 : 2 - u * 2;
  const pos = p * (f.levels.length - 1);
  const j = Math.min(f.levels.length - 2, Math.floor(pos));
  const v = f.levels[j] * (1 - (pos - j)) + f.levels[j + 1] * (pos - j);
  const idle = 0.2 + 0.22 * Math.sin(f.t * 1.7 + p * 9.4);
  return Math.min(1, v * 0.85 + idle * (f.live ? 0.12 : 0.5));
}

/** 参数化主环半径：radial_ex.radius 0..1 → min(w,h) 的 8%..30%（0.5 = 原 16%） */
function ringRadius(f: VizFrame): number {
  return Math.min(f.w, f.h) * (0.08 + f.settings.radial_ex.radius * 0.22);
}

/** 全屏氛围底：焦点处大半径径向渐变铺满整个歌词区，随低音呼吸 */
function drawAmbient(
  ctx: CanvasRenderingContext2D,
  f: VizFrame,
  cx: number,
  cy: number,
  pulse: number,
) {
  const R = Math.hypot(f.w, f.h) * 0.72;
  const k = 0.045 + f.settings.intensity * 0.065 + pulse * 0.045;
  const g = ctx.createRadialGradient(cx, cy, 0, cx, cy, R);
  g.addColorStop(0, rgba(f.colors.b, k * 1.5));
  g.addColorStop(0.5, rgba(f.colors.c, k * 0.6));
  g.addColorStop(1, "rgba(0,0,0,0)");
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, f.w, f.h);
}

/** 环柱：内外双向光柱 + 中心光晕 + 全屏氛围底（圆心固定当前句槽位） */
export function createRadialBars(): VizEffect {
  return {
    draw(f: VizFrame) {
      const { ctx, w, h, t, colors: cols, settings: s, beat, bass } = f;
      if (!ctx) return;
      const ex = s.radial_ex;
      const { x: cx, y: cy } = focusPoint(w, h, f.focus);
      const pulse = bass * 0.6 + beat * 0.4;

      drawAmbient(ctx, f, cx, cy, pulse);

      const baseR = ringRadius(f) * (1 + pulse * 0.06 * s.intensity);
      // 外伸：默认(0.5)≈原观感 1.7×baseR；内伸为外伸的比例
      const outK = 0.6 + ex.out_len * 2.2;
      const inK = 0.06 + ex.in_len * 0.48;
      const maxH = baseR * outK * (0.7 + s.intensity * 0.4 + pulse * 0.25);
      const n = 72;

      // 中心光盘
      const glow = ctx.createRadialGradient(cx, cy, 0, cx, cy, baseR * 1.35);
      glow.addColorStop(0, rgba(cols.a, 0.16 + s.intensity * 0.2 + pulse * 0.12));
      glow.addColorStop(0.55, rgba(cols.b, 0.08 + s.intensity * 0.1));
      glow.addColorStop(1, "rgba(0,0,0,0)");
      ctx.fillStyle = glow;
      ctx.beginPath();
      ctx.arc(cx, cy, baseR * 1.35, 0, Math.PI * 2);
      ctx.fill();

      // 内环线
      ctx.beginPath();
      ctx.arc(cx, cy, baseR, 0, Math.PI * 2);
      ctx.strokeStyle = rgba(cols.a, 0.22 + s.intensity * 0.25 + pulse * 0.15);
      ctx.lineWidth = 1.2;
      ctx.stroke();

      const glowOn = s.intensity > 0.55;
      if (glowOn) {
        ctx.shadowColor = rgba(cols.b, 0.45);
        ctx.shadowBlur = 8 * s.intensity;
      }
      ctx.lineCap = "round";
      for (let i = 0; i < n; i++) {
        const ang = (i / n) * Math.PI * 2 - Math.PI / 2 + t * 0.08 * s.speed;
        const v = ringLevel(f, i / n);
        const col = lerpRgb(cols.a, cols.b, i / (n - 1));
        const outH = 3 + v * maxH;
        const inH = 2 + v * maxH * inK;
        const cos = Math.cos(ang);
        const sin = Math.sin(ang);
        ctx.beginPath();
        ctx.moveTo(cx + cos * (baseR - inH), cy + sin * (baseR - inH));
        ctx.lineTo(cx + cos * (baseR + outH), cy + sin * (baseR + outH));
        ctx.strokeStyle = rgba(col, 0.35 + v * (0.5 + s.intensity * 0.35));
        ctx.lineWidth = Math.max(1.5, ((Math.PI * 2 * baseR) / n) * 0.42);
        ctx.stroke();
      }
      ctx.shadowBlur = 0;
    },
  };
}

/** 确定性伪随机 */
function frac(n: number) {
  return n - Math.floor(n);
}

interface Branch {
  /** 圆周位置 0..1 */
  u: number;
  /** 出生时刻（秒） */
  born: number;
  /** 3 段折线的角度/长度扰动种子 */
  seed: number;
}

/** 内环辐射出的火花粒子 */
interface Spark {
  ang: number;
  r: number;
  /** 径向速度（指数衰减，先快后慢） */
  vr: number;
  born: number;
  life: number;
  size: number;
  /** 颜色 sprite 索引：按频段固定（低/中/高频 → a/b/c） */
  sprite: number;
}

/**
 * 环线（闪电）：随机游走锯齿电弧——形状保持 ~150ms 后「重击」换形（非逐帧抽搐），
 * 节拍立刻触发重击；白热细芯 + 彩色辉光底描 + 前两帧残影 + 强拍时向外分叉。
 */
export function createRadialLine(): VizEffect {
  const N = 160;
  let boltTarget = new Float32Array(N + 1);
  let boltCur = new Float32Array(N + 1);
  let ghosts: Float32Array[] = [];
  let branches: Branch[] = [];
  let sparks: Spark[] = [];
  let sparkSprites: HTMLCanvasElement[] = [];
  let sparkSpriteKey = "";
  /** 16 扇区的跟随基线（onset 检测）与触发冷却 */
  const SECTORS = 16;
  let bandBase = new Float32Array(SECTORS);
  let sectorCd = new Float32Array(SECTORS);
  let bandsPrimed = false;
  let lastRegen = -10;
  let beatLatch = false;

  /** 生成新一道闪电形状（闭合随机游走 + 稀疏尖刺，归一到 [-1,1]） */
  function regenBolt() {
    ghosts.unshift(boltCur.slice(0, N + 1));
    if (ghosts.length > 2) ghosts.pop();
    let x = 0;
    for (let i = 0; i < N; i++) {
      x += (Math.random() - 0.5) * 1.1;
      // 稀疏 V 形尖刺
      if (Math.random() < 0.055) x += (Math.random() - 0.5) * 3.2;
      boltTarget[i] = x;
    }
    // 闭合修正：把漂移量线性摊回整圈，保证首尾同值
    for (let i = 0; i < N; i++) boltTarget[i] -= (x * i) / N;
    boltTarget[N] = boltTarget[0];
    let mx = 0.2;
    for (let i = 0; i <= N; i++) mx = Math.max(mx, Math.abs(boltTarget[i]));
    for (let i = 0; i <= N; i++) boltTarget[i] /= mx;
  }
  regenBolt();
  boltCur.set(boltTarget);

  return {
    draw(f: VizFrame) {
      const { ctx, w, h, t, colors: cols, settings: s, beat, bass, dt } = f;
      if (!ctx) return;
      const ex = s.radial_ex;
      const { x: cx, y: cy } = focusPoint(w, h, f.focus);
      const pulse = bass * 0.6 + beat * 0.4;

      drawAmbient(ctx, f, cx, cy, pulse);

      const baseR = ringRadius(f) * (1 + pulse * 0.05 * s.intensity);
      const amp = baseR * (0.3 + ex.out_len * 1.8) * (0.7 + s.intensity * 0.5 + pulse * 0.2);
      // 双层异速：辉光层慢速正向，白热芯略快且带固定相位差，两层像缠绕的电弧
      const rotGlow = t * 0.1 * s.speed;
      const rotCore = t * 0.145 * s.speed + 0.35;

      // 闪电重击：节拍立刻换形，否则每 ~150ms 自发一次（低音越重越频繁）
      const rising = beat > 0.85 && !beatLatch;
      beatLatch = beat > 0.85;
      const interval = f.live ? 0.24 - bass * 0.14 : 0.4;
      if (rising || t - lastRegen > interval) {
        regenBolt();
        lastRegen = t;
        if (rising && f.live) {
          // 强拍分叉：1-3 条向外短电弧
          const nb = 1 + Math.floor(Math.random() * 2.5);
          for (let k = 0; k < nb; k++) {
            branches.push({ u: Math.random(), born: t, seed: Math.random() * 100 });
          }
        }
      }
      branches = branches.filter((b) => t - b.born < 0.22);

      // 当前形状快速靠拢目标（~70ms 完成，闪电「啪」地换形而非抖动）
      const snap = Math.min(1, dt / 0.07);
      for (let i = 0; i <= N; i++) boltCur[i] += (boltTarget[i] - boltCur[i]) * snap;

      const boltR = (i: number, bolt: Float32Array, shrink: number) => {
        const u = i / N;
        const lvl = ringLevel(f, u);
        const zz = (bolt[i] + 1) * 0.5;
        return (baseR + (lvl * (0.4 + 0.6 * zz) + pulse * 0.08) * amp) * shrink;
      };
      const traceBolt = (bolt: Float32Array, shrink: number, rot: number) => {
        ctx!.beginPath();
        for (let i = 0; i <= N; i++) {
          const u = i / N;
          const ang = u * Math.PI * 2 - Math.PI / 2 + rot;
          const r = boltR(i, bolt, shrink);
          const x = cx + Math.cos(ang) * r;
          const y = cy + Math.sin(ang) * r;
          if (i === 0) ctx!.moveTo(x, y);
          else ctx!.lineTo(x, y);
        }
        ctx!.closePath();
      };

      // 残影：前两道闪电收缩变淡（像视网膜余像；跟随辉光层相位）
      ghosts.forEach((g, k) => {
        traceBolt(g, 1 - (k + 1) * 0.07, rotGlow - (k + 1) * 0.06);
        ctx.strokeStyle = rgba(cols.c, 0.12 / (k + 1) + pulse * 0.03);
        ctx.lineWidth = 1;
        ctx.lineJoin = "miter";
        ctx.stroke();
      });

      // 分叉电弧：3 段折线向外（从辉光层弧上甩出）
      for (const b of branches) {
        const age = (t - b.born) / 0.22;
        const alpha = (1 - age) * (0.4 + s.intensity * 0.3);
        const i0 = Math.round(b.u * N);
        const ang0 = b.u * Math.PI * 2 - Math.PI / 2 + rotGlow;
        let r0 = boltR(i0, boltCur, 1);
        let a0 = ang0;
        ctx.beginPath();
        ctx.moveTo(cx + Math.cos(a0) * r0, cy + Math.sin(a0) * r0);
        for (let seg = 0; seg < 3; seg++) {
          const stepLen = amp * (0.32 - seg * 0.07) * (0.6 + frac(b.seed + seg * 0.37) * 0.8);
          a0 += (frac(b.seed * 1.7 + seg * 0.71) - 0.5) * 0.5;
          r0 += stepLen;
          ctx.lineTo(cx + Math.cos(a0) * r0, cy + Math.sin(a0) * r0);
        }
        ctx.strokeStyle = rgba(lerpRgb(cols.b, [255, 255, 255], 0.4), alpha);
        ctx.lineWidth = 1;
        ctx.stroke();
      }

      // 主电弧底描：彩色辉光（细，主要提供光晕而非线宽）
      traceBolt(boltCur, 1, rotGlow);
      const lg = ctx.createLinearGradient(cx - baseR * 2, cy, cx + baseR * 2, cy);
      lg.addColorStop(0, rgba(cols.a, 0.45));
      lg.addColorStop(0.5, rgba(lerpRgb(cols.a, cols.b, 0.5), 0.9));
      lg.addColorStop(1, rgba(cols.c, 0.5));
      ctx.strokeStyle = lg;
      ctx.lineWidth = 1.3 + s.intensity * 0.6 + pulse * 0.5;
      ctx.lineJoin = "miter";
      ctx.shadowColor = rgba(cols.b, 0.65 * s.intensity + 0.2);
      ctx.shadowBlur = 13 * (0.35 + s.intensity);
      ctx.stroke();
      ctx.shadowBlur = 0;

      // 白热主芯：视觉主体，粗而亮（相位错开、转速略快）
      traceBolt(boltCur, 1, rotCore);
      ctx.strokeStyle = rgba(
        lerpRgb(cols.a, [255, 255, 255], 0.75),
        0.72 + s.intensity * 0.24 + pulse * 0.15,
      );
      ctx.lineWidth = 1.9 + s.intensity * 0.8 + pulse * 0.4;
      ctx.stroke();

      // 内环
      ctx.beginPath();
      ctx.arc(cx, cy, baseR * 0.55, 0, Math.PI * 2);
      ctx.strokeStyle = rgba(cols.a, 0.14 + pulse * 0.12);
      ctx.lineWidth = 1;
      ctx.stroke();

      // 内环火花辐射：分频段 onset 触发——某频段能量上跳一次，就在对应扇区喷一波
      const emit = Math.round(ex.emit);
      if (emit > 0) {
        const cKey = `${cols.a}|${cols.b}|${cols.c}`;
        if (cKey !== sparkSpriteKey) {
          sparkSpriteKey = cKey;
          sparkSprites = [cols.a, cols.b, cols.c].map((c) => makeGlowSprite(c));
        }
        // 触发阈值由灵敏度决定：0.5 → 0.15，拉满 0.04 近乎拨草寻蛇
        const threshold = 0.26 - ex.sensitivity * 0.22;
        for (let sec = 0; sec < SECTORS; sec++) {
          const u = sec / SECTORS;
          // 与闪电同一镜像映射：扇区能量对应圆周位置
          const p = u < 0.5 ? u * 2 : 2 - u * 2;
          const pos = p * (f.levels.length - 1);
          const j = Math.min(f.levels.length - 2, Math.floor(pos));
          const lvl = f.levels[j] * (1 - (pos - j)) + f.levels[j + 1] * (pos - j);
          const base = bandBase[sec];
          if (bandsPrimed && lvl > base + threshold && lvl > 0.12 && t - sectorCd[sec] > 0.15) {
            sectorCd[sec] = t;
            const over = lvl - base - threshold; // 超出越多喷得越猛
            const burst = Math.max(3, Math.round(emit * (0.1 + over * 0.9)));
            // 颜色按频段固定：低/中/高频扇区各取 a/b/c，不随机
            const sprite = p < 1 / 3 ? 0 : p < 2 / 3 ? 1 : 2;
            for (let k = 0; k < burst && sparks.length < emit; k++) {
              sparks.push({
                // 扇区角 ± 微小偏差（≈±2°），走直线
                ang:
                  (u + (Math.random() - 0.5) * 0.035) * Math.PI * 2 - Math.PI / 2 + rotGlow,
                r: baseR * 0.55,
                // 初速随机 + 超出越多喷得越快
                vr: baseR * (2.4 + Math.random() * 2.6 + over * 2.5),
                born: t,
                life: 0.55 + Math.random() * 0.6,
                size: 1.4 + Math.random() * 2.6,
                sprite,
              });
            }
          }
          // 基线慢速跟随（上慢下快）：持续大音量不会连发，只认「新突起」
          bandBase[sec] += (lvl - base) * Math.min(1, dt * (lvl > base ? 3.5 : 9));
        }
        bandsPrimed = true;
        const maxR = Math.hypot(w, h) * 0.6;
        const drag = Math.exp(-2.8 * dt); // 指数减速：先快后慢
        ctx.globalCompositeOperation = "lighter";
        sparks = sparks.filter((sp) => {
          sp.vr *= drag;
          sp.r += sp.vr * dt;
          const age = t - sp.born;
          if (age > sp.life || sp.r > maxR) return false;
          const k = age / sp.life;
          ctx.globalAlpha = (1 - k) * (0.7 + s.intensity * 0.3);
          const d = sp.size * (4 - k * 2.2) * (0.8 + s.intensity * 0.5);
          const px = cx + Math.cos(sp.ang) * sp.r - d / 2;
          const py = cy + Math.sin(sp.ang) * sp.r - d / 2;
          ctx.drawImage(sparkSprites[sp.sprite], px, py, d, d);
          return true;
        });
        ctx.globalAlpha = 1;
        ctx.globalCompositeOperation = "source-over";
      } else if (sparks.length) {
        sparks = [];
      }
    },
  };
}
