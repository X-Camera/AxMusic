import { CloseDialog } from "./components/CloseDialog";
import { MiniPlayer } from "./components/MiniPlayer";
import { Sidebar } from "./components/Sidebar";
import { AlbumWallPage } from "./features/browse/AlbumWallPage";
import { ArtistsPage } from "./features/browse/ArtistsPage";
import { FoldersPage } from "./features/browse/FoldersPage";
import { SongsPage } from "./features/browse/SongsPage";
import { ManagePage } from "./features/manage/ManagePage";
import { LyricsWindow } from "./features/manage/LyricsWindow";
import { NowPlayingPage } from "./features/player/NowPlayingPage";
import { PlaylistsPage } from "./features/playlists/PlaylistsPage";
import { SettingsPage } from "./features/settings/SettingsPage";
import { useApp } from "./state/useApp";

function isLyricsWindow(): boolean {
  return new URLSearchParams(window.location.search).get("win") === "lyrics";
}

export default function App() {
  const route = useApp((s) => s.route);
  const fullPlayer = useApp((s) => s.fullPlayer);

  // 独立「搜索歌词」子窗口：不渲染主壳
  if (isLyricsWindow()) {
    return <LyricsWindow />;
  }

  return (
    <div className="app-shell">
      <Sidebar />
      <div className="main-col">
        {route === "songs" && <SongsPage />}
        {route === "albums" && <AlbumWallPage />}
        {route === "manage" && <ManagePage />}
        {route === "artists" && <ArtistsPage />}
        {route === "folders" && <FoldersPage />}
        {route === "playlists" && <PlaylistsPage />}
        {route === "settings" && <SettingsPage />}
      </div>
      <MiniPlayer />
      {fullPlayer && <NowPlayingPage />}
      <CloseDialog />
    </div>
  );
}
