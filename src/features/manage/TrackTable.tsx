import { Music } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from "react";

import type { ArchiveStatus, TrackRow } from "../../lib/types";
import { trackRowToAddItem } from "../../lib/api";
import { FavoriteHeart } from "../../components/FavoriteHeart";
import { TABLE_ROW_HEIGHT, useTableVirtualizer } from "../../components/VirtualList";
import { loadCover, observeCover, peekCover, unobserveCover } from "./coverCache";
import "./TrackTable.css";

/** 与 thead 的 th 数量一致；spacer 行 colSpan 用 */
const COL_COUNT = 11;

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

/** 无损容器扩展名（格式列标签用 success 描边区分） */
const LOSSLESS_EXT = new Set(["flac", "wav", "alac", "aiff", "aif", "wv", "ape", "tta"]);
const isLossless = (format: string) => LOSSLESS_EXT.has(format.toLowerCase());

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
  archiveMap,
  scrollRef,
  onSelectedChange,
  onPlay,
  onActivate,
}: {
  rows: TrackRow[];
  selected: Set<number>;
  activeId: number | null;
  /** 归档状态 map: track_id → ArchiveStatus */
  archiveMap: Record<number, ArchiveStatus> | null;
  /** 滚动容器（.page-scroll），虚拟化据此算可视窗口 */
  scrollRef: RefObject<HTMLElement | null>;
  onSelectedChange: (s: Set<number>) => void;
  onPlay: (row: TrackRow, indexInView: number) => void;
  /** 单击行 → 右侧显示 catalog 字段（再次单击已激活行 → 回到统计） */
  onActivate: (row: TrackRow) => void;
}) {
  const tableRef = useRef<HTMLTableElement>(null);
  const [scrollMargin, setScrollMargin] = useState(0);

  // tbody 起点相对滚动容器的偏移（表头 + 表格边框），供 virtual-core 对齐 scrollTop
  useLayoutEffect(() => {
    const measure = () => {
      const table = tableRef.current;
      const scroller = scrollRef.current;
      if (!table || !scroller) return;
      const thead = table.tHead;
      const top =
        table.getBoundingClientRect().top -
        scroller.getBoundingClientRect().top +
        scroller.scrollTop +
        (thead?.offsetHeight ?? 0);
      setScrollMargin((p) => (p === top ? p : Math.max(0, top)));
    };
    measure();
    const scroller = scrollRef.current;
    if (!scroller) return;
    const ro = new ResizeObserver(measure);
    ro.observe(scroller);
    return () => ro.disconnect();
  }, [scrollRef]);

  const { vis, paddingTop, paddingBottom } = useTableVirtualizer({
    count: rows.length,
    rowHeight: TABLE_ROW_HEIGHT,
    scrollRef,
    scrollMargin,
    overscan: 15,
  });

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
    <table className="track-table" ref={tableRef}>
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
          <th style={{ width: 56 }} title="封面（文件内嵌）">
            封面
          </th>
          <th style={{ width: 40 }} title="喜爱">
            ♥
          </th>
          <th className="cell-left">曲名</th>
          <th className="cell-left">歌手</th>
          <th>专辑</th>
          <th style={{ width: 56 }}>年份</th>
          <th style={{ width: 48 }}>轨号</th>
          <th style={{ width: 56 }} title="文件格式">
            格式
          </th>
          <th style={{ width: 56 }} title="歌词：嵌=标签内，挂=外挂 .lrc（绿=有）">
            歌词
          </th>
          <th style={{ width: 56 }} title="归档状态（已关联 catalog 才检查；未关联显示 —）">
            归档
          </th>
        </tr>
      </thead>
      <tbody>
        {paddingTop > 0 && (
          <tr aria-hidden style={{ height: paddingTop }}>
            <td colSpan={COL_COUNT} style={{ padding: 0, border: "none", height: paddingTop }} />
          </tr>
        )}
        {vis.map((vi) => {
          const t = rows[vi.index];
          if (!t) return null;
          const idx = vi.index;
          const mTitle = matchStr(t.title, t.catalog_title);
          const mArtist = matchStr(t.artist, t.catalog_artist);
          const mAlbum = matchStr(t.album, t.catalog_album);
          const mYear = matchYear(t.year, t.catalog_year);
          const mNo = matchNo(t.track_no, t.catalog_track_no);
          return (
            <tr
              key={t.id}
              style={{ height: TABLE_ROW_HEIGHT }}
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
              <td onClick={(e) => e.stopPropagation()}>
                <FavoriteHeart item={trackRowToAddItem(t)} />
              </td>
              <td
                className={`ellipsis cell-left${mTitle ? " cell-match" : ""}`}
                title={t.title || t.filename}
              >
                {t.title || t.filename}
              </td>
              <td
                className={`ellipsis cell-left${mArtist ? " cell-match" : ""}${t.artist ? "" : " cell-empty"}`}
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
              <td title={t.format ? `格式 ${t.format.toUpperCase()}` : undefined}>
                {t.format ? (
                  <span className={`fmt-tag${isLossless(t.format) ? " lossless" : ""}`}>
                    {t.format.toUpperCase()}
                  </span>
                ) : (
                  <span className="cell-empty">—</span>
                )}
              </td>
              <td
                title={`内嵌歌词：${t.has_lyrics ? "有" : "无"}；外挂歌词：${t.has_lrc ? "有" : "无"}`}
              >
                <span
                  className={`lyr-mini${t.has_lyrics ? " ok" : ""}`}
                  title="内嵌歌词（标签内）"
                >
                  嵌
                </span>
                <span className={`lyr-mini${t.has_lrc ? " ok" : ""}`} title="外挂歌词（.lrc）">
                  挂
                </span>
              </td>
              <td>
                {t.catalog_id == null ? (
                  <span className="cell-empty" title="先要刮削，关联后才检查归档">
                    —
                  </span>
                ) : archiveMap && archiveMap[t.id] != null ? (
                  archiveMap[t.id].ok ? (
                    <span className="cell-ok" title="归档规范">✓</span>
                  ) : (
                    <span
                      className="cell-warn"
                      title={archiveMap[t.id].issues.map((i) => i.message).join("\n")}
                    >
                      ?
                    </span>
                  )
                ) : (
                  <span className="cell-empty">…</span>
                )}
              </td>
            </tr>
          );
        })}
        {paddingBottom > 0 && (
          <tr aria-hidden style={{ height: paddingBottom }}>
            <td colSpan={COL_COUNT} style={{ padding: 0, border: "none", height: paddingBottom }} />
          </tr>
        )}
      </tbody>
    </table>
  );
}
