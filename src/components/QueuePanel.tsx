import { X } from "lucide-react";

import type { QueueItem } from "../lib/types";
import { api, formatTime } from "../lib/api";
import { useApp } from "../state/useApp";
import { FavoriteHeart } from "./FavoriteHeart";
import "./QueuePanel.css";

/** 播放队列格：由 RightDock 承载（可独占或与歌词上下等分） */
export function QueuePanel() {
  const player = useApp((s) => s.player);
  const playQueue = useApp((s) => s.playQueue);
  const removeQueueAt = useApp((s) => s.removeQueueAt);
  const setQueuePanelOpen = useApp((s) => s.setQueuePanelOpen);

  const queue = player?.queue ?? [];
  const queueLen = queue.length;
  const queueIndex = player?.queue_index ?? null;

  async function saveQueueAs() {
    if (queueLen === 0) return;
    const name = window.prompt("存为歌单名称", "");
    if (!name || !name.trim()) return;
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
      window.alert(String(e));
    }
  }

  async function playQueueAt(items: QueueItem[], index: number) {
    await playQueue(items, index);
  }

  return (
    <div className="queue-panel" aria-label="播放队列">
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
      <div className="queue-panel-list" role="listbox" aria-label="播放队列">
        {queueLen === 0 ? (
          <div className="tertiary queue-panel-empty">队列为空</div>
        ) : (
          queue.map((q, i) => (
            <div key={`${q.path}-${i}`} className="queue-panel-item-wrap">
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
          ))
        )}
      </div>
    </div>
  );
}
