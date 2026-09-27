import { useEffect, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  Database,
  Disc3,
  FileText,
  Image,
  ListMusic,
  Loader2,
  X,
} from "lucide-react";

import { api } from "../../lib/api";
import type {
  ImportItemStats,
  ImportPreview,
  ImportResult,
  ImportSelection,
} from "../../lib/types";
import "./ImportLibraryPanel.css";

type ContentKey = keyof ImportSelection;

interface ContentMeta {
  key: ContentKey;
  title: string;
  hint: string;
  icon: typeof Database;
  primary?: boolean;
  unit: string;
}

const CONTENT_METAS: ContentMeta[] = [
  {
    key: "catalog",
    title: "刮削数据库",
    hint: "已采纳的在线元数据（catalog）。导入后自动关联当前库未刮削曲目，不改音频文件。",
    icon: Database,
    primary: true,
    unit: "条",
  },
  {
    key: "songs",
    title: "歌曲",
    hint: "复制当前库没有的音频文件（保持源库相对路径），并写入曲目记录。",
    icon: Disc3,
    unit: "首",
  },
  {
    key: "lyrics",
    title: "歌词",
    hint: "复制 lrc/ 下的外挂歌词；同名文件跳过。",
    icon: FileText,
    unit: "个",
  },
  {
    key: "covers",
    title: "封面",
    hint: "复制 covers/ 下的封面图；同名文件跳过。与数据库一起导入时会更新封面引用。",
    icon: Image,
    unit: "张",
  },
  {
    key: "playlists",
    title: "歌单",
    hint: "复制 playlists/ 下的 .m3u8；同名歌单跳过，不会覆盖。",
    icon: ListMusic,
    unit: "个",
  },
];

function fmt(n: number): string {
  return n.toLocaleString("zh-CN");
}

function ItemCompare({ stats, unit }: { stats: ImportItemStats; unit: string }) {
  return (
    <div className="imp-compare">
      <span>
        源 <b className="mono">{fmt(stats.source_total)}</b> {unit}
      </span>
      <span className="imp-dot">·</span>
      <span className="imp-dup">
        重复 <b className="mono">{fmt(stats.duplicate)}</b>
      </span>
      <span className="imp-dot">·</span>
      <span className="imp-new">
        将新增 <b className="mono">{fmt(stats.new)}</b>
      </span>
      {stats.missing > 0 && (
        <>
          <span className="imp-dot">·</span>
          <span className="imp-miss">
            缺失 <b className="mono">{fmt(stats.missing)}</b>
          </span>
        </>
      )}
    </div>
  );
}

/**
 * 导入其他库面板：识别结果 → 勾选内容（默认全不选）→ 对比数字 → 执行。
 */
