import { useEffect, useMemo, useState } from "react";
import { ImagePlus, Search } from "lucide-react";
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
}: {
  trackId: number;
  onWritten: () => void;
  /** 打开当前曲目的搜索歌词面板 */
  onSearchLyrics: () => void;
}) {
  const [data, setData] = useState<CompareData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [writing, setWriting] = useState(false);
  const [writeCover, setWriteCover] = useState(true);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [fileCover, setFileCover] = useState<string | null>(null);
  const [catalogCover, setCatalogCover] = useState<string | null>(null);
  const [coverOpen, setCoverOpen] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setData(null);
    setError(null);
    setPicked(new Set());
    setFileCover(null);
    setCatalogCover(null);
    (async () => {
      try {
        const d = await api.catalogCompare(trackId);
        if (!cancelled) {
          setData(d);
          setCatalogCover(d.cover_data);
          // 默认勾选全部有实际差异且 catalog 有值的字段
          const init = new Set(
            d.changes
              .filter((c) => c.new.trim() !== "" && c.old.trim() !== c.new.trim())
              .map((c) => c.field),
          );
          setPicked(init);
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

  const real = useMemo(
    () =>
      (data?.changes ?? []).filter(
        (c) => c.new.trim() !== "" && c.old.trim() !== c.new.trim(),
      ),
    [data],
  );

  function toggleField(field: string) {
    const next = new Set(picked);
    if (next.has(field)) next.delete(field);
    else next.add(field);
    setPicked(next);
  }

  async function writeBack() {
    setWriting(true);
    setError(null);
    try {
      await api.catalogApplyToTrack(trackId, [...picked], writeCover);
      onWritten();
    } catch (e) {
      setError(String(e));
    } finally {
      setWriting(false);
    }
  }

  /** 单独刮取封面（多源候选），不改音频文件 */
  function openCoverPicker() {
    setCoverOpen(true);
  }

  return (
    <aside className="cmp-panel" role="complementary" aria-label="文件与 catalog 对比">
      <header className="cmp-head">
        <h2>文件 vs 本地 catalog</h2>
        <button
          className="btn"
          onClick={onSearchLyrics}
          title="在线搜索歌词（LRCLIB / 网易云 / QQ音乐）"
        >
          <Search size={14} /> 搜索歌词
        </button>
      </header>
      {error && <div className="error-line cmp-error">{error}</div>}
      {!data && !error && <div className="tertiary cmp-empty">加载中…</div>}
      {data && !data.catalog && (
        <div className="empty-state">
          <p className="muted">
            尚未关联 catalog 记录。可「尝试自动匹配」按字段关联，或到「刮削」拉取云端数据存入
            catalog。
          </p>
          <button
            className="btn"
            onClick={() =>
              void api
                .catalogMatchOne(trackId)
                .then(() => onWritten())
                .catch((e) => setError(String(e)))
            }
          >
            尝试自动匹配
          </button>
        </div>
      )}
      {data && data.catalog && (
        <>
          <div className="cmp-meta">
            <div>
              <div className="muted">文件</div>
              <div>{data.track.path}</div>
            </div>
            <div>
              <div className="muted">catalog</div>
              <div>
                {data.catalog.artist} — {data.catalog.title}
              </div>
            </div>
          </div>
          <div className="cmp-covers">
            <figure className="cmp-cover">
              <figcaption className="muted">文件封面</figcaption>
              {fileCover ? (
                <img src={fileCover} alt="文件封面" />
              ) : (
                <div className="cmp-cover-empty tertiary">无</div>
              )}
            </figure>
            <figure className="cmp-cover">
              <figcaption className="muted">catalog 封面</figcaption>
              {catalogCover ? (
                <img src={catalogCover} alt="catalog 封面" />
              ) : (
                <div className="cmp-cover-empty tertiary">
                  {fileCover ? "未刮取" : "无封面，可刮取"}
                </div>
              )}
            </figure>
            <button
              className="btn"
              title="多源搜索封面（CAA / iTunes / 网易云 / QQ音乐），点选一张采纳到库 covers/"
              onClick={openCoverPicker}
            >
              <ImagePlus size={14} />
              {catalogCover ? "重新刮取封面" : "刮取封面"}
            </button>
          </div>
          <table className="cmp-table">
            <thead>
              <tr>
                <th style={{ width: 28 }} />
                <th>字段</th>
                <th>文件当前</th>
                <th />
                <th>catalog</th>
              </tr>
            </thead>
            <tbody>
              {data.changes.map((ch, i) => {
                const changed = ch.old.trim() !== ch.new.trim();
                const writable = ch.new.trim() !== "";
                const selectable = changed && writable;
                return (
                  <tr key={i} className={changed ? "changed" : ""}>
                    <td onClick={(e) => e.stopPropagation()}>
                      <input
                        type="checkbox"
                        aria-label={`写入 ${FIELD_LABEL[ch.field] ?? ch.field}`}
                        disabled={!selectable}
                        checked={selectable && picked.has(ch.field)}
                        onChange={() => toggleField(ch.field)}
                        title={selectable ? "勾选后写入文件" : "无变更或 catalog 无值"}
                      />
                    </td>
                    <td>{FIELD_LABEL[ch.field] ?? ch.field}</td>
                    <td className="old">{ch.old || "—"}</td>
                    <td className="arrow">→</td>
                    <td className="new">{ch.new || "—"}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          <footer className="cmp-foot">
            <label className="cmp-check">
              <input
                type="checkbox"
                checked={writeCover}
                onChange={(e) => setWriteCover(e.target.checked)}
              />
              同时写入封面（若 catalog 有缓存）
            </label>
            <div>
              <button
                className="btn btn-primary"
                disabled={writing || picked.size === 0}
                onClick={() => void writeBack()}
              >
                {writing ? "写入中…" : `写入文件（已选 ${picked.size} 项）`}
              </button>
            </div>
          </footer>
          {real.length > 0 && picked.size < real.length && (
            <div className="tertiary cmp-hint">
              未勾选的差异不会写入；catalog 空值字段永不覆盖文件标签。
            </div>
          )}
        </>
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
