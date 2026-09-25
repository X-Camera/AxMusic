import { QueuePanel } from "./QueuePanel";
import { SideLyrics } from "./SideLyrics";
import { useApp } from "../state/useApp";
import "./RightDock.css";

/**
 * 右边栏容器：承载播放列表 / 主页歌词。
 * - dock：浏览页贴顶栏下缘挤入
 * - slot：管理/设置填入原右边栏槽位
 * 两者同开时上下 1:1 等分（上列表、下歌词）。
 */
export function RightDock({ variant }: { variant: "dock" | "slot" }) {
  const queueOpen = useApp((s) => s.queuePanelOpen);
  const lyricsOpen = useApp((s) => s.lyricsPanelOpen);

  if (!queueOpen && !lyricsOpen) return null;

  const both = queueOpen && lyricsOpen;

  return (
    <aside className={`right-dock right-dock-${variant}`} aria-label="右边栏">
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
