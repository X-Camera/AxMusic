import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

/**
 * 动效音频源：优先消费 Rust 推来的真实频谱（viz://spectrum，~30Hz），
 * 无数据（暂停/未订阅/超时 150ms）时无缝回退到模拟能量。
 * 进程级单例：频谱 levels 跨挂载保留，面板重开不从零起步。
 */

export const VIZ_BINS = 64;

export interface VizAudioState {
  /** 64 bin 平滑能量 0..1 */
  levels: Float32Array;
  /** 低频能量 0..1 */
  bass: number;
  /** 节拍脉冲 0..1（沿保持后衰减） */
  beat: number;
  /** 当前是否真实音频驱动 */
  live: boolean;
}

interface VizSpectrumPayload {
  bins: number[];
  bass: number;
  rms: number;
  beat: boolean;
}

class VizAudioSource {
  /** 输出（混合+平滑后） */
  readonly levels = new Float32Array(VIZ_BINS);
  private realBins = new Float32Array(VIZ_BINS);
  private simTargets = new Float32Array(VIZ_BINS);
  private realBass = 0;
  private bassOut = 0;
  private beatPulse = 0;
  private lastEventAt = 0;
  private beatHoldUntil = 0;
  /** 0=纯模拟 1=纯真实，渐变切换避免跳变 */
  private liveBlend = 0;
  private subscribed = false;
  private unlisten: (() => void) | null = null;

  setSubscribed(on: boolean) {
    if (on === this.subscribed) return;
    this.subscribed = on;
    if (on) {
      void (async () => {
        const fn = await listen<VizSpectrumPayload>("viz://spectrum", (e) => {
          if (!this.subscribed) return;
          const p = e.payload;
          const n = Math.min(VIZ_BINS, p.bins.length);
          for (let i = 0; i < n; i++) this.realBins[i] = p.bins[i];
          this.realBass = p.bass;
          this.lastEventAt = performance.now();
          if (p.beat) this.beatHoldUntil = performance.now() + 140;
        });
        if (this.subscribed) this.unlisten = fn;
        else fn();
        void invoke("viz_set_active", { active: true }).catch(() => undefined);
      })();
    } else {
      this.unlisten?.();
      this.unlisten = null;
      void invoke("viz_set_active", { active: false }).catch(() => undefined);
    }
  }

  /** 每 rAF 调用：t 秒、dt 秒；intensity 只影响模拟回退的摆幅 */
  frame(t: number, dt: number, playing: boolean, intensity: number): VizAudioState {
    const now = performance.now();
    const isLive = playing && now - this.lastEventAt < 150;
    this.liveBlend += ((isLive ? 1 : 0) - this.liveBlend) * Math.min(1, dt * 8);
    const lb = this.liveBlend;

    // 模拟目标（原 energy() 的 112BPM 正弦 + 分频摆动）
    const simBeat = playing
      ? Math.pow(Math.max(0, Math.sin(t * Math.PI * 2 * (112 / 60) * 0.5)), 3)
      : 0;
    const idle = 0.35 + 0.25 * Math.sin(t * 0.7);
    const swell = playing ? 0.55 + 0.45 * Math.sin(t * 0.9) : idle;
    for (let i = 0; i < VIZ_BINS; i++) {
      const f = i / VIZ_BINS;
      const tilt = Math.pow(1 - f, 1.15) * (0.35 + simBeat * 0.65 * intensity + swell * 0.35);
      const wob = Math.sin(t * (1.4 + f * 4.5) + i * 0.7) * (0.12 + intensity * 0.18);
      this.simTargets[i] = Math.min(1, Math.max(0.05, tilt + wob));
      const target = this.simTargets[i] * (1 - lb) + this.realBins[i] * lb;
      this.levels[i] += (target - this.levels[i]) * (playing ? 0.28 : 0.12);
    }

    // bass：真实用 Rust 值，模拟用节拍
    const bassT = this.realBass * lb + simBeat * (1 - lb);
    this.bassOut += (bassT - this.bassOut) * Math.min(1, dt * 10);

    // 节拍脉冲：真实 onset 沿保持 140ms；模拟直接取节拍波形
    const beatReal = now < this.beatHoldUntil ? 1 : 0;
    const beatT = beatReal * lb + simBeat * (1 - lb);
    this.beatPulse = Math.max(beatT, this.beatPulse - dt * 3.2);

    return {
      levels: this.levels,
      bass: this.bassOut,
      beat: Math.min(1, this.beatPulse),
      live: isLive,
    };
  }
}

let shared: VizAudioSource | null = null;

export function getVizAudioSource(): VizAudioSource {
  if (!shared) shared = new VizAudioSource();
  return shared;
}