export function ImportLibraryPanel({
  preview,
  onClose,
  onImported,
}: {
  preview: ImportPreview;
  onClose: () => void;
  onImported: (result: ImportResult) => void;
}) {
  const [sel, setSel] = useState<ImportSelection>({
    catalog: false,
    songs: false,
    lyrics: false,
    covers: false,
    playlists: false,
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Esc 关闭（busy 时不响应）
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && !busy) onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  const items = CONTENT_METAS;
  const selectedCount = items.filter((it) => sel[it.key]).length;
  const pendingNew = items
    .filter((it) => sel[it.key])
    .reduce((sum, it) => sum + (preview[it.key]?.new ?? 0), 0);

  function toggle(key: ContentKey) {
    setSel((s) => ({ ...s, [key]: !s[key] }));
  }

  async function doImport() {
    if (busy || selectedCount === 0) return;
    setBusy(true);
    setError(null);
    try {
      const r = await api.libraryImportRun(preview.source_root, sel);
      onImported(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className="imp-overlay"
      role="dialog"
      aria-modal="true"
      aria-label="导入其他库"
      onClick={busy ? undefined : onClose}
    >
      <div className="imp-panel" onClick={(e) => e.stopPropagation()}>
        <header className="imp-head">
          <h2>导入其他库</h2>
          <button className="imp-close" onClick={onClose} disabled={busy} aria-label="关闭">
            <X size={16} />
          </button>
        </header>

        <div className="imp-roots">
          <div className="imp-root-row">
            <span className="imp-root-label">即将导入</span>
            <span className="imp-root-path mono" title={preview.source_root}>
              {preview.source_root}
            </span>
          </div>
          <div className="imp-root-row">
            <span className="imp-root-label">导入到</span>
            <span className="imp-root-path mono" title={preview.current_root}>
              {preview.current_root}
            </span>
          </div>
          <div className="imp-recognized">
            <CheckCircle2 size={13} />
            识别成功 · AxMusic 库
          </div>
        </div>

        <div className="imp-body">
          <div className="imp-section-title">
            与当前库对比 · 勾选要导入的内容
            <span className="tertiary">（默认不选；重复项会跳过）</span>
          </div>

          <ul className="imp-list">
            {items.map((it) => {
              const Icon = it.icon;
              const on = sel[it.key];
              const stats = preview[it.key];
              const empty = !stats || stats.source_total === 0;
              return (
                <li
                  key={it.key}
                  className={`imp-item${on ? " on" : ""}${it.primary ? " primary" : ""}${empty ? " empty" : ""}`}
                >
                  <label className="imp-item-label">
                    <input
                      type="checkbox"
                      checked={on}
                      disabled={busy || empty}
                      onChange={() => toggle(it.key)}
                    />
                    <span className="imp-item-main">
                      <span className="imp-item-title">
                        <Icon size={15} className={it.primary ? "imp-icon-accent" : undefined} />
                        {it.title}
                        {it.primary && <span className="imp-badge">最重要</span>}
                        {empty && <span className="imp-badge muted">源库为空</span>}
                      </span>
                      <span className="imp-item-hint">{it.hint}</span>
                      {!empty && stats && <ItemCompare stats={stats} unit={it.unit} />}
                      {empty && (
                        <span className="tertiary imp-item-hint">
                          源库没有可导入的{it.title}
                        </span>
                      )}
                    </span>
                  </label>
                </li>
              );
            })}
          </ul>
        </div>

        <footer className="imp-foot">
          <div className="imp-summary">
            {selectedCount === 0 ? (
              <span className="tertiary">未选择任何内容</span>
            ) : (
              <>
                已选 <b>{selectedCount}</b> 项 · 预计新增约{" "}
                <b className="mono imp-new">{fmt(pendingNew)}</b> 项
              </>
            )}
            {error && (
              <div className="imp-error">
                <AlertTriangle size={13} /> {error}
              </div>
            )}
          </div>
          <div className="imp-actions">
            <button className="btn imp-btn" onClick={onClose} disabled={busy}>
              取消
            </button>
            <button
              className="btn btn-primary imp-btn"
              disabled={busy || selectedCount === 0}
              onClick={() => void doImport()}
            >
              {busy ? (
                <>
                  <Loader2 size={14} className="spin" /> 导入中…
                </>
              ) : (
                "开始导入"
              )}
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}

/** 导入完成后的结果摘要条 */
export function ImportResultBanner({
  result,
  onClose,
}: {
  result: ImportResult;
  onClose: () => void;
}) {
  const rows: { label: string; added: number; skipped: number; failed?: number }[] = [
    { label: "数据库", added: result.catalog_added, skipped: result.catalog_skipped },
    {
      label: "歌曲",
      added: result.songs_added,
      skipped: result.songs_skipped,
      failed: result.songs_failed,
    },
    { label: "歌词", added: result.lyrics_added, skipped: result.lyrics_skipped },
    { label: "封面", added: result.covers_added, skipped: result.covers_skipped },
    { label: "歌单", added: result.playlists_added, skipped: result.playlists_skipped },
  ];
  const parts = rows
    .filter((r) => r.added || r.skipped || r.failed)
    .map(
      (r) =>
        `${r.label} +${r.added}（跳过 ${r.skipped}${r.failed ? `，失败 ${r.failed}` : ""}）`,
    );
  if (result.tracks_linked > 0) {
    parts.push(`关联曲目 ${result.tracks_linked}`);
  }
  return (
    <div className="imp-banner">
      <CheckCircle2 size={14} />
      <span className="imp-banner-text">导入完成：{parts.join(" · ") || "无变更"}</span>
      {result.errors.length > 0 && (
        <span className="imp-banner-err" title={result.errors.join("\n")}>
          {result.errors.length} 条错误
        </span>
      )}
      <button className="imp-banner-close" onClick={onClose} aria-label="关闭">
        <X size={14} />
      </button>
    </div>
  );
}
