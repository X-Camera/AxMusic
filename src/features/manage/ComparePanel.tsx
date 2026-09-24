import { useCallback, useEffect, useMemo, useState } from "react";
import { FileInput, ImagePlus, Loader2, Search } from "lucide-react";
import { api } from "../../lib/api";
import type { CatalogRow, FieldChange, TrackRow } from "../../lib/types";
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

/** 未关联时可编辑的文件标签字段 */
const EDIT_FIELDS = ["title", "artist", "album", "album_artist", "year", "track_no", "release_type"] as const;

export interface CompareData {
  track: TrackRow;
  catalog: CatalogRow | null;
  changes: FieldChange[];
  /** catalog 缓存封面（data URL），未刮取为 null */
  cover_data: string | null;
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

  /** 任一写操作在途即锁住全部写按钮：它们最终都写同一个音频文件，并发会相互覆盖 */
  const writing = writingField !== null || writingCover || writingTags;

  useEffect(() => {
    let cancelled = false;
    setData(null);
    setError(null);
    setFileCover(null);
    setCatalogCover(null);
    setDraft({});
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
            release_type: d.track.release_type || "",
          });
          if (d.track.has_cover) {
            const thumb = await api.trackCoverThumb(d.track.path).catch(() => null);
            if (!cancelled) setFileCover(thumb);
          }
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
      release_type: t.release_type || "",
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
            {EDIT_FIELDS.map((field) => (
              <label key={field} className="cmp-field-edit">
                <span className="cmp-field-label">{FIELD_LABEL[field] ?? field}</span>
                <input
                  value={draft[field] ?? ""}
                  onChange={(e) => setDraft((d) => ({ ...d, [field]: e.target.value }))}
                  placeholder="—"
                />
              </label>
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
          {fields.map((f) => (
            <div key={f.field} className={`cmp-field${f.changed ? " changed" : ""}`}>
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
          <div className="tertiary cmp-hint">
            带写入按钮的字段与文件不一致；catalog 空值不写入。
          </div>
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
