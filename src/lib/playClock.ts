/** 播放进度时钟：轮询 position 作锚点，播放中按墙钟外推，供 rAF 平滑绘制。 */

export type PlayClock = {
  baseMs: number;
  baseAt: number;
  playing: boolean;
};

export function createPlayClock(): PlayClock {
  return { baseMs: 0, baseAt: 0, playing: false };
}

export function clockNow(c: PlayClock): number {
  if (!c.playing) return c.baseMs;
  return c.baseMs + (performance.now() - c.baseAt);
}

export function clockReanchor(c: PlayClock, ms: number, playing: boolean): void {
  c.baseMs = ms;
  c.baseAt = performance.now();
  c.playing = playing;
}

/**
 * 用轮询快照校正时钟。
 * 播放中小漂移不重新锚定（避免每 500ms 微跳），大跳变（seek/切歌）才对齐。
 */
export function clockSyncFromSnapshot(
  c: PlayClock,
  posMs: number,
  playing: boolean,
  opts?: { force?: boolean; driftOkMs?: number },
): void {
  const driftOk = opts?.driftOkMs ?? 280;
  if (opts?.force || !playing) {
    clockReanchor(c, posMs, playing);
    return;
  }
  if (!c.playing) {
    clockReanchor(c, posMs, true);
    return;
  }
  const drift = Math.abs(clockNow(c) - posMs);
  if (drift > driftOk) {
    clockReanchor(c, posMs, true);
  }
}
