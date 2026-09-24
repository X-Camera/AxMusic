import type { RepeatMode } from "./types";

/** 循环按钮三态轮转：关 → 列表循环 → 单曲循环（业界惯例；随机独立） */
export const REPEAT_CYCLE: readonly RepeatMode[] = ["off", "all", "one"];

export const REPEAT_TITLE: Record<RepeatMode, string> = {
  off: "循环关闭",
  all: "列表循环",
  one: "单曲循环",
};

export function nextRepeat(current: RepeatMode): RepeatMode {
  const i = REPEAT_CYCLE.indexOf(current);
  return REPEAT_CYCLE[(i + 1) % REPEAT_CYCLE.length];
}
