import type { TrackRow } from "../../lib/types";
import "./TrackTable.css";

function StatusDot({ ok, label }: { ok: boolean; label: string }) {
  return (
    <span
      className={`status-dot${ok ? " ok" : " miss"}`}
      title={ok ? `${label} ✓` : `${label} 缺`}
    >
      {ok ? "✓" : "缺"}
    </span>
  );
}

export function TrackTable({
  rows,
  selected,
  onSelectedChange,
  onPlay,
  onInclude,
}: {
  rows: TrackRow[];
  selected: Set<number>;
  onSelectedChange: (s: Set<number>) => void;
  onPlay: (row: TrackRow, indexInView: number) => void;
  onInclude: (row: TrackRow) => void;
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
          <th style={{ width: 28 }} />
          <th>曲名</th>
          <th>歌手</th>
          <th>专辑</th>
          <th style={{ width: 56 }}>年</th>
          <th style={{ width: 44 }}>#</th>
          <th style={{ width: 40 }}>封</th>
          <th style={{ width: 40 }}>词</th>
          <th style={{ width: 40 }}>年</th>
          <th style={{ width: 40 }}>型</th>
          <th style={{ width: 40 }}>MB</th>
          <th style={{ width: 88 }} />
        </tr>
      </thead>
      <tbody>
        {rows.map((t, idx) => {
          const status = t.tag_status;
          const bar =
            status === "complete" ? "ok" : status === "partial" ? "warn" : "bad";
          const inLib = t.path.replace(/\\/g, "/").includes("/"); // always true; real check via include shortcut
          void inLib;
          return (
            <tr
              key={t.id}
              className={`row bar-${bar}${selected.has(t.id) ? " selected" : ""}`}
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
              <td className="bar-cell">
                <span className={`row-bar ${bar}`} />
              </td>
              <td className="ellipsis" title={t.title || t.filename}>
                {t.title || t.filename}
              </td>
              <td className="ellipsis muted" title={t.artist}>
                {t.artist || "—"}
              </td>
              <td className="ellipsis muted" title={t.album}>
                {t.album || "—"}
              </td>
              <td className="mono">{t.year || "—"}</td>
              <td className="mono">{t.track_no ?? "—"}</td>
              <td>
                <StatusDot ok={t.has_cover} label="封面" />
              </td>
              <td>
                <StatusDot ok={t.has_lyrics} label="歌词" />
              </td>
              <td>
                <StatusDot ok={t.has_year} label="年份" />
              </td>
              <td>
                <StatusDot ok={!!t.album} label="类型/专辑" />
              </td>
              <td>
                <StatusDot ok={t.has_mb_id} label="MusicBrainz" />
              </td>
              <td className="row-actions" onClick={(e) => e.stopPropagation()}>
                <button className="link-btn" onClick={() => onPlay(t, idx)} title="播放">
                  播放
                </button>
                <button className="link-btn" onClick={() => onInclude(t)} title="纳入库管理（复制进库）">
                  入库
                </button>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
