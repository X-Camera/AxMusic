import { MiniPlayer } from "./components/MiniPlayer";
import { Sidebar } from "./components/Sidebar";
import { AlbumWallPage } from "./features/browse/AlbumWallPage";
import { ArtistsPage } from "./features/browse/ArtistsPage";
import { PlaceholderPage } from "./features/browse/PlaceholderPage";
import { SongsPage } from "./features/browse/SongsPage";
import { ManagePage } from "./features/manage/ManagePage";
import { NowPlayingPage } from "./features/player/NowPlayingPage";
import { PlaylistsPage } from "./features/playlists/PlaylistsPage";
import { useApp } from "./state/useApp";

export default function App() {
  const route = useApp((s) => s.route);
  const fullPlayer = useApp((s) => s.fullPlayer);

  return (
    <div className="app-shell">
      <Sidebar />
      <div className="main-col">
        {route === "songs" && <SongsPage />}
        {route === "albums" && <AlbumWallPage />}
        {route === "manage" && <ManagePage />}
        {route === "artists" && <ArtistsPage />}
        {route === "folders" && <PlaceholderPage route="folders" />}
        {route === "playlists" && <PlaylistsPage />}
        {route === "settings" && <PlaceholderPage route="settings" />}
      </div>
      <MiniPlayer />
      {fullPlayer && <NowPlayingPage />}
    </div>
  );
}
