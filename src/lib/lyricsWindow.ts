import { emit, emitTo, listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

import type { LyricsTarget } from "./types";

export const LYRICS_WINDOW_LABEL = "lyrics-search";
export const LYRICS_EVT_TARGET = "lyrics://target";
export const LYRICS_EVT_SAVED = "lyrics://saved";
/** 子窗口入口 query（只标窗口类型，不带曲目信息） */
export const LYRICS_WIN_QUERY = "win=lyrics";

/**
 * 打开独立「搜索歌词」子窗口。已存在则聚焦并换目标曲目。
 * 真系统窗口，可拖出主应用、放到其他显示器。
 * 曲目参数经 IPC/事件传递，不进子窗口 URL（避免路径进历史/崩溃转储）。
 */
export async function openLyricsWindow(target: LyricsTarget): Promise<void> {
  const existing = await WebviewWindow.getByLabel(LYRICS_WINDOW_LABEL);
  if (existing) {
    await existing.setFocus();
    await emitTo(LYRICS_WINDOW_LABEL, LYRICS_EVT_TARGET, target);
    return;
  }

  // 先落 pending，子窗口挂载后 take，再无竞态丢参
  await invoke("set_pending_lyrics_target", { target });
  const win = new WebviewWindow(LYRICS_WINDOW_LABEL, {
    url: `index.html?${LYRICS_WIN_QUERY}`,
    title: "搜索歌词",
    width: 920,
    height: 700,
    minWidth: 720,
    minHeight: 520,
    center: true,
    resizable: true,
    decorations: false,
    focus: true,
  });

  await new Promise<void>((resolve, reject) => {
    const offCreated = win.once("tauri://created", () => {
      void offErr.then((f) => f());
      resolve();
    });
    const offErr = win.once("tauri://error", (e) => {
      void offCreated.then((f) => f());
      reject(e.payload ?? new Error("创建歌词窗口失败"));
    });
  });
}

/**
 * 子窗口保存歌词后广播；主窗口/管理页据此刷新。
 * 用全局 emit（所有窗口可收），比 emitTo 定向更稳——子窗口关闭竞态时事件不会丢。
 * Rust 侧写入成功后也会广播同名事件；监听方需容忍短时间重复。
 */
export function emitLyricsSaved(target: LyricsTarget): void {
  void emit(LYRICS_EVT_SAVED, target);
}

/** 订阅歌词已保存（主窗口侧刷新用）。Rust/JS 可能各广播一次，做短去抖避免双刷 */
export function onLyricsSaved(cb: (t: LyricsTarget) => void): () => void {
  let un: (() => void) | undefined;
  let cancelled = false;
  let timer = 0;
  void listen<LyricsTarget>(LYRICS_EVT_SAVED, (e) => {
    if (cancelled) return;
    window.clearTimeout(timer);
    const payload = e.payload;
    timer = window.setTimeout(() => {
      timer = 0;
      if (!cancelled) cb(payload);
    }, 40);
  }).then((f) => {
    if (cancelled) f();
    else un = f;
  });
  return () => {
    cancelled = true;
    window.clearTimeout(timer);
    un?.();
  };
}

/** 子窗口内订阅切换目标曲目 */
export function onLyricsTarget(cb: (t: LyricsTarget) => void): () => void {
  let un: (() => void) | undefined;
  let cancelled = false;
  void listen<LyricsTarget>(LYRICS_EVT_TARGET, (e) => cb(e.payload)).then((f) => {
    if (cancelled) f();
    else un = f;
  });
  return () => {
    cancelled = true;
    un?.();
  };
}

/** 子窗口冷启动：取走主窗口预置的目标曲目（只取一次）。 */
export async function takePendingLyricsTarget(): Promise<LyricsTarget | null> {
  try {
    return await invoke<LyricsTarget | null>("take_pending_lyrics_target");
  } catch {
    return null;
  }
}
