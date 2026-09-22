import { invoke } from "@tauri-apps/api/core";
import type {
  AlbumCard,
  LibraryRoot,
  PathsInfo,
  PlayerSnapshot,
  QueueItem,
  ScanResult,
  TrackFilter,
  TrackInfo,
  TrackRow,
} from "./types";

export const api = {
  getPaths: () => invoke<PathsInfo>("get_paths"),
  getAppInfo: () => invoke<{ name: string; version: string }>("get_app_info"),

  getLibraryRoot: () => invoke<LibraryRoot | null>("get_library_root"),
  initLibrary: (req: {
    mode: "new" | "existing";
    parent?: string;
    name?: string;
    path?: string;
  }) => invoke<LibraryRoot>("init_library", { req }),

  getTracks: (filter?: TrackFilter) =>
    invoke<TrackRow[]>("get_tracks", { filter: filter ?? null }),
  getAlbums: () => invoke<AlbumCard[]>("get_albums"),
  getAlbumTracks: (album: string, albumArtist: string) =>
    invoke<TrackRow[]>("get_album_tracks", { album, albumArtist }),
  getTrackCount: () => invoke<number>("get_track_count"),
  refreshScan: () => invoke<ScanResult>("refresh_scan"),
  includeInLibrary: (path: string) =>
    invoke<{ copied_to: string; track: TrackRow }>("include_in_library", { path }),

  getPlayerState: () => invoke<PlayerSnapshot>("get_player_state"),
  playFile: (path: string) => invoke<PlayerSnapshot>("play_file", { path }),
  playQueue: (items: QueueItem[], start: number) =>
    invoke<PlayerSnapshot>("play_queue", { items, start }),
  playerPlay: () => invoke<PlayerSnapshot>("player_play"),
  playerPause: () => invoke<PlayerSnapshot>("player_pause"),
  playerToggle: () => invoke<PlayerSnapshot>("player_toggle"),
  playerNext: () => invoke<PlayerSnapshot>("player_next"),
  playerPrev: () => invoke<PlayerSnapshot>("player_prev"),
  playerSeek: (ms: number) => invoke<PlayerSnapshot>("player_seek", { ms }),
  playerSetVolume: (volume: number) =>
    invoke<PlayerSnapshot>("player_set_volume", { volume }),

  listDirAudio: (path: string) => invoke<TrackInfo[]>("list_dir_audio", { path }),
};

export function formatTime(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) ms = 0;
  const total = Math.floor(ms / 1000);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export function trackRowToQueueItem(t: TrackRow): QueueItem {
  return {
    path: t.path,
    title: t.title || t.filename,
    duration_ms: t.duration_ms,
  };
}
