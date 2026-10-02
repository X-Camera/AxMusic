export type PlayStatus = "Stopped" | "Playing" | "Paused";
/** 循环模式（与随机正交）：关 → 列表循环 → 单曲循环 */
export type RepeatMode = "off" | "all" | "one";

/** 响度均衡模式：关 / 按曲目 / 按专辑 */
export type ReplayGainMode = "off" | "track" | "album";

/** 增益来源：track/album 来自文件标签；estimated 为无标签时运行时估算（不写文件） */
export type ReplayGainSource = "none" | "track" | "album" | "estimated";

/** 本曲响度均衡状态（播放条标识用） */
export interface ReplayGainInfo {
  /** 是否启用了补偿（模式非关且读到标签） */
  active: boolean;
  /** 实际应用的增益 (dB)；未启用为 0 */
  applied_gain_db: number;
  source: ReplayGainSource;
  track_gain_db: number | null;
  album_gain_db: number | null;
  track_peak: number | null;
  album_peak: number | null;
  /** 峰值限幅把增益收紧过（防削波） */
  peak_limited: boolean;
  /** 限幅前的原始增益 (dB) */
  requested_gain_db: number;
}

/** 单曲扫描估算结果（管理右栏） */
export interface ReplayGainScan {
  /** 门限积分响度 LUFS；解码失败或整轨静音为 null */
  measured_lufs: number | null;
  /** 建议曲目增益 dB；measured_lufs 为 null 时同为 null */
  track_gain_db: number | null;
  /** 样本峰值（线性）；将与增益一并写入标签 */
  track_peak: number | null;
  existing_track_gain_db: number | null;
  existing_track_peak: number | null;
  /** 曲目增益或峰值任一存在即为 true（非「两者都有」） */
  has_track_tags: boolean;
}

/** 文件里已有的 ReplayGain/R128 标签（不解码只读） */
export interface ReplayGainTags {
  track_gain_db: number | null;
  track_peak: number | null;
  album_gain_db: number | null;
  album_peak: number | null;
}

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
  shuffle: boolean;
  repeat: RepeatMode;
  /** 响度均衡状态（未启用/无标签为默认零值） */
  replaygain: ReplayGainInfo;
  /** 引擎上报的错误（如格式暂不支持）；取出即清，收到后弹一次提示 */
  error: string | null;
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
  /** 碟号（多碟发行；无碟号信息为 null） */
  disc_no: number | null;
  duration_ms: number;
  format: string;
  sample_rate: number | null;
  bit_rate: number | null;
  has_cover: boolean;
  /** 内嵌歌词（标签内） */
  has_lyrics: boolean;
  /** 外挂歌词（库 lrc/ 或同目录 .lrc） */
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
  /** 专辑聚合稳定键，取曲目用它 */
  group_key: string;
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
  /** 排序字段，默认 title（曲名） */
  sort?: TrackSortField | "album_artist" | "duration" | "filename" | null;
  /** 升/降序，默认 asc */
  sort_dir?: "asc" | "desc" | null;
}

/** 管理表可点列排序字段（TrackFilter.sort 的子集） */
export type TrackSortField = "title" | "artist" | "album" | "year" | "track_no" | "format";

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

