import { MiniPlayer } from "./components/MiniPlayer";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { AlbumWallPage } from "./features/browse/AlbumWallPage";
import { PlaceholderPage } from "./features/browse/PlaceholderPage";
import { ManagePage } from "./features/manage/ManagePage";
import { NowPlayingPage } from "./features/player/NowPlayingPage";
import { PlaylistsPage } from "./features/playlists/PlaylistsPage";
import { useApp } from "./state/useApp";

export default function App() {
  const route = useApp((s) => s.route);
  const fullPlayer = useApp((s) => s.fullPlayer);

  return (
    <div className="app-shell">
      <TitleBar />
      <Sidebar />
      <div className="main-col">
        {route === "albums" && <AlbumWallPage />}
        {route === "manage" && <ManagePage />}
        {route === "artists" && <PlaceholderPage route="artists" />}
        {route === "folders" && <PlaceholderPage route="folders" />}
        {route === "playlists" && <PlaylistsPage />}
        {route === "settings" && <PlaceholderPage route="settings" />}
      </div>
      <MiniPlayer />
      {fullPlayer && <NowPlayingPage />}
    </div>
  );
}
