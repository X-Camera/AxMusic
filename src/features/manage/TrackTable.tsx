import { Music } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import type { TrackRow } from "../../lib/types";
import { loadCover, observeCover, peekCover, unobserveCover } from "./coverCache";
import "./TrackTable.css";

/* ── 与 catalog 的匹配判定：trim + 忽略大小写；年取前 4 位数字特化；轨号按数值 ── */
const norm = (s: string | null | undefined) => (s ?? "").trim().toLowerCase();

function matchStr(file: string, catalog: string | null): boolean {
  const c = norm(catalog);
  return c !== "" && norm(file) === c;
}

function matchYear(file: string, catalog: string | null): boolean {
  const f = norm(file);
  const c = norm(catalog);
  if (f === "" || c === "") return false;
  const fy = f.match(/\d{4}/);
  const cy = c.match(/\d{4}/);
  if (fy && cy) return fy[0] === cy[0];
  return f === c;
}

function matchNo(file: number | null, catalog: number | null): boolean {
  return file != null && catalog != null && file === catalog;
}

/** 内嵌封面缩略图：进入视口才提取（懒加载），无封面显示占位图标且不发起加载。 */
function CoverThumb({
  path,
  mtime,
  hasCover,
}: {
  path: string;
  mtime: number;
  hasCover: boolean;
}) {
  const key = `${path}:${mtime}`;
  const [src, setSrc] = useState<string | null>(() => peekCover(key) ?? null);
  const boxRef = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (!hasCover) return;
    const cached = peekCover(key);
    if (cached !== undefined) {
      setSrc(cached);
      return;
    }
    const el = boxRef.current;
    if (!el) return;
    let alive = true;
    observeCover(el, () => {
      void loadCover(key, path).then((url) => {
        if (alive) setSrc(url);
      });
    });
    return () => {
      alive = false;
      unobserveCover(el);
    };
  }, [key, path, hasCover]);

  return (
    <span ref={boxRef} className="cover-cell">
      {src ? <img src={src} alt="" /> : <Music size={14} />}
    </span>
  );
}

export function TrackTable({
  rows,
  selected,
  activeId,
  onSelectedChange,
  onPlay,
  onActivate,
  onScrape,
}: {
  rows: TrackRow[];
  selected: Set<number>;
  activeId: number | null;
  onSelectedChange: (s: Set<number>) => void;
  onPlay: (row: TrackRow, indexInView: number) => void;
  /** 单击行 → 右侧显示 文件 vs catalog 对比（再次单击已激活行 → 回到统计） */
  onActivate: (row: TrackRow) => void;
  onScrape: (row: TrackRow) => void;
}) {
  function toggle(id: number) {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    onSelectedChange(next);
  }

  function toggleAll() {
    if (selected.size === rows.length) onSelectedChange(new Set());
    else onSelectedChange(new Set(rows.map((r) => r.id)));
  }

  return (
    <table className="track-table">
      <thead>
        <tr>
          <th style={{ width: 36 }}>
            <input
              type="checkbox"
              aria-label="全选"
              checked={rows.length > 0 && selected.size === rows.length}
              onChange={toggleAll}
            />
          </th>
          <th style={{ width: 48 }} title="封面（文件内嵌）">
            封
          </th>
          <th>曲名</th>
          <th>歌手</th>
          <th>专辑</th>
          <th style={{ width: 56 }}>年</th>
          <th style={{ width: 44 }}>#</th>
          <th style={{ width: 44 }} title="内嵌歌词（标签内）">
            内嵌
          </th>
          <th style={{ width: 44 }} title="外挂歌词（同目录同名 .lrc）">
            外挂
          </th>
          <th style={{ width: 72 }} />
        </tr>
      </thead>
      <tbody>
        {rows.map((t, idx) => {
          const mTitle = matchStr(t.title, t.catalog_title);
          const mArtist = matchStr(t.artist, t.catalog_artist);
          const mAlbum = matchStr(t.album, t.catalog_album);
          const mYear = matchYear(t.year, t.catalog_year);
          const mNo = matchNo(t.track_no, t.catalog_track_no);
          return (
            <tr
              key={t.id}
              className={`row${activeId === t.id ? " active" : ""}${selected.has(t.id) ? " selected" : ""}`}
              onClick={() => onActivate(t)}
              onDoubleClick={() => onPlay(t, idx)}
            >
              <td onClick={(e) => e.stopPropagation()}>
                <input
                  type="checkbox"
                  aria-label={`选择 ${t.title}`}
                  checked={selected.has(t.id)}
                  onChange={() => toggle(t.id)}
                />
              </td>
              <td>
                <CoverThumb path={t.path} mtime={t.mtime} hasCover={t.has_cover} />
              </td>
              <td
                className={`ellipsis${mTitle ? " cell-match" : ""}`}
                title={t.title || t.filename}
              >
                {t.title || t.filename}
              </td>
              <td
                className={`ellipsis${mArtist ? " cell-match" : ""}${t.artist ? "" : " cell-empty"}`}
                title={t.artist}
              >
                {t.artist || "—"}
              </td>
              <td
                className={`ellipsis${mAlbum ? " cell-match" : ""}${t.album ? "" : " cell-empty"}`}
                title={t.album}
              >
                {t.album || "—"}
              </td>
              <td
                className={`mono${mYear ? " cell-match" : ""}${t.year ? "" : " cell-empty"}`}
              >
                {t.year || "—"}
              </td>
              <td
                className={`mono${mNo ? " cell-match" : ""}${t.track_no != null ? "" : " cell-empty"}`}
              >
                {t.track_no ?? "—"}
              </td>
              <td className={t.has_lyrics ? "cell-ok" : "cell-empty"}>
                {t.has_lyrics ? "✓" : "—"}
              </td>
              <td className={t.has_lrc ? "cell-ok" : "cell-empty"}>
                {t.has_lrc ? "✓" : "—"}
              </td>
              <td className="row-actions" onClick={(e) => e.stopPropagation()}>
                <button className="link-btn" onClick={() => onPlay(t, idx)} title="播放">
                  播放
                </button>
                <button className="link-btn" onClick={() => onScrape(t)} title="刮削此曲到 catalog">
                  刮削
                </button>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