/** 资源管理器右键菜单注册状态（仅 Windows 可注册） */
export interface ShellMenuStatus {
  supported: boolean;
  registered: boolean;
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

/** 满窗歌词字体 */
export type LyricsFont =
  | "display"
  | "yahei"
  | "dengxian"
  | "kaiti"
  | "songti"
  | "heiti";

export interface AppSettings {
  library_root: string | null;
  volume: number;
  shuffle: boolean;
  repeat: RepeatMode;
  restore_volume: boolean;
  /** 响度均衡：off / track / album */
  replaygain_mode: ReplayGainMode;
  lyrics_save_mode: LyricsSaveMode;
  lyrics_prefer: LyricsPrefer;
  lyrics_sources: LyricsSources;
  /** 满窗歌词字号缩放 0.75–1.5 */
  lyrics_font_scale: number;
  lyrics_font: LyricsFont;
  /** 满窗歌词行间距（主句 line-height） */
  lyrics_line_height: number;
  /** 主界面歌词字号缩放（与满窗分开） */
  side_lyrics_font_scale: number;
  side_lyrics_font: LyricsFont;
  /** 主界面歌词行间距 */
  side_lyrics_line_height: number;
  /** 主界面歌词区背景动效 */
  side_viz: SideVizSettings;
  songs_view: SongsView;
  close_behavior: CloseBehavior;
  theme_mode: ThemeMode;
  color_scheme: ColorScheme;
}

export type SideVizKind =
  | "aurora"
  | "spectrum"
  | "particles"
  | "radial-bars"
  | "radial-line"
  | "fluid"
  | "silk";

/** 同一效果的配色风格：素雅（单色白）/ 柔和 / 炫酷（多彩渐变）/ 封面（专辑取色） */
/** 色彩丰富程度：素雅（纯黑白灰）/ 柔和（单色渐变）/ 炫酷（双色渐变） */
export type SideVizPalette = "mono" | "soft" | "vivid";
/** 主色来源：跟随主题强调色 / 跟随封面取色 / 自选颜色（color 字段） */
export type SideVizColorSource = "theme" | "cover" | "custom";

/** 「封面流体」专属参数（0–1，前端映射物理量） */
export interface FluidVizParams {
  /** 模糊度 → CSS blur 6–32px */
  blur: number;
  /** 低音呼吸幅度 */
  breathe: number;
  /** 旋转/漂移速度系数 */
  spin: number;
}

/** 「极光丝绸」专属参数 */
export interface SilkVizParams {
  /** 流速 */
  flow: number;
  /** 层次复杂度 → fbm octaves 3–6 */
  complexity: number;
  /** 亮度 */
  brightness: number;
}

/** 「频谱」专属参数 */
export interface SpectrumVizParams {
  /** 柱数 16–128 */
  bars: number;
  /** 发光强度 0=关 */
  glow: number;
  /** 峰值滞留点 */
  peaks: boolean;
  /** 上下镜像 */
  mirror: boolean;
}

/** 「粒子」专属参数 */
export interface ParticlesVizParams {
  /** 粒子数 8–160 */
  count: number;
  /** 近距连线（plexus） */
  links: boolean;
  /** 连线距离系数 0–1 */
  link_dist: number;
  /** 粒子大小系数 0–1 */
  size: number;
}

/** 「环形」（环柱/环线共用）专属参数 */
export interface RadialVizParams {
  /** 主圆环大小 0–1 → min(w,h)×0.08–0.30 */
  radius: number;
  /** 外伸长度（环柱）/ 波形幅度（环线）系数 0–1 */
  out_len: number;
  /** 内伸长度系数 0–1（仅环柱） */
  in_len: number;
  /** 内环粒子发射量 0–200（仅环线，0=关） */
  emit: number;
  /** 粒子触发灵敏度 0–1（越高越容易触发；仅环线） */
  sensitivity: number;
}

/** 每个效果各自一份的公共参数 */
export interface VizCommonParams {
  palette: SideVizPalette;
  /** 主色来源 */
  color_source: SideVizColorSource;
  /** 主色（hex，如 #82aaff）；color_source=custom 时生效 */
  color: string;
  /** 0–1 强度 */
  intensity: number;
  /** 0–1 图层不透明度 */
  opacity: number;
  /** 0.2–2 动画速度 */
  speed: number;
  /** 渲染缩放 0.5 / 0.75 / 1（省 GPU） */
  render_scale: number;
  /** 帧率上限 30 / 60 */
  fps_cap: number;
}

export interface SideVizSettings {
  enabled: boolean;
  kind: SideVizKind;
  /** 公共参数按效果各存一份（key = SideVizKind） */
  commons: Record<SideVizKind, VizCommonParams>;
  fluid: FluidVizParams;
  silk: SilkVizParams;
  spectrum_ex: SpectrumVizParams;
  particles_ex: ParticlesVizParams;
  radial_ex: RadialVizParams;
}

/** 传给效果层的设置：side_viz 全量 + 当前效果的公共参数摊平 */
export type EffectiveVizSettings = SideVizSettings & VizCommonParams;

export type SettingsPatch = Partial<
  Omit<AppSettings, "library_root">
>;

export type RouteId =
  | "songs"
  | "albums"
  | "artists"
  | "folders"
  | "playlists"
  | "history"
  | "manage"
  | "settings";

/** 一次有效听歌事件（listen_history.db：库内或 data_root，展示汇总去重） */
export interface ListenEvent {
  id: number;
  /** Unix 毫秒 */
  started_at: number;
  ended_at: number;
  play_ms: number;
  track_duration_ms: number;
  path: string;
  title: string;
  artist: string;
  album: string;
  album_artist: string;
  year: string;
  track_no: number | null;
  mb_recording_mbid: string;
  catalog_id: number | null;
  /** 通常 "player"；导入记录保留源侧取值 */
  source: string;
  /** natural | skip | exit | interrupted（进行中/被杀占位） */
  end_reason: string;
}

export interface ListenSummary {
  total_plays: number;
  total_ms: number;
  unique_tracks: number;
  unique_artists: number;
  unique_albums: number;
}

export interface TopListenItem {
  /** 聚合键（mbid 或 title|artist / artist / album_artist|album 的小写归一），非稳定 id */
  key: string;
  /** 组内 MAX 代表值，仅展示；kind=album 时 title/artist 不可靠 */
  title: string;
  artist: string;
  album: string;
  plays: number;
  total_ms: number;
  /** 组内代表文件路径（播放 / 封面） */
  path: string;
  /** 组内曲目时长代表值（入队用） */
  track_duration_ms: number;
}

export interface DailyListen {
  /** YYYY-MM-DD */
  date: string;
  plays: number;
  total_ms: number;
}

export interface ScrapeCandidate {
  id: string;
  kind: string;
  /** 来源：musicbrainz / itunes / netease / qq */
  source: string;
  title: string;
  artist: string;
  year: string;
  track_count: number;
  country: string;
  disambiguation: string;
  release_id: string;
}

/** 单曲所属专辑（挑专辑用；release_id 可喂给 scrapeBuildPlan 的 album 模式） */
export interface TrackAlbum {
  source: string;
  release_id: string;
  title: string;
  artist: string;
  year: string;
  track_count: number;
  country: string;
  /** Album / Single / Compilation / Live…（仅 MB） */
  release_type: string;
  disambiguation: string;
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
  /** 碟号（多碟发行；单碟/无信息为 null） */
  disc_no: number | null;
  release_type: string;
}

export interface ApplyPlan {
  candidate_id: string;
  release_id: string;
  /** 来源（写入 catalog.source） */
  source: string;
  candidate_label: string;
  tracks: TrackPlan[];
  /** 采纳后存入本地 catalog 的完整曲目表（专辑模式为整张） */
  catalog_tracks: CatalogTrackDraft[];
  unmatched: string[];
}

/** 刮削搜索流式批次（scrape://batch 事件载荷） */
export interface ScrapeBatch {
  searchId: number;
  source: string;
  items: ScrapeCandidate[];
  error?: string;
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
  /** 碟号（多碟发行；单碟/无信息为 null） */
  disc_no: number | null;
  release_type: string;
  cover_path: string | null;
  created_at: string;
}

export interface CompareData {
  track: TrackRow;
  catalog: CatalogRow | null;
  /** 全部匹配候选（MBID/字段），可切换后按指定条写入 */
  matches?: CatalogRow[];
  changes: FieldChange[];
  /** catalog 缓存封面（data URL），未刮取为 null */
  cover_data: string | null;
  /** 本次比较刚写入 catalog_id（列表需刷新绿字） */
  linked_now?: boolean;
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
  /** 前端生成的搜索代次，用于丢弃过期批次（库外 trackId 恒 0 不可作代次） */
  searchId: number;
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

// ── archive (归档状态) ──────────────────────────────────────────────

/** 一项归档问题 */
export interface ArchiveIssue {
  /** "song_location" | "song_name" | "song_strip_version" | "lyrics_location" | "lyrics_name" */
  kind: string;
  message: string;
  current: string;
  expected: string;
  /** true = 可选建议（如去除末尾版本括号），不影响规范判定 */
  optional: boolean;
}

/** 单曲归档状态 */
export interface ArchiveStatus {
  ok: boolean;
  issues: ArchiveIssue[];
}

// ── 库根杂项扫描 ────────────────────────────────────────────────────

/** 库根白名单外的一项 */
export interface LibraryRootItem {
  name: string;
  path: string;
  is_dir: boolean;
  /** "audio" | "lyrics" | "other_file" | "other_dir" */
  kind: string;
}

/** 库根杂项扫描结果 */
export interface LibraryRootScan {
  ok: boolean;
  items: LibraryRootItem[];
}

/** 杂项整理结果 */
export interface OrganizeResult {
  moved: string[];
  failed: string[];
}

// ── 导入其他库 ──────────────────────────────────────────────────────

/** 一类可导入内容的对比数字 */
export interface ImportItemStats {
  /** 源库条目总数 */
  source_total: number;
  /** 与当前库重复（导入时跳过） */
  duplicate: number;
  /** 将新增 */
  new: number;
  /** 源文件缺失，无法复制（仅「歌曲」会非 0） */
  missing: number;
}

/** 导入预览：源库识别结果 + 与当前库对比 */
export interface ImportPreview {
  source_root: string;
  current_root: string;
  /** 已刮削数据库（catalog） */
  catalog: ImportItemStats;
  /** 歌曲（音频文件 + tracks 记录） */
  songs: ImportItemStats;
  /** 外挂歌词（lrc/） */
  lyrics: ImportItemStats;
  /** 封面（covers/） */
  covers: ImportItemStats;
  /** 歌单（playlists/*.m3u8） */
  playlists: ImportItemStats;
  /** 听歌记录（源侧 data/listen_history.db） */
  listen: ImportItemStats;
  /** 源侧听歌库路径；未找到为 null */
  listen_path: string | null;
}

/** 用户勾选的导入范围（默认全不选） */
export interface ImportSelection {
  catalog: boolean;
  songs: boolean;
  lyrics: boolean;
  covers: boolean;
  playlists: boolean;
  listen: boolean;
}

/** 导入执行结果 */
export interface ImportResult {
  catalog_added: number;
  catalog_skipped: number;
  songs_added: number;
  songs_skipped: number;
  songs_failed: number;
  lyrics_added: number;
  lyrics_skipped: number;
  covers_added: number;
  covers_skipped: number;
  playlists_added: number;
  playlists_skipped: number;
  listen_added: number;
  listen_skipped: number;
  /** 导入后 auto_match 关联上的曲目数 */
  tracks_linked: number;
  errors: string[];
}
