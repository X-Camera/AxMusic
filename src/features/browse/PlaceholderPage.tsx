import type { RouteId } from "../../lib/types";
import { useApp } from "../../state/useApp";
import { TopBar } from "../../components/TopBar";

const COPY: Partial<Record<RouteId, { title: string; body: string }>> = {
  "now-playing": {
    title: "正在播放",
    body: "播放页（大封面 + LRC）在后续里程碑。现在请用底部迷你播放条。",
  },
  artists: {
    title: "歌手",
    body: "歌手浏览占位页。本轮先用专辑墙 + 管理表。",
  },
  folders: {
    title: "文件夹",
    body: "任意路径文件夹浏览占位页。播放不依赖库目录，本轮可从管理表或专辑墙播放。",
  },
  settings: {
    title: "设置",
    body: "设置占位页。便携数据目录等信息稍后可在此查看。",
  },
};

export function PlaceholderPage({ route }: { route: RouteId }) {
  const info = COPY[route] ?? { title: "未命名", body: "占位页。" };
  void useApp;
  return (
    <>
      <TopBar title={info.title} />
      <div className="page-scroll">
        <div className="empty-state">
          <div className="display" style={{ fontSize: 22 }}>
            {info.title}
          </div>
          <p className="muted">{info.body}</p>
        </div>
      </div>
    </>
  );
}
