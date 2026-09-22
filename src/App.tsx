import { MiniPlayer } from "./components/MiniPlayer";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { AlbumWallPage } from "./features/browse/AlbumWallPage";
import { PlaceholderPage } from "./features/browse/PlaceholderPage";
import { ManagePage } from "./features/manage/ManagePage";
import { useApp } from "./state/useApp";

export default function App() {
  const route = useApp((s) => s.route);

  return (
    <div className="app-shell">
      <TitleBar />
      <Sidebar />
      <div className="main-col">
        {route === "albums" && <AlbumWallPage />}
        {route === "manage" && <ManagePage />}
        {route === "now-playing" && <PlaceholderPage route="now-playing" />}
        {route === "artists" && <PlaceholderPage route="artists" />}
        {route === "folders" && <PlaceholderPage route="folders" />}
        {route === "playlists" && <PlaceholderPage route="playlists" />}
        {route === "settings" && <PlaceholderPage route="settings" />}
      </div>
      <MiniPlayer />
    </div>
  );
}
