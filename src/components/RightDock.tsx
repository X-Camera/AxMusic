import { QueuePanel } from "./QueuePanel";
import { SideLyrics } from "./SideLyrics";
import { useApp } from "../state/useApp";
import "./RightDock.css";

/**
 * 右边栏容器：承载播放列表 / 主页歌词。
 * 由 App 常驻挂载（绝对定位贴右缘），切页不卸载——歌词引擎不重放载入飞入。
 * 两者同开时上下 1:1 等分（上列表、下歌词）。
 */
export function RightDock() {
  const queueOpen = useApp((s) => s.queuePanelOpen);
  const lyricsOpen = useApp((s) => s.lyricsPanelOpen);

  if (!queueOpen && !lyricsOpen) return null;

  const both = queueOpen && lyricsOpen;

  return (
    <aside className="right-dock right-dock-dock" aria-label="右边栏">
      {queueOpen && (
        <div className={`right-dock-pane${both ? " split" : ""}`}>
          <QueuePanel />
        </div>
      )}
      {lyricsOpen && (
        <div className={`right-dock-pane${both ? " split" : ""}`}>
          <SideLyrics />
        </div>
      )}
    </aside>
  );
}
