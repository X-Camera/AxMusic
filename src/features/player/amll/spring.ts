/**
 * 弹簧物理 — 移植自 amll 参考项目 utils/spring.ts（MIT，原案 pushkine）。
 * 解析解求解器：setTarget 后位置是时间的闭式函数，无需逐步积分，跳帧不失稳。
 * 秒制 float；速度用数值差分接续（替代原项目的解析导数，等效且更省）。
 */

export interface SpringParams {
  mass: number;
  damping: number;
  stiffness: number;
  /** 强制过阻尼（无过冲） */
  soft?: boolean;
}

/** 歌词行纵向位移：微过冲，AM 跟手感的来源 */
export const POS_Y_PARAMS: SpringParams = { mass: 0.9, damping: 15, stiffness: 90 };
/** 歌词行缩放：更沉一点，放大收敛慢半拍 */
export const SCALE_PARAMS: SpringParams = { mass: 2, damping: 25, stiffness: 100 };

type Seconds = number;

/** 解析解：from + 初速度 → to，返回位置关于时间的函数（t<0 时恒为 from）。 */
function solveSpring(
  from: number,
  velocity: number,
  to: number,
  params: SpringParams,
): (t: Seconds) => number {
  const { mass, damping, stiffness, soft } = params;
  const delta = to - from;
  // 参数守卫：mass/stiffness 必须为正（mass=0 会让 sqrt(k/m) 炸出 NaN 并扩散到渲染层）
  if (!(mass > 0) || !(stiffness > 0)) {
    console.warn("[spring] 非法参数（mass/stiffness 必须为正），直接贴目标", params);
    return (t) => (t < 0 ? from : to);
  }
  // 过阻尼（或 soft 强制）：临界阻尼解析式，指数衰减无振荡（与参考实现一致）
  if (soft || 1 <= damping / (2 * Math.sqrt(stiffness * mass))) {
    const angularFrequency = -Math.sqrt(stiffness / mass);
    const leftover = -angularFrequency * delta - velocity;
    return (t) => (t < 0 ? from : to - (delta + t * leftover) * Math.E ** (t * angularFrequency));
  }
  // 欠阻尼：衰减振荡，带回弹
  const dampingFrequency = Math.sqrt(4 * mass * stiffness - damping ** 2);
  const leftover = (damping * delta - 2 * mass * velocity) / dampingFrequency;
  const dfm = (0.5 * dampingFrequency) / mass;
  const dm = -(0.5 * damping) / mass;
  return (t) =>
    t < 0 ? from : to - (Math.cos(t * dfm) * delta + Math.sin(t * dfm) * leftover) * Math.E ** (t * dm);
}

export class Spring {
  private pos: number;
  private target: number;
  private t: Seconds = 0;
  private solver: (t: Seconds) => number;
  private lastV = 0;
  private pending: { target: number; inS: Seconds } | null = null;

  constructor(start = 0, private readonly params: SpringParams) {
    this.pos = start;
    this.target = start;
    this.solver = () => start;
  }

  /** 瞬移到指定位置（拖拽跟手 / 减少动态效果降级用）。 */
  setPosition(p: number): void {
    this.pos = p;
    this.target = p;
    this.t = 0;
    this.lastV = 0;
    this.pending = null;
    this.solver = () => p;
  }

  /** 弹簧去往目标；delayS > 0 时排队延迟生效（阶梯级联用）。 */
  setTarget(target: number, delayS: Seconds = 0): void {
    if (delayS > 0) {
      this.pending = { target, inS: delayS };
      return;
    }
    if (!this.pending && Math.abs(this.target - target) < 1e-4) return;
    this.pending = null;
    this.target = target;
    // 从当前位置与速度接续，动画不跳变
    this.solver = solveSpring(this.pos, this.lastV, target, this.params);
    this.t = 0;
  }

  update(dtS: Seconds): void {
    if (dtS <= 0) return;
    if (this.pending) {
      this.pending.inS -= dtS;
      if (this.pending.inS <= 0) {
        const p = this.pending.target;
        this.setTarget(p);
      }
    }
    this.t += dtS;
    const prev = this.pos;
    this.pos = this.solver(this.t);
    this.lastV = (this.pos - prev) / dtS;
    if (this.arrived()) this.setPosition(this.target);
  }

  arrived(): boolean {
    return (
      !this.pending && Math.abs(this.target - this.pos) < 0.05 && Math.abs(this.lastV) < 0.02
    );
  }

  getCurrentPosition(): number {
    return this.pos;
  }

  getTargetPosition(): number {
    return this.pending?.target ?? this.target;
  }
}
