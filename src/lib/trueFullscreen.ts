import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

const appWindow = getCurrentWindow();

/**
 * 真全屏（覆盖任务栏）。
 *
 * Windows 上游 bug（tao 未修）：最大化窗口直接全屏，tao 重建样式时会把
 * WS_MAXIMIZE 加回，窗口被夹在工作区，任务栏区域露出一条黑边。
 * 规避在 Rust 侧 enter/exit_true_fullscreen：原位清最大化态（几何不动）、
 * 钉住还原位置、全程关 DWM 过渡动画 —— 进出无缩放动画也不闪桌面。
 */
let pending: Promise<void> = Promise.resolve();
let restoreMaximized = false;

/** 串行化，避免双击/Esc 连打时状态交错 */
function serialize(job: () => Promise<void>): Promise<void> {
  const run = pending.then(job, job);
  pending = run;
  return run;
}

async function doEnter(): Promise<void> {
  const wasMaximized = await invoke<boolean>("enter_true_fullscreen");
  if (wasMaximized) restoreMaximized = true;
}

async function doExit(): Promise<void> {
  const restore = restoreMaximized;
  restoreMaximized = false;
  await invoke("exit_true_fullscreen", { restoreMaximized: restore });
}

export function enterTrueFullscreen(): Promise<void> {
  return serialize(doEnter);
}

export function exitTrueFullscreen(): Promise<void> {
  return serialize(doExit);
}

export function toggleTrueFullscreen(): Promise<void> {
  return serialize(async () => {
    if (await appWindow.isFullscreen()) await doExit();
    else await doEnter();
  });
}
