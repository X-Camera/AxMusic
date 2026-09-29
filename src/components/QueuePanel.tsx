import { useRef } from "react";
import { X } from "lucide-react";

import type { QueueItem } from "../lib/types";
import { api, formatTime } from "../lib/api";
import { alertError, promptText } from "../lib/dialog";
import { QUEUE_DROP_ZONE } from "../lib/dropOpen";
import { useApp } from "../state/useApp";
import { FavoriteHeart } from "./FavoriteHeart";
import { VirtualList, QUEUE_ROW_HEIGHT } from "./VirtualList";
import "./QueuePanel.css";

/** 播放队列格：由 RightDock 承载（可独占或与歌词上下等分） */
export function QueuePanel() {
  const player = useApp((s) => s.player);
  const playQueue = useApp((s) => s.playQueue);
  const removeQueueAt = useApp((s) => s.removeQueueAt);
  const setQueuePanelOpen = useApp((s) => s.setQueuePanelOpen);
  const listRef = useRef<HTMLDivElement>(null);

  const queue = player?.queue ?? [];
  const queueLen = queue.length;
  const queueIndex = player?.queue_index ?? null;

  async function saveQueueAs() {
    if (queueLen === 0) return;
    const name = await promptText("存为歌单名称", "");
    if (!name) return;
    try {
      await api.playlistCreate(
        name.trim(),
        queue.map((q) => ({
          path: q.path,
          title: q.title,
          artist: "",
          duration_ms: q.duration_ms,
        })),
      );
    } catch (e) {
      await alertError(e);
    }
  }

  async function playQueueAt(items: QueueItem[], index: number) {
    await playQueue(items, index);
  }

  return (
    <div className="queue-panel" aria-label="播放队列" data-drop-zone={QUEUE_DROP_ZONE}>
      <div className="queue-panel-head">
        <span>播放队列 · {queueLen} 首</span>
        <div className="queue-panel-head-actions">
          <button
            className="link-btn"
            disabled={queueLen === 0}
            title="另存为新歌单"
            onClick={() => void saveQueueAs()}
          >
            存为歌单
          </button>
          <button
            className="queue-panel-close"
            title="收起播放列表"
            aria-label="收起播放列表"
            onClick={() => setQueuePanelOpen(false)}
          >
            <X size={14} />
          </button>
        </div>
      </div>
      <div className="queue-panel-list" role="listbox" aria-label="播放队列" ref={listRef}>
        {queueLen === 0 ? (
          <div className="tertiary queue-panel-empty">队列为空</div>
        ) : (
          <VirtualList
            items={queue}
            rowHeight={QUEUE_ROW_HEIGHT}
            getItemKey={(q, i) => `${q.path}:${i}`}
            getScrollElement={() => listRef.current}
            renderRow={(q, i) => (
              <div className="queue-panel-item-wrap">
                <button
                  className={`queue-panel-item${i === queueIndex ? " active" : ""}`}
                  role="option"
                  aria-selected={i === queueIndex}
                  onClick={() => void playQueueAt(queue, i)}
                >
                  <span className="queue-panel-item-title">
                    {q.title || q.path.split(/[\\/]/).pop()}
                  </span>
                  <span className="mono tertiary">{formatTime(q.duration_ms)}</span>
                </button>
                <FavoriteHeart
                  item={{
                    path: q.path,
                    title: q.title,
                    artist: "",
                    duration_ms: q.duration_ms,
                  }}
                  size={13}
                  className="queue-panel-fav"
                />
                <button
                  className="queue-panel-remove"
                  title="移出队列"
                  aria-label="移出队列"
                  onClick={() => void removeQueueAt(i)}
                >
                  <X size={13} />
                </button>
              </div>
            )}
          />
        )}
      </div>
    </div>
  );
}
