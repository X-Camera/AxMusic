import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { FileInput, FolderCheck, ImagePlus, Loader2, Search, Gauge } from "lucide-react";
import { api } from "../../lib/api";
import type {
  ArchiveStatus,
  CatalogRow,
  FieldChange,
  ReplayGainScan,
  ReplayGainTags,
  TrackRow,
} from "../../lib/types";
import { useFavorites } from "../../state/useFavorites";
import { CoverPicker } from "./CoverPicker";
import "./ComparePanel.css";

const FIELD_LABEL: Record<string, string> = {
  title: "曲名",
  artist: "歌手",
  album: "专辑",
  album_artist: "专辑艺人",
  year: "年份",
  track_no: "轨号",
  release_type: "专辑类型",
  musicbrainz_recording: "MB 录音",
  musicbrainz_release: "MB 发行",
};

const FIELD_ORDER = [
  "title",
  "artist",
  "album",
  "album_artist",
  "year",
  "track_no",
  "release_type",
  "musicbrainz_recording",
  "musicbrainz_release",
];

/** 紧凑排版分组：一行两个字段；剩余字段单独一行 */
const FIELD_GROUPS: string[][] = [
  ["title", "album"],
  ["artist", "album_artist"],
  ["year", "track_no"],
  ["release_type"],
  ["musicbrainz_recording"],
  ["musicbrainz_release"],
];

/** 未关联时可编辑的文件标签字段（与 catalog 双列同构；不放专辑类型） */
const EDIT_GROUPS: readonly (readonly string[])[] = [
  ["title", "album"],
  ["artist", "album_artist"],
  ["year", "track_no"],
];
const EDIT_FIELDS: readonly string[] = EDIT_GROUPS.flat();

export interface CompareData {
  track: TrackRow;
  catalog: CatalogRow | null;
  changes: FieldChange[];
  /** catalog 缓存封面（data URL），未刮取为 null */
  cover_data: string | null;
  /** 本次比较刚写入 catalog_id（列表需刷新绿字） */
  linked_now?: boolean;
}

