import { emitTo, listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

import type { LyricsTarget } from "./types";

export const LYRICS_WINDOW_LABEL = "lyrics-search";
export const LYRICS_EVT_TARGET = "lyrics://target";
export const LYRICS_EVT_SAVED = "lyrics://saved";

/** 把目标曲目编进 URL，子窗口冷启动时解析（避开跨窗口时序） */
export function lyricsTargetToUrl(target: LyricsTarget): string {
  const qs = new URLSearchParams({
    win: "lyrics",
    id: String(target.id),
    path: target.path,
    title: target.title,
    artist: target.artist,
    filename: target.filename,
  });
  return `index.html?${qs.toString()}`;
}

export function parseLyricsTargetFromLocation(): LyricsTarget | null {
  const q = new URLSearchParams(window.location.search);
  if (q.get("win") !== "lyrics") return null;
  const path = q.get("path") ?? "";
  if (!path) return null;
  return {
    id: Number(q.get("id") ?? 0) || 0,
    path,
    title: q.get("title") ?? "",
    artist: q.get("artist") ?? "",
    filename: q.get("filename") ?? "",
  };
}

/**
 * 打开独立「搜索歌词」子窗口。已存在则聚焦并换目标曲目。
 * 真系统窗口，可拖出主应用、放到其他显示器。
 */
export async function openLyricsWindow(target: LyricsTarget): Promise<void> {
  const existing = await WebviewWindow.getByLabel(LYRICS_WINDOW_LABEL);
  if (existing) {
    await existing.setFocus();
    await emitTo(LYRICS_WINDOW_LABEL, LYRICS_EVT_TARGET, target);
    return;
  }

  const win = new WebviewWindow(LYRICS_WINDOW_LABEL, {
    url: lyricsTargetToUrl(target),
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

/** 子窗口保存歌词后广播；主窗口/管理页据此刷新 */
export function emitLyricsSaved(target: LyricsTarget): void {
  void emitTo("main", LYRICS_EVT_SAVED, target);
  void emitTo(LYRICS_WINDOW_LABEL, LYRICS_EVT_SAVED, target);
}

/** 订阅歌词已保存（主窗口侧刷新用） */
export function onLyricsSaved(cb: (t: LyricsTarget) => void): () => void {
  let un: (() => void) | undefined;
  let cancelled = false;
  void listen<LyricsTarget>(LYRICS_EVT_SAVED, (e) => cb(e.payload)).then((f) => {
    if (cancelled) f();
    else un = f;
  });
  return () => {
    cancelled = true;
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
