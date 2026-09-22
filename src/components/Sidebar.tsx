import {
  Disc3,
  FolderOpen,
  ListMusic,
  ListVideo,
  Music2,
  Settings,
  UserRound,
} from "lucide-react";

import type { RouteId } from "../lib/types";
import { useApp } from "../state/useApp";
import "./Sidebar.css";

const PLAY_ITEMS: { id: RouteId; label: string; icon: typeof Disc3 }[] = [
  { id: "songs", label: "歌曲", icon: Music2 },
  { id: "albums", label: "专辑墙", icon: Disc3 },
  { id: "artists", label: "歌手", icon: UserRound },
  { id: "folders", label: "文件夹", icon: FolderOpen },
  { id: "playlists", label: "播放列表", icon: ListMusic },
];

export function Sidebar() {
  const route = useApp((s) => s.route);
  const setRoute = useApp((s) => s.setRoute);

  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="brand-mark">
          <ListVideo size={18} strokeWidth={2.2} />
        </div>
        <div className="brand-text">
          <div className="brand-name">AxMusic</div>
          <div className="brand-sub">本地音乐</div>
        </div>
      </div>

      <nav className="nav" aria-label="播放">
        {PLAY_ITEMS.map((item) => {
          const Icon = item.icon;
          const active = route === item.id;
          return (
            <button
              key={item.id}
              className={`nav-item${active ? " active" : ""}`}
              onClick={() => setRoute(item.id)}
              aria-current={active ? "page" : undefined}
            >
              <Icon size={16} />
              <span>{item.label}</span>
            </button>
          );
        })}
      </nav>

      <div className="nav-divider" />

      <nav className="nav" aria-label="管理">
        <button
          className={`nav-item${route === "manage" ? " active" : ""}`}
          onClick={() => setRoute("manage")}
          aria-current={route === "manage" ? "page" : undefined}
        >
          <ListVideo size={16} />
          <span>管理</span>
        </button>
      </nav>

      <div className="nav-spacer" />

      <div className="nav-divider" />
      <nav className="nav" aria-label="设置">
        <button
          className={`nav-item${route === "settings" ? " active" : ""}`}
          onClick={() => setRoute("settings")}
          aria-current={route === "settings" ? "page" : undefined}
        >
          <Settings size={16} />
          <span>设置</span>
        </button>
      </nav>
    </aside>
  );
}