export function ComparePanel({
  trackId,
  onWritten,
  onSearchLyrics,
  onScrape,
}: {
  trackId: number;
  onWritten: () => void;
  /** 打开当前曲目的搜索歌词面板 */
  onSearchLyrics: () => void;
  /** 打开当前曲目的刮削向导 */
  onScrape: () => void;
}) {
  const [data, setData] = useState<CompareData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [writingField, setWritingField] = useState<string | null>(null);
  const [writingCover, setWritingCover] = useState(false);
  const [writingTags, setWritingTags] = useState(false);
  const [draft, setDraft] = useState<Record<string, string>>({});
  const [fileCover, setFileCover] = useState<string | null>(null);
  const [catalogCover, setCatalogCover] = useState<string | null>(null);
  const [coverOpen, setCoverOpen] = useState(false);
  const [archive, setArchive] = useState<ArchiveStatus | null>(null);
  const [normalizing, setNormalizing] = useState<string | null>(null);
  const [rgScan, setRgScan] = useState<ReplayGainScan | null>(null);
  const [rgTags, setRgTags] = useState<ReplayGainTags | null>(null);
  const [scanningRg, setScanningRg] = useState(false);
  const [writingRg, setWritingRg] = useState(false);

  /** 任一写操作在途即锁住全部写按钮：它们最终都写同一个音频文件，并发会相互覆盖 */
  const writing =
    writingField !== null || writingCover || writingTags || normalizing !== null || writingRg;

  // onWritten 身份随父组件重渲染变化；用 ref 保持加载 effect 稳定
  const onWrittenRef = useRef(onWritten);
  useEffect(() => {
    onWrittenRef.current = onWritten;
  }, [onWritten]);

  useEffect(() => {
    let cancelled = false;
    setData(null);
    setError(null);
    setFileCover(null);
    setCatalogCover(null);
    setDraft({});
    setArchive(null);
    setRgScan(null);
    setRgTags(null);
    (async () => {
      try {
        const d = await api.catalogCompare(trackId);
        if (!cancelled) {
          setData(d);
          setCatalogCover(d.cover_data);
          setDraft({
            title: d.track.title || "",
            artist: d.track.artist || "",
            album: d.track.album || "",
            album_artist: d.track.album_artist || "",
            year: d.track.year || "",
            track_no: d.track.track_no != null ? String(d.track.track_no) : "",
          });
          if (d.track.has_cover) {
            const thumb = await api.trackCoverThumb(d.track.path).catch(() => null);
            if (!cancelled) setFileCover(thumb);
          }
          // 归档状态：仅已关联 catalog 时有意义（未关联由 UI 提示先刮削）
          if (d.track.catalog_id != null) {
            const map = await api.archiveCheckBatch([trackId]).catch(() => null);
            if (!cancelled && map && map[trackId] != null) setArchive(map[trackId]);
          }
          // 已有 REPLAYGAIN 标签：选中即显示，不必先扫描
          const tags = await api.replaygainTags(trackId).catch(() => null);
          if (!cancelled && tags) setRgTags(tags);
          // 本次比较补上了关联 → 刷列表，让绿字/未关联筛选立刻更新
          if (!cancelled && d.linked_now) onWrittenRef.current();
        }
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [trackId]);

  const title = useMemo(() => {
    if (!data) return "";
    const artist = data.catalog?.artist || data.track.artist || "未知歌手";
    const title = data.catalog?.title || data.track.title || data.track.filename;
    return `${artist} - ${title}`;
  }, [data]);

  const fields = useMemo(() => {
    const cat = data?.catalog;
    const track = data?.track;
    if (!cat || !track) return [];
    const oldOf: Record<string, string> = {
      title: track.title,
      artist: track.artist,
      album: track.album,
      album_artist: track.album_artist,
      year: track.year,
      track_no: track.track_no != null ? String(track.track_no) : "",
      release_type: track.release_type,
      musicbrainz_recording: track.mb_recording_mbid,
      musicbrainz_release: track.mb_release_mbid,
    };
    const newOf: Record<string, string> = {
      title: cat.title,
      artist: cat.artist,
      album: cat.album,
      album_artist: cat.album_artist,
      year: cat.year,
      track_no: cat.track_no != null ? String(cat.track_no) : "",
      release_type: cat.release_type,
      musicbrainz_recording: cat.mbid,
      musicbrainz_release: cat.release_mbid,
    };
    return FIELD_ORDER.map((field) => {
      const value = newOf[field] ?? "";
      const old = oldOf[field] ?? "";
      const changed = old.trim() !== value.trim();
      const writable = value.trim() !== "";
      return {
        field,
        label: FIELD_LABEL[field] ?? field,
        value,
        changed,
        writable: changed && writable,
      };
    }).filter((f) => f.value.trim() !== "" || f.field === "track_no");
  }, [data]);

  const writeField = useCallback(
    async (field: string) => {
      setWritingField(field);
      setError(null);
      try {
        await api.catalogApplyToTrack(trackId, [field], false);
        onWritten();
      } catch (e) {
        setError(String(e));
      } finally {
        setWritingField(null);
      }
    },
    [trackId, onWritten],
  );

  const writeCover = useCallback(async () => {
    setWritingCover(true);
    setError(null);
    try {
      await api.catalogApplyToTrack(trackId, [], true);
      onWritten();
    } catch (e) {
      setError(String(e));
    } finally {
      setWritingCover(false);
    }
  }, [trackId, onWritten]);

  const dirtyFields = useMemo(() => {
    if (!data) return [] as { field: string; old: string; new: string }[];
    const t = data.track;
    const base: Record<string, string> = {
      title: t.title || "",
      artist: t.artist || "",
      album: t.album || "",
      album_artist: t.album_artist || "",
      year: t.year || "",
      track_no: t.track_no != null ? String(t.track_no) : "",
    };
    return EDIT_FIELDS.map((field) => ({
      field,
      old: base[field] ?? "",
      new: draft[field] ?? "",
      // 「与旧值不同即变更」：清空也是变更（写入文件 = 显式删除该标签）
    })).filter((c) => c.old.trim() !== c.new.trim());
  }, [data, draft]);

  const writeTags = useCallback(async () => {
    if (dirtyFields.length === 0) return;
    setWritingTags(true);
    setError(null);
    try {
      await api.trackWriteTags(trackId, dirtyFields);
      onWritten();
    } catch (e) {
      setError(String(e));
    } finally {
      setWritingTags(false);
    }
  }, [trackId, dirtyFields, onWritten]);

  const normalizeIssue = useCallback(
    async (kind: string) => {
      setNormalizing(kind);
      setError(null);
      try {
        await api.archiveNormalizeIssue(trackId, kind);
        // 重新检查归档状态
        const map = await api.archiveCheckBatch([trackId]);
        if (map[trackId] != null) setArchive(map[trackId]);
        // 路径变了：喜爱/歌单条目靠兜底重匹配自愈，这里刷心形对照键
        void useFavorites.getState().reload();
        onWritten();
      } catch (e) {
        setError(String(e));
      } finally {
        setNormalizing(null);
      }
    },
    [trackId, onWritten],
  );

  const scanReplayGain = useCallback(async () => {
    setScanningRg(true);
    setError(null);
    try {
      const r = await api.replaygainAnalyze(trackId);
      setRgScan(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setScanningRg(false);
    }
  }, [trackId]);

  const writeReplayGain = useCallback(async () => {
    // 0 是合法增益；只排除 null/undefined，与按钮渲染条件同口径
    if (rgScan?.track_gain_db == null) return;
    setWritingRg(true);
    setError(null);
    try {
      await api.replaygainWrite(trackId, rgScan.track_gain_db, rgScan.track_peak);
      // 面板会因 onWritten 换 key 重挂载并重新读标签；这里只通知刷新列表
      onWritten();
    } catch (e) {
      setError(String(e));
    } finally {
      setWritingRg(false);
    }
  }, [trackId, rgScan, onWritten]);

  const formatDb = (db: number | null | undefined) =>
    db === null || db === undefined ? "—" : `${db >= 0 ? "+" : ""}${db.toFixed(2)} dB`;

  return (
    <aside className="cmp-panel" role="complementary" aria-label="catalog 字段">
      <header className="cmp-head">
        <h2 title={title}>{title}</h2>
        <div className="cmp-head-actions">
          <button className="btn" title="MusicBrainz 单曲刮削（写入 catalog）" onClick={onScrape}>
            刮削
          </button>
          <button
            className="btn"
            onClick={onSearchLyrics}
            title="在线搜索歌词（LRCLIB / 网易云 / QQ音乐）"
          >
            <Search size={14} /> 歌词
          </button>
        </div>
      </header>
      {error && <div className="error-line cmp-error">{error}</div>}
      {!data && !error && <div className="tertiary cmp-empty">加载中…</div>}
      {/* 封面展示与刮取不依赖 catalog：未刮削也能看文件封面、刮封面 */}
      {data && (
        <div className="cmp-covers">
          <figure className="cmp-cover">
            {catalogCover ? (
              <img src={catalogCover} alt="封面" />
            ) : fileCover ? (
              <img src={fileCover} alt="封面" />
            ) : (
              <div className="cmp-cover-empty tertiary">无封面，可刮取</div>
            )}
            <figcaption className="muted">
              {catalogCover ? "库封面" : fileCover ? "文件封面" : "封面"}
            </figcaption>
          </figure>
          <div className="cmp-cover-actions">
            <button
              className="btn"
              title="多源搜索封面（CAA / iTunes / 网易云 / QQ音乐），点选一张采纳到库 covers/"
              onClick={() => setCoverOpen(true)}
            >
              <ImagePlus size={14} />
              {catalogCover ? "重新刮取封面" : "刮取封面"}
            </button>
            <button
              className="btn"
              disabled={!catalogCover || writing}
              title="将库封面写入歌曲文件"
              onClick={() => void writeCover()}
            >
              {writingCover ? <Loader2 size={14} className="spin" /> : <FileInput size={14} />}
              写入封面
            </button>
          </div>
        </div>
      )}
      {data && !data.catalog && (
        <>
          <div className="tertiary cmp-hint">
            尚未关联 catalog。可改正文件标签后写入，有助于刮削/匹配；把字段清空再写入 = 删除该标签。
          </div>
          <div className="cmp-fields">
            <div className="cmp-fields-title">文件字段（可编辑）</div>
            {EDIT_GROUPS.map((group, gi) => (
              <div
                key={gi}
                className={`cmp-field-row${group.length === 1 ? " single" : ""}`}
              >
                {group.map((field) => (
                  <label key={field} className="cmp-field-compact cmp-field-edit">
                    <span className="cmp-field-label">{FIELD_LABEL[field] ?? field}</span>
                    <input
                      value={draft[field] ?? ""}
                      onChange={(e) => setDraft((d) => ({ ...d, [field]: e.target.value }))}
                      placeholder="—"
                    />
                  </label>
                ))}
              </div>
            ))}
            <div className="cmp-edit-actions">
              <button
                className="btn btn-primary"
                disabled={writing || dirtyFields.length === 0}
                title="将修改写入歌曲文件标签（清空某字段 = 删除该标签）"
                onClick={() => void writeTags()}
              >
                {writingTags ? <Loader2 size={14} className="spin" /> : <FileInput size={14} />}
                写入文件
                {dirtyFields.length > 0 ? `（${dirtyFields.length}）` : ""}
              </button>
            </div>
          </div>
        </>
      )}
      {data && data.catalog && (
        <div className="cmp-fields">
          <div className="cmp-fields-title">catalog 字段</div>
          {FIELD_GROUPS.map((group, gi) => {
            const items = group
              .map((field) => fields.find((f) => f.field === field))
              .filter(Boolean) as typeof fields;
            if (items.length === 0) return null;
            return (
              <div key={gi} className={`cmp-field-row${items.length === 1 ? " single" : ""}`}>
                {items.map((f) => (
                  <div key={f.field} className={`cmp-field-compact${f.changed ? " changed" : ""}`}>
                    <span className="cmp-field-label">{f.label}</span>
                    <span className="cmp-field-value" title={f.value || "—"}>
                      {f.value || "—"}
                    </span>
                    {f.writable ? (
                      <button
                        className="icon-btn"
                        title="写入歌曲文件"
                        disabled={writing}
                        onClick={() => void writeField(f.field)}
                      >
                        {writingField === f.field ? (
                          <Loader2 size={14} className="spin" />
                        ) : (
                          <FileInput size={14} />
                        )}
                      </button>
                    ) : (
                      <span className="icon-btn-placeholder" />
                    )}
                  </div>
                ))}
              </div>
            );
          })}
          <div className="tertiary cmp-hint">
            带写入按钮的字段与文件不一致；catalog 空值不写入。
          </div>
        </div>
      )}
      {/* 归档状态：仅已关联 catalog；未关联提示先刮削 */}
      {data && data.track.catalog_id == null && (
        <div className="cmp-archive">
          <div className="cmp-fields-title">归档状态</div>
          <div className="tertiary cmp-hint">先要刮削，关联 catalog 后再整理归档</div>
        </div>
      )}
      {data && data.track.catalog_id != null && archive && (
        <div className="cmp-archive">
          <div className="cmp-fields-title">
            归档状态
            {archive.ok ? (
              <span className="cmp-archive-ok">✓ 规范</span>
            ) : (
              <span className="cmp-archive-bad">
                ? {archive.issues.length} 项不规范
              </span>
            )}
          </div>
          {!archive.ok && (
            <ul className="cmp-archive-issues">
              {archive.issues.map((issue, i) => (
                <li key={`${issue.kind}-${i}`} className="cmp-archive-issue">
                  <div className="cmp-archive-issue-row">
                    <span className="cmp-archive-msg">{issue.message}</span>
                    <button
                      className="btn btn-sm"
                      disabled={writing}
                      title={
                        issue.kind.endsWith("location")
                          ? "只挪位置，保留当前文件名"
                          : "只改命名，留在当前目录"
                      }
                      onClick={() => void normalizeIssue(issue.kind)}
                    >
                      {normalizing === issue.kind ? (
                        <Loader2 size={12} className="spin" />
                      ) : (
                        <FolderCheck size={12} />
                      )}
                      整理
                    </button>
                  </div>
                  {issue.current && (
                    <span className="cmp-archive-path tertiary" title={issue.current}>
                      当前：{issue.current}
                    </span>
                  )}
                  <span className="cmp-archive-path tertiary" title={issue.expected}>
                    期望：{issue.expected}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
      {/* 响度增益：已有标签常显；扫描估算 → 确认写入（不自动改文件） */}
      {data && (
        <div className="cmp-rg">
          <div className="cmp-fields-title">响度增益</div>
          <div className="tertiary cmp-hint">
            文件标签常显；扫描可估算建议增益。写入后播放按「响度均衡」自动拉平音量。
          </div>
          <div className="cmp-rg-result">
            <div className="cmp-field-row">
              <div className="cmp-field-compact">
                <span className="cmp-field-label">曲目增益</span>
                <span className="cmp-field-value mono" title={formatDb(rgTags?.track_gain_db)}>
                  {formatDb(rgTags?.track_gain_db)}
                </span>
              </div>
              <div className="cmp-field-compact">
                <span className="cmp-field-label">曲目峰值</span>
                <span className="cmp-field-value mono">
                  {rgTags?.track_peak != null ? rgTags.track_peak.toFixed(4) : "—"}
                </span>
              </div>
            </div>
            <div className="cmp-field-row">
              <div className="cmp-field-compact">
                <span className="cmp-field-label">专辑增益</span>
                <span className="cmp-field-value mono" title="单曲扫描不写专辑增益">
                  {formatDb(rgTags?.album_gain_db)}
                </span>
              </div>
              <div className="cmp-field-compact">
                <span className="cmp-field-label">专辑峰值</span>
                <span className="cmp-field-value mono">
                  {rgTags?.album_peak != null ? rgTags.album_peak.toFixed(4) : "—"}
                </span>
              </div>
            </div>
          </div>
          <div className="cmp-rg-row">
            <button
              className="btn btn-sm"
              disabled={scanningRg || writing}
              title="解码分析响度与峰值，不修改文件"
              onClick={() => void scanReplayGain()}
            >
              {scanningRg ? <Loader2 size={12} className="spin" /> : <Gauge size={12} />}
              {rgScan ? "重新扫描" : "扫描增益"}
            </button>
            {rgScan && rgScan.track_gain_db != null && (
              <button
                className="btn btn-sm btn-primary"
                disabled={writing || scanningRg}
                title="写入 REPLAYGAIN_TRACK_GAIN / TRACK_PEAK（已有曲目增益会被覆盖）"
                onClick={() => void writeReplayGain()}
              >
                {writingRg ? <Loader2 size={12} className="spin" /> : <FileInput size={12} />}
                写入标签
              </button>
            )}
          </div>
          {rgScan && (
            <div className="cmp-rg-result">
              <div className="cmp-field-row">
                <div className="cmp-field-compact">
                  <span className="cmp-field-label">测量响度</span>
                  <span className="cmp-field-value mono">
                    {rgScan.measured_lufs != null
                      ? `${rgScan.measured_lufs.toFixed(1)} LUFS`
                      : "—"}
                  </span>
                </div>
                <div className="cmp-field-compact">
                  <span className="cmp-field-label">建议增益</span>
                  <span className="cmp-field-value mono">{formatDb(rgScan.track_gain_db)}</span>
                </div>
              </div>
              <div className="cmp-field-row">
                <div className="cmp-field-compact">
                  <span className="cmp-field-label">测量峰值</span>
                  <span className="cmp-field-value mono">
                    {rgScan.track_peak != null ? rgScan.track_peak.toFixed(4) : "—"}
                  </span>
                </div>
                <div className="cmp-field-compact">
                  <span className="cmp-field-label">写入说明</span>
                  <span className="cmp-field-value" title="峰值用于播放端防削波">
                    增益+峰值
                  </span>
                </div>
              </div>
            </div>
          )}
        </div>
      )}
      {coverOpen && (
        <CoverPicker
          trackId={trackId}
          onClose={() => setCoverOpen(false)}
          onApplied={(coverData) => {
            setCatalogCover(coverData);
            setCoverOpen(false);
          }}
        />
      )}
    </aside>
  );
}
