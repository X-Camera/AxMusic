export type PlayStatus = "Stopped" | "Playing" | "Paused";

export interface TrackInfo {
  path: string;
  title: string;
  duration_ms: number;
  sample_rate: number;
  channels: number;
}

export interface QueueItem {
  path: string;
  title: string;
  duration_ms: number;
}

export interface PlayerSnapshot {
  status: PlayStatus;
  position_ms: number;
  duration_ms: number;
  volume: number;
  track: TrackInfo | null;
  queue: QueueItem[];
  queue_index: number | null;
}

export interface LibraryRoot {
  id: number;
  path: string;
  initialized_at: string;
}

export interface TrackRow {
  id: number;
  path: string;
  filename: string;
  title: string;
  artist: string;
  album: string;
  album_artist: string;
  year: string;
  track_no: number | null;
  duration_ms: number;
  format: string;
  sample_rate: number | null;
  bit_rate: number | null;
  has_cover: boolean;
  has_lyrics: boolean;
  has_year: boolean;
  has_mb_id: boolean;
  tag_status: string;
  missing: string;
  release_type: string;
  mb_recording_mbid: string;
  mb_release_mbid: string;
  /** Linked local catalog row id; null = 待刮削 */
  catalog_id: number | null;
}

export interface AlbumCard {
  album: string;
  album_artist: string;
  year: string;
  has_cover: boolean;
  track_count: number;
  cover_path: string | null;
}

export interface TrackFilter {
  query?: string | null;
  missing_only?: boolean;
  unlinked_only?: boolean;
  limit?: number | null;
}

export interface ScanProgress {
  scanned: number;
  totalFiles: number;
  added: number;
  updated: number;
  errors: number;
  current: string;
}

export interface ScanResult {
  total: number;
  added: number;
  updated: number;
  errors: number;
}

export interface PathsInfo {
  data_root: string;
  portable: boolean;
  db_path: string;
  settings_path: string;
}

export type RouteId =
  | "now-playing"
  | "albums"
  | "artists"
  | "folders"
  | "playlists"
  | "manage"
  | "settings";

export interface ScrapeCandidate {
  id: string;
  kind: string;
  title: string;
  artist: string;
  year: string;
  track_count: number;
  country: string;
  disambiguation: string;
  release_id: string;
}

export interface FieldChange {
  field: string;
  old: string;
  new: string;
}

export interface TrackPlan {
  track_id: number;
  path: string;
  display: string;
  matched_title: string;
  changes: FieldChange[];
}

export interface CatalogTrackDraft {
  /** recording MBID */
  mbid: string;
  release_mbid: string;
  title: string;
  artist: string;
  album: string;
  album_artist: string;
  year: string;
  track_no: number | null;
  release_type: string;
}

export interface ApplyPlan {
  candidate_id: string;
  release_id: string;
  candidate_label: string;
  tracks: TrackPlan[];
  /** 采纳后存入本地 catalog 的完整曲目表（专辑模式为整张） */
  catalog_tracks: CatalogTrackDraft[];
  cover_will_write: boolean;
  unmatched: string[];
}

export interface CatalogRow {
  id: number;
  source: string;
  kind: string;
  mbid: string;
  release_mbid: string;
  title: string;
  artist: string;
  album: string;
  album_artist: string;
  year: string;
  track_no: number | null;
  release_type: string;
  cover_path: string | null;
  created_at: string;
}

export interface CompareData {
  track: TrackRow;
  catalog: CatalogRow | null;
  changes: FieldChange[];
}
