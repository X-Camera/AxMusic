import { invoke } from "@tauri-apps/api/core";
import type {
  AlbumCard,
  ApplyPlan,
  LibraryRoot,
  LibraryStats,
  PathsInfo,
  PlayerSnapshot,
  QueueItem,
  ScanResult,
  ScrapeCandidate,
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
  getLibraryStats: () => invoke<LibraryStats>("library_stats"),
  /** 提取文件内嵌封面缩略图（磁盘缓存），无封面/解析失败返回 null */
  trackCoverThumb: (path: string) =>
    invoke<string | null>("track_cover_thumb", { path }),
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

  scrapeSearchAlbum: (album: string, artist: string) =>
    invoke<ScrapeCandidate[]>("scrape_search_album", { album, artist }),
  scrapeSearchTrack: (title: string, artist: string) =>
    invoke<ScrapeCandidate[]>("scrape_search_track", { title, artist }),
  scrapeBuildPlan: (
    releaseMbid: string,
    trackIds: number[],
    mode: "album" | "track",
    writeCover: boolean,
  ) =>
    invoke<ApplyPlan>("scrape_build_plan", {
      releaseMbid,
      trackIds,
      mode,
      writeCover,
    }),
  catalogSave: (plan: ApplyPlan, fetchCover: boolean) =>
    invoke<number[]>("catalog_save", { plan, fetchCover }),
  catalogCompare: (trackId: number) =>
    invoke<import("./types").CompareData>("catalog_compare", { trackId }),
  catalogApplyToTrack: (trackId: number, fields: string[], writeCover: boolean) =>
    invoke<number>("catalog_apply_to_track", { trackId, fields, writeCover }),
  catalogMatchOne: (trackId: number) =>
    invoke<number | null>("catalog_match_one", { trackId }),
  catalogMatchAll: () => invoke<number>("catalog_match_all"),
  isInLibrary: (path: string) => invoke<boolean>("is_in_library", { path }),

  /** 多源并发搜索（歌手/歌名可改，非空才生效）；结果经 lyrics://batch 流式推送，lyrics://done 收尾 */
  lyricsSearch: (trackId: number, artist?: string, title?: string) =>
    invoke<void>("lyrics_search", { trackId, artist: artist ?? null, title: title ?? null }),
  lyricsFetch: (id: string) =>
    invoke<import("./types").LyricsContent>("lyrics_fetch", { id }),
  lyricsSave: (trackId: number, lrcId: string, mode: "sidecar" | "embed") =>
    invoke<string>("lyrics_save", { trackId, lrcId, mode }),
  lyricsExportSidecar: (trackId: number, overwrite: boolean) =>
    invoke<string>("lyrics_export_sidecar", { trackId, overwrite }),
  lyricsEmbedSidecar: (trackId: number) =>
    invoke<string>("lyrics_embed_sidecar", { trackId }),
  lyricsCurrent: (trackId: number) =>
    invoke<import("./types").LyricsCurrent>("lyrics_current", { trackId }),
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
