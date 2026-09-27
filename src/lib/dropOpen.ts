import { getCurrentWindow } from "@tauri-apps/api/window";

import { api, folderFileToQueueItem } from "./api";
import type { QueueItem } from "./types";
import { useApp } from "../state/useApp";

/** 与 QueuePanel 的 data-drop-zone 共享；改名必须两侧同步 */
export const QUEUE_DROP_ZONE = "queue";
const QUEUE_DROP_SELECTOR = `[data-drop-zone="${QUEUE_DROP_ZONE}"]`;

/** 拖到播放队列面板 = 追加；其它区域 = 替换队列并从头播放 */
type DropAction = "replace-play" | "enqueue";

/**
 * Tauri onDragDropEvent 的 position 为**物理像素**（相对窗口客户区）。
 * elementFromPoint / getBoundingClientRect 使用 CSS 像素，故除以 devicePixelRatio
 * （应等于当前窗口缩放比）。混合 DPI 跨屏时需实测；下方再用面板矩形兜底命中。
 */
function toCssPoint(pos: { x: number; y: number }): { x: number; y: number } {
  const scale = window.devicePixelRatio || 1;
  return { x: pos.x / scale, y: pos.y / scale };
}

function isOverQueueZone(pos: { x: number; y: number }): boolean {
  const { x, y } = toCssPoint(pos);
  const el = document.elementFromPoint(x, y);
  if (el?.closest(QUEUE_DROP_SELECTOR)) return true;
  // 兜底：浮层挡住 hit-test 或边缘空洞时，看队列面板矩形
  for (const node of document.querySelectorAll(QUEUE_DROP_SELECTOR)) {
    const r = node.getBoundingClientRect();
    if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) return true;
  }
  return false;
}

function dropActionFor(overQueue: boolean): DropAction {
  return overQueue ? "enqueue" : "replace-play";
}

/** 解析拖放路径为队列项；无音频时返回空数组 */
async function resolveDropItems(paths: string[]): Promise<QueueItem[]> {
  if (!paths || paths.length === 0) return [];
  const files = await api.resolveDropPaths(paths);
  return files.map(folderFileToQueueItem);
}

/** 执行拖放：替换播放 或 追加队列 */
async function applyDrop(items: QueueItem[], action: DropAction): Promise<void> {
  if (items.length === 0) return;
  const { playQueue, enqueue } = useApp.getState();
  if (action === "enqueue") {
    await enqueue(items);
  } else {
    await playQueue(items, 0);
  }
}

function setQueueDropHighlight(on: boolean) {
  document.body.classList.toggle("drop-over-queue", on);
}

function setDropDragging(on: boolean) {
  document.body.classList.toggle("drop-dragging", on);
}

/**
 * 订阅系统文件拖放（Windows 资源管理器拖入）。
 * 只在主窗口调用一次；子窗口不启用。返回取消订阅。
 */
export function setupDropOpen(onFeedback?: (msg: string | null) => void): () => void {
  let overQueue = false;
  let dragging = false;
  let disposed = false;
  let overRaf = 0;
  const win = getCurrentWindow();

  const unlistenPromise = win.onDragDropEvent(async (event) => {
    if (disposed) return;
    const payload = event.payload;

    if (payload.type === "enter" || payload.type === "over") {
      if (!dragging) {
        dragging = true;
        setDropDragging(true);
      }
      // over 高频触发：rAF 合并命中检测，避免每帧 elementFromPoint + 写 DOM
      if (payload.type === "over") {
        if (overRaf) return;
        const pos = payload.position;
        overRaf = requestAnimationFrame(() => {
          overRaf = 0;
          if (disposed) return;
          const q = isOverQueueZone(pos);
          if (q !== overQueue) {
            overQueue = q;
            setQueueDropHighlight(q);
          }
        });
        return;
      }
      const q = isOverQueueZone(payload.position);
      if (q !== overQueue) {
        overQueue = q;
        setQueueDropHighlight(q);
      }
      return;
    }

    if (payload.type === "leave") {
      if (overRaf) {
        cancelAnimationFrame(overRaf);
        overRaf = 0;
      }
      overQueue = false;
      if (dragging) {
        dragging = false;
        setDropDragging(false);
      }
      setQueueDropHighlight(false);
      onFeedback?.(null);
      return;
    }

    if (payload.type === "drop") {
      if (overRaf) {
        cancelAnimationFrame(overRaf);
        overRaf = 0;
      }
      const action = dropActionFor(isOverQueueZone(payload.position));
      overQueue = false;
      if (dragging) {
        dragging = false;
        setDropDragging(false);
      }
      setQueueDropHighlight(false);
      try {
        const items = await resolveDropItems(payload.paths);
        if (disposed) return;
        if (items.length === 0) {
          onFeedback?.("没有可播放的音频文件");
          return;
        }
        await applyDrop(items, action);
        if (disposed) return;
        onFeedback?.(
          action === "enqueue"
            ? `已追加 ${items.length} 首到队列`
            : `正在播放 · 队列已替换为 ${items.length} 首`,
        );
      } catch (e) {
        console.error("[AxMusic] 拖放处理失败:", e);
        if (!disposed) onFeedback?.("拖放处理失败，请重试");
      }
    }
  });

  return () => {
    disposed = true;
    if (overRaf) {
      cancelAnimationFrame(overRaf);
      overRaf = 0;
    }
    void unlistenPromise.then((f) => f()).catch(() => {});
    setQueueDropHighlight(false);
    setDropDragging(false);
  };
}
