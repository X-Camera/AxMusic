import { invoke } from "@tauri-apps/api/core";
import { notNil } from "./nil";
import type {
  AlbumCard,
  AppSettings,
  ApplyPlan,
  ArtistCard,
  LibraryRoot,
  LibraryStats,
  PathsInfo,
  PlayerSnapshot,
  RepeatMode,
  PlaylistAddItem,
  PlaylistDetail,
  PlaylistSummary,
  QueueItem,
  ReplayGainScan,
  ReplayGainTags,
  ScanResult,
  SettingsPatch,
  TrackFilter,
  TrackInfo,
  TrackRow,
} from "./types";

export const api = {
  getPaths: () => invoke<PathsInfo>("get_paths"),
  getAppInfo: () => invoke<{ name: string; version: string }>("get_app_info"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  updateSettings: (patch: SettingsPatch) =>
    invoke<AppSettings>("update_settings", { patch }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  /** 关闭询问：action = tray|exit；remember 记住为默认关闭行为 */
  resolveWindowClose: (action: "tray" | "exit", remember: boolean) =>
    invoke<void>("resolve_window_close", { action, remember }),

  /** 资源管理器右键菜单：查询 / 注册 / 卸载（HKCU，无需管理员） */
  shellMenuStatus: () =>
    invoke<import("./types").ShellMenuStatus>("shell_menu_status"),
  shellMenuRegister: () =>
    invoke<import("./types").ShellMenuStatus>("shell_menu_register"),
  shellMenuUnregister: () =>
    invoke<import("./types").ShellMenuStatus>("shell_menu_unregister"),

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
  getArtists: () => invoke<ArtistCard[]>("get_artists"),
  getArtistAlbums: (artist: string) =>
    invoke<AlbumCard[]>("get_artist_albums", { artist }),
  getArtistTracks: (artist: string) =>
    invoke<TrackRow[]>("get_artist_tracks", { artist }),
  getTrackCount: () => invoke<number>("get_track_count"),
  getLibraryStats: () => invoke<LibraryStats>("library_stats"),
  /** 提取文件内嵌封面缩略图（磁盘缓存），无封面/解析失败返回 null */
  trackCoverThumb: (path: string) =>
    invoke<string | null>("track_cover_thumb", { path }),
  /** 正在播放页：标签 + 640px 封面 + 内嵌/外挂歌词 */
  trackMediaInfo: (path: string) =>
    invoke<{
      path: string;
      filename: string;
      title: string;
      artist: string;
      album: string;
      album_artist: string;
      year: string;
      has_lyrics: boolean;
      cover_data: string | null;
      embedded: string | null;
      sidecar: string | null;
    }>("track_media_info", { path }),
  refreshScan: () => invoke<ScanResult>("refresh_scan"),
  includeInLibrary: (path: string) =>
    invoke<{ copied_to: string; track: TrackRow }>("include_in_library", { path }),

  getPlayerState: () => invoke<PlayerSnapshot>("get_player_state"),
  playFile: (path: string) => invoke<PlayerSnapshot>("play_file", { path }),
  playQueue: (items: QueueItem[], start: number) =>
    invoke<PlayerSnapshot>("play_queue", { items, start }),
  /** 追加到当前队列末尾（排队等播放；同 path 去重） */
  playerEnqueue: (items: QueueItem[]) =>
    invoke<PlayerSnapshot>("player_enqueue", { items }),
  /** 移出队列第 index 首；若是当前曲则续播下一首或停 */
  playerRemoveAt: (index: number) =>
    invoke<PlayerSnapshot>("player_remove_at", { index }),
  playerPlay: () => invoke<PlayerSnapshot>("player_play"),
  playerPause: () => invoke<PlayerSnapshot>("player_pause"),
  playerToggle: () => invoke<PlayerSnapshot>("player_toggle"),
  playerNext: () => invoke<PlayerSnapshot>("player_next"),
  playerPrev: () => invoke<PlayerSnapshot>("player_prev"),
  playerSeek: (ms: number) => invoke<PlayerSnapshot>("player_seek", { ms }),
  playerSetVolume: (volume: number) =>
    invoke<PlayerSnapshot>("player_set_volume", { volume }),
  playerSetShuffle: (shuffle: boolean) =>
    invoke<PlayerSnapshot>("player_set_shuffle", { shuffle }),
  playerSetRepeat: (repeat: RepeatMode) =>
    invoke<PlayerSnapshot>("player_set_repeat", { repeat }),

  listDirAudio: (path: string) => invoke<TrackInfo[]>("list_dir_audio", { path }),
  /** 单层目录：子文件夹 + 本层音频（withFiles=false 时只要子目录，给树展开用） */
  listDirTree: (path: string, withFiles = true) =>
    invoke<import("./types").FolderListing>("list_dir_tree", { path, withFiles }),
  /** 系统拖放：文件/文件夹路径 → 可播放条目（文件夹只取一层，不递归） */
  resolveDropPaths: (paths: string[]) =>
    invoke<import("./types").FolderFile[]>("resolve_drop_paths", { paths }),
  /** 递归收集音频（含子文件夹，后端有上限，不读标签） */
  listDirAudioRecursive: (path: string) =>
    invoke<import("./types").FolderFile[]>("list_dir_audio_recursive", { path }),
  /** 查标签缓存（mtime/size 有效才返回） */
  folderMetaLookup: (paths: string[]) =>
    invoke<import("./types").FolderMeta[]>("folder_meta_lookup", { paths }),
  /** 读标签并写缓存（有效缓存复用） */
  folderMetaRead: (paths: string[]) =>
    invoke<import("./types").FolderMeta[]>("folder_meta_read", { paths }),

  /** 专辑刮削：四源并发，结果经 scrape://batch/{searchId,source,items} 流式返回 */
  scrapeSearchAlbum: (searchId: number, album: string, artist: string) =>
    invoke<void>("scrape_search_album", { searchId, album, artist }),
  /** 单曲刮削：同上，按曲名 */
  scrapeSearchTrack: (searchId: number, title: string, artist: string) =>
    invoke<void>("scrape_search_track", { searchId, title, artist }),
  /** 单曲所属专辑列表（挑专辑用；MB 可多条） */
  scrapeTrackAlbums: (source: string, trackId: string) =>
    invoke<import("./types").TrackAlbum[]>("scrape_track_albums", { source, trackId }),
  scrapeBuildPlan: (
    source: string,
    releaseMbid: string,
    trackIds: number[],
    mode: "album" | "track",
    forceTrackNo?: number,
  ) =>
    invoke<ApplyPlan>("scrape_build_plan", {
      source,
      releaseMbid,
      trackIds,
      mode,
      ...(notNil(forceTrackNo) ? { forceTrackNo } : {}),
    }),
  /** 采纳刮削结果 → 本地 catalog（只存文字，封面另走 catalogFetchCover） */
  catalogSave: (plan: ApplyPlan) => invoke<number[]>("catalog_save", { plan }),
  catalogCompare: (trackId: number) =>
    invoke<import("./types").CompareData>("catalog_compare", { trackId }),
  /** 多源封面搜索（CAA/iTunes/网易云/QQ音乐）→ 候选列表（缩略图+大图 URL） */
  coverSearch: (trackId: number) =>
    invoke<import("./types").CoverCandidate[]>("cover_search", { trackId }),
  /** 采纳封面候选：下载大图存 `<库>/covers/` 并更新 catalog 引用，返回 data URL */
  coverApply: (trackId: number, url: string) =>
    invoke<string>("cover_apply", { trackId, url }),
  catalogApplyToTrack: (trackId: number, fields: string[], writeCover: boolean, catalogId?: number) =>
    invoke<number>("catalog_apply_to_track", {
      trackId,
      fields,
      writeCover,
      ...(catalogId && catalogId > 0 ? { catalogId } : {}),
    }),
  /** 用户编辑文件标签写回（空值不覆盖）；写后会再试一次 catalog 字段关联 */
  trackWriteTags: (trackId: number, changes: { field: string; old: string; new: string }[]) =>
    invoke<number>("track_write_tags", { trackId, changes }),

  // ── replaygain 扫描估算 ─────────────────────────────────────────
  /** 只读文件已有 REPLAYGAIN/R128 标签（不解码） */
  replaygainTags: (trackId: number) =>
    invoke<ReplayGainTags>("replaygain_tags", { trackId }),
  /** 解码量响度，返回建议增益（不写文件；结果会进运行时缓存） */
  replaygainAnalyze: (trackId: number) =>
    invoke<ReplayGainScan>("replaygain_analyze", { trackId }),
  /** 写入 REPLAYGAIN_TRACK_GAIN；trackPeak 为 null 时不写峰值、也不删已有 PEAK */
  replaygainWrite: (trackId: number, trackGainDb: number, trackPeak: number | null) =>
    invoke<void>("replaygain_write", {
      trackId,
      trackGainDb,
      trackPeak,
    }),
  catalogMatchOne: (trackId: number) =>
    invoke<number | null>("catalog_match_one", { trackId }),
  catalogMatchAll: () => invoke<number>("catalog_match_all"),
  isInLibrary: (path: string) => invoke<boolean>("is_in_library", { path }),
  /** 按绝对路径查库内曲目；无库/未入库返回 null */
  getTrackByPath: (path: string) =>
    invoke<TrackRow | null>("get_track_by_path", { path }),

  /** 多源并发搜索（歌手/歌名可改，非空才生效）；结果经 lyrics://batch 流式推送，lyrics://done 收尾。
   *  `searchId` 由前端生成，事件按它过滤过期批次；库外文件只传 path（trackId 传 null/0）。 */
  lyricsSearch: (
    searchId: number,
    trackId: number | null,
    artist?: string,
    title?: string,
    path?: string | null,
  ) =>
    invoke<void>("lyrics_search", {
      searchId,
      trackId: trackId && trackId > 0 ? trackId : null,
      path: path ?? null,
      artist: artist ?? null,
      title: title ?? null,
    }),
  lyricsFetch: (id: string) =>
    invoke<import("./types").LyricsContent>("lyrics_fetch", { id }),
  lyricsSave: (
    trackId: number | null,
    lrcId: string,
    mode: "sidecar" | "embed",
    path?: string | null,
  ) =>
    invoke<string>("lyrics_save", {
      trackId: trackId && trackId > 0 ? trackId : null,
      path: path ?? null,
      lrcId,
      mode,
    }),
  lyricsExportSidecar: (
    trackId: number | null,
    overwrite: boolean,
    path?: string | null,
  ) =>
    invoke<string>("lyrics_export_sidecar", {
      trackId: trackId && trackId > 0 ? trackId : null,
      path: path ?? null,
      overwrite,
    }),
  lyricsEmbedSidecar: (trackId: number | null, path?: string | null) =>
    invoke<string>("lyrics_embed_sidecar", {
      trackId: trackId && trackId > 0 ? trackId : null,
      path: path ?? null,
    }),
  lyricsCurrent: (trackId: number | null, path?: string | null) =>
    invoke<import("./types").LyricsCurrent>("lyrics_current", {
      trackId: trackId && trackId > 0 ? trackId : null,
      path: path ?? null,
    }),

  // ── archive (归档状态) ──────────────────────────────────────────
  archiveCheckBatch: (trackIds: number[]) =>
    invoke<Record<number, import("./types").ArchiveStatus>>("archive_check_batch", {
      trackIds,
    }),
  archiveNormalize: (trackId: number) =>
    invoke<string>("archive_normalize", { trackId }),
  /** 只修一条归档意见：song_location | song_name | lyrics_location | lyrics_name */
  archiveNormalizeIssue: (trackId: number, kind: string) =>
    invoke<string>("archive_normalize_issue", { trackId, kind }),
  /** 库根杂项扫描（白名单外直接子项） */
  libraryRootScan: () =>
    invoke<import("./types").LibraryRootScan>("library_root_scan"),
  /** 杂项全部挪进 Unarchived/ */
  libraryRootOrganize: () =>
    invoke<import("./types").OrganizeResult>("library_root_organize"),
  /** 识别其他 AxMusic 库并生成对比预览 */
  libraryImportPreview: (path: string) =>
    invoke<import("./types").ImportPreview>("library_import_preview", { path }),
  /** 按勾选范围执行导入 */
  libraryImportRun: (path: string, selection: import("./types").ImportSelection) =>
    invoke<import("./types").ImportResult>("library_import_run", { path, selection }),

  // ── playlists (m3u8，存 <库>/playlists/) ────────────────────────
  playlistList: () => invoke<PlaylistSummary[]>("playlist_list"),
  /** 新建并一次写入条目（items 可空 = 空歌单） */
  playlistCreate: (name: string, items: PlaylistAddItem[]) =>
    invoke<PlaylistDetail>("playlist_create", { name, items }),
  playlistRename: (name: string, newName: string) =>
    invoke<PlaylistSummary>("playlist_rename", { name, newName }),
  playlistDelete: (name: string) => invoke<void>("playlist_delete", { name }),
  playlistGet: (name: string) => invoke<PlaylistDetail>("playlist_get", { name }),
  /** 追加条目（已在歌单里的自动跳过） */
  playlistAddTracks: (name: string, items: PlaylistAddItem[]) =>
    invoke<PlaylistDetail>("playlist_add_tracks", { name, items }),
  /** 移除条目（按 rel_path 定位，连续点击不怕列表错位） */
  playlistRemoveTrack: (name: string, relPath: string) =>
    invoke<PlaylistDetail>("playlist_remove_track", { name, relPath }),
  /** 条目上移/下移一格（delta = ±1，按 rel_path 定位） */
  playlistMoveTrack: (name: string, relPath: string, delta: number) =>
    invoke<PlaylistDetail>("playlist_move_track", { name, relPath, delta }),
  /** 批量清理失效条目（一次读写），返回清理后的歌单 */
  playlistCleanMissing: (name: string) =>
    invoke<PlaylistDetail>("playlist_clean_missing", { name }),
  // ── 喜爱（系统歌单「喜爱」的快捷读写）──────────────────────────
  favoritePaths: () => invoke<string[]>("favorite_paths"),
  favoriteToggle: (item: PlaylistAddItem) =>
    invoke<import("./types").FavoriteToggleResult>("favorite_toggle", { item }),
  // ── 听歌历史（since = Unix 毫秒下界，缺省全部；limit 缺省由后端裁决）──
  listenSummary: (since?: number) =>
    invoke<import("./types").ListenSummary>("listen_summary", { since: since ?? null }),
  listenTop: (kind: "track" | "album" | "artist", limit?: number, since?: number) =>
    invoke<import("./types").TopListenItem[]>("listen_top", {
      kind,
      limit: limit ?? null,
      since: since ?? null,
    }),
  listenRecent: (limit?: number, since?: number) =>
    invoke<import("./types").ListenEvent[]>("listen_recent", {
      limit: limit ?? null,
      since: since ?? null,
    }),
  /** 按日听歌量（年度报告趋势预留，UI 未接线） */
  listenDaily: (since?: number) =>
    invoke<import("./types").DailyListen[]>("listen_daily", { since: since ?? null }),
  listenHourHist: (since?: number) =>
    invoke<number[]>("listen_hour_hist", { since: since ?? null }),
};

export function formatTime(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) ms = 0;
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = s.toString().padStart(2, "0");
  // 超过一小时补小时位，避免 75:00 这种读法
  return h > 0 ? `${h}:${m.toString().padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

export function trackRowToQueueItem(t: TrackRow): QueueItem {
  return {
    path: t.path,
    title: t.title || t.filename,
    duration_ms: t.duration_ms,
  };
}

export function trackRowToAddItem(t: TrackRow): PlaylistAddItem {
  return {
    path: t.path,
    title: t.title || t.filename,
    artist: t.artist,
    duration_ms: t.duration_ms,
  };
}

export function entryToQueueItem(e: import("./types").PlaylistEntry): QueueItem {
  const title = e.track?.title || e.title;
  return {
    path: e.path,
    title: title || e.path.split(/[\\/]/).pop() || e.path,
    duration_ms: e.duration_ms,
  };
}

export function folderFileToQueueItem(f: import("./types").FolderFile): QueueItem {
  return {
    path: f.path,
    title: f.title || f.name,
    duration_ms: f.duration_ms,
  };
}

export function folderFileToAddItem(f: import("./types").FolderFile): PlaylistAddItem {
  return {
    path: f.path,
    title: f.title || f.name,
    artist: f.artist,
    duration_ms: f.duration_ms,
  };
}
