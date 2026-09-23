export type PlayStatus = "Stopped" | "Playing" | "Paused";
export type PlayMode = "sequential" | "shuffle" | "repeat_one";

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
  play_mode: PlayMode;
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
  /** 内嵌歌词（标签内） */
  has_lyrics: boolean;
  /** 外挂歌词（同目录同名 .lrc） */
  has_lrc: boolean;
  has_year: boolean;
  has_mb_id: boolean;
  tag_status: string;
  missing: string;
  release_type: string;
  mb_recording_mbid: string;
  mb_release_mbid: string;
  /** Linked local catalog row id; null = 待刮削 */
  catalog_id: number | null;
  mtime: number;
  file_size: number;
  /** catalog 关联行字段（未关联为 null），管理表匹配高亮用 */
  catalog_title: string | null;
  catalog_artist: string | null;
  catalog_album: string | null;
  catalog_year: string | null;
  catalog_track_no: number | null;
}

export interface AlbumCard {
  album: string;
  album_artist: string;
  year: string;
  has_cover: boolean;
  track_count: number;
  cover_path: string | null;
  /** 封面懒加载样例曲目（组内优先有封面的） */
  cover_track_path: string | null;
  cover_track_mtime: number;
}

/** 歌手浏览卡片：album_artist 优先，空则 artist */
export interface ArtistCard {
  name: string;
  track_count: number;
  album_count: number;
  has_cover: boolean;
  cover_track_path: string | null;
  cover_track_mtime: number;
}

/** 管理页右栏「库统计」聚合数据 */
export interface LibraryStats {
  total_tracks: number;
  /** 已关联 catalog（已刮削） */
  linked_tracks: number;
  with_cover: number;
  /** 内嵌歌词 */
  with_lyrics: number;
  /** 外挂 .lrc */
  with_lrc: number;
  catalog_tracks: number;
  catalog_albums: number;
  catalog_artists: number;
}

export interface TrackFilter {
  query?: string | null;
  missing_only?: boolean;
  unlinked_only?: boolean;
  limit?: number | null;
  /** "album"（默认）| "title" | "artist" */
  sort?: "album" | "title" | "artist" | null;
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

export type LyricsSaveMode = "sidecar" | "embed";
export type LyricsPrefer = "sidecar" | "embed";
export type SongsView = "list" | "grid";
export type CloseBehavior = "ask" | "tray" | "exit";

/** 外观：暗色 / 浅色 */
export type ThemeMode = "dark" | "light";

/** 皮肤（表面 + 强调色家族） */
export type ColorScheme =
  | "nebula"
  | "sky"
  | "jade"
  | "rose"
  | "amber"
  | "coral"
  | "graphite";

export interface LyricsSources {
  lrclib: boolean;
  netease: boolean;
  qq: boolean;
}

export interface AppSettings {
  library_root: string | null;
  volume: number;
  play_mode: PlayMode;
  restore_volume: boolean;
  lyrics_save_mode: LyricsSaveMode;
  lyrics_prefer: LyricsPrefer;
  lyrics_sources: LyricsSources;
  songs_view: SongsView;
  close_behavior: CloseBehavior;
  theme_mode: ThemeMode;
  color_scheme: ColorScheme;
}

export type SettingsPatch = Partial<
  Omit<AppSettings, "library_root">
>;

export type RouteId =
  | "songs"
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
  /** catalog 缓存封面（data URL），未刮取为 null */
  cover_data: string | null;
}

export interface CoverCandidate {
  id: string;
  /** "caa" | "itunes" | "netease" | "qq" */
  source: string;
  title: string;
  artist: string;
  thumb_url: string;
  /** 大图 URL（采纳时下载） */
  url: string;
}

/** 歌词面板目标：库内曲目，或满窗播放的任意文件（id=0 + path） */
export interface LyricsTarget {
  /** 库内 tracks.id；库外文件为 0 */
  id: number;
  path: string;
  title: string;
  artist: string;
  filename: string;
}

export interface LyricsCandidate {
  /** source-prefixed id, e.g. "lrclib:123" / "netease:456" / "qq:xxx" */
  id: string;
  source: "lrclib" | "netease" | "qq";
  track_name: string;
  artist_name: string;
  album_name: string;
  duration: number;
  has_synced: boolean;
  has_plain: boolean;
}

export interface LyricsContent {
  synced: string | null;
  plain: string | null;
  translation: string | null;
}

export interface LyricsCurrent {
  embedded: string | null;
  sidecar: string | null;
}

export interface LyricsBatch {
  trackId: number;
  source: string;
  items: LyricsCandidate[];
  error?: string;
}

export interface PlaylistSummary {
  /** 文件名（不含 .m3u8）；喜爱固定为「喜爱」 */
  name: string;
  /** 条目总数（含失效项） */
  track_count: number;
  /** EXTINF 时长之和（未知计 0） */
  total_ms: number;
  /** 系统「喜爱」歌单（不可重命名/删除） */
  is_favorites: boolean;
}

export interface PlaylistAddItem {
  path: string;
  title: string;
  artist: string;
  duration_ms: number;
}

export interface PlaylistEntry {
  /** 解析后的绝对路径 */
  path: string;
  /** 文件里存的那一行 */
  rel_path: string;
  title: string;
  artist: string;
  duration_ms: number;
  exists: boolean;
  /** DB 富化（仅展示），无库/未入库为 null */
  track: TrackRow | null;
}

export interface PlaylistDetail {
  /** 喜爱固定为「喜爱」 */
  name: string;
  entries: PlaylistEntry[];
  /** 系统「喜爱」歌单 */
  is_favorites: boolean;
}

export interface FavoriteToggleResult {
  /** 切换后是否已喜爱 */
  favorited: boolean;
  /** 喜爱歌单当前条目数 */
  track_count: number;
}

/** 文件夹浏览：子目录 */
export interface FolderDir {
  name: string;
  path: string;
}

/** 文件夹浏览：音频文件（简要标签） */
export interface FolderFile {
  path: string;
  name: string;
  title: string;
  artist: string;
  duration_ms: number;
}

/** 单层目录列表（树展开 + 右侧文件） */
export interface FolderListing {
  path: string;
  parent: string | null;
  dirs: FolderDir[];
  files: FolderFile[];
}

/** 标签缓存条目（path 唯一，字段与 FolderFile 对齐） */
export type FolderMeta = FolderFile;
