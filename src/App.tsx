import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";

import { CloseDialog } from "./components/CloseDialog";
import { DropOpenLayer } from "./components/DropOpenLayer";
import { MiniPlayer } from "./components/MiniPlayer";
import { RightDock } from "./components/RightDock";
import { Sidebar } from "./components/Sidebar";
import { AlbumWallPage } from "./features/browse/AlbumWallPage";
import { ArtistsPage } from "./features/browse/ArtistsPage";
import { FoldersPage } from "./features/browse/FoldersPage";
import { SongsPage } from "./features/browse/SongsPage";
import { HistoryPage } from "./features/history/HistoryPage";
import { ManagePage } from "./features/manage/ManagePage";
import { LyricsWindow } from "./features/manage/LyricsWindow";
import { NowPlayingPage } from "./features/player/NowPlayingPage";
import { PlaylistsPage } from "./features/playlists/PlaylistsPage";
import { SettingsPage } from "./features/settings/SettingsPage";
import { VizSettingsWindow } from "./features/visualizer/VizSettingsWindow";
import { useApp } from "./state/useApp";
import { useFavorites } from "./state/useFavorites";
import { DialogHost } from "./lib/dialog";

function windowKind(): string | null {
  return new URLSearchParams(window.location.search).get("win");
}

/** 本身已有右边栏的页面：播放列表只替换槽位内容，不再从右挤入 */
const QUEUE_SLOT_ROUTES = new Set(["manage", "settings"]);

export default function App() {
  const route = useApp((s) => s.route);
  const fullPlayer = useApp((s) => s.fullPlayer);
  const queuePanelOpen = useApp((s) => s.queuePanelOpen);
  const lyricsPanelOpen = useApp((s) => s.lyricsPanelOpen);
  const sideOpen = queuePanelOpen || lyricsPanelOpen;
  /** 浏览页挤内容；管理/设置由本页右栏占位，不挤 */
  const squeeze = sideOpen && !QUEUE_SLOT_ROUTES.has(route);

  // 归档/重扫可能改路径：刷喜爱对照键（后端读取时会兜底重匹配自愈）
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen("library://changed", () => {
      void useFavorites.getState().reload();
    }).then((f) => {
      unlisten = f;
    });
    return () => unlisten?.();
  }, []);

  // 独立子窗口：不渲染主壳
  const win = windowKind();
  if (win === "lyrics") {
    return <LyricsWindow />;
  }
  if (win === "viz-settings") {
    return <VizSettingsWindow />;
  }

  return (
    <div className="app-shell">
      <Sidebar />
      <div className={`main-col${squeeze ? " queue-squeeze" : ""}`}>
        {route === "songs" && <SongsPage />}
        {route === "albums" && <AlbumWallPage />}
        {route === "manage" && <ManagePage />}
        {route === "artists" && <ArtistsPage />}
        {route === "folders" && <FoldersPage />}
        {route === "playlists" && <PlaylistsPage />}
        {route === "history" && <HistoryPage />}
        {route === "settings" && <SettingsPage />}
        {/* 常驻挂载：切页不卸载歌词/队列，避免重载飞入 */}
        {sideOpen && <RightDock />}
      </div>
      <MiniPlayer />
      {fullPlayer && <NowPlayingPage />}
      <CloseDialog />
      <DialogHost />
      <DropOpenLayer />
    </div>
  );
}
