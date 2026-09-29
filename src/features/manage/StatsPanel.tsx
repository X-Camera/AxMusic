import { useCallback, useEffect, useRef, useState } from "react";
import { notNil } from "../../lib/nil";
import { friendlyErr } from "../../lib/errors";
import { FolderCheck, FolderInput, Loader2, FileAudio, FileText, File, Folder } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../../lib/api";
import type { ImportPreview, ImportResult, LibraryRootScan, LibraryStats } from "../../lib/types";
import { ImportLibraryPanel, ImportResultBanner } from "./ImportLibraryPanel";
import "./StatsPanel.css";

const KIND_ICON: Record<string, typeof Folder> = {
  audio: FileAudio,
  lyrics: FileText,
  other_file: File,
  other_dir: Folder,
};

const KIND_LABEL: Record<string, string> = {
  audio: "音频",
  lyrics: "歌词",
  other_file: "文件",
  other_dir: "文件夹",
};

/** 右栏常驻面板：未选中曲目时显示库统计（选中后切换为 文件 vs catalog 对比）。 */
export function StatsPanel({
  stats,
  onImported,
}: {
  stats: LibraryStats | null;
  /** 导入完成后刷新列表/统计 */
  onImported?: () => void;
}) {
  const [rootScan, setRootScan] = useState<LibraryRootScan | null>(null);
  const [organizing, setOrganizing] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [importPreview, setImportPreview] = useState<ImportPreview | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [importResult, setImportResult] = useState<ImportResult | null>(null);
  const [picking, setPicking] = useState(false);
  const aliveRef = useRef(true);

  const pct =
    stats && stats.total_tracks > 0
      ? Math.round((stats.linked_tracks / stats.total_tracks) * 100)
      : 0;

  const reloadScan = useCallback(async () => {
    try {
      const s = await api.libraryRootScan();
      if (aliveRef.current) {
        setRootScan(s);
        setScanError(null);
      }
    } catch (e) {
      if (aliveRef.current) setScanError(friendlyErr(e));
    }
  }, []);

  useEffect(() => {
    void reloadScan();
    return () => {
      aliveRef.current = false;
    };
  }, [reloadScan]);

  const organize = useCallback(async () => {
    setOrganizing(true);
    setScanError(null);
    try {
      const r = await api.libraryRootOrganize();
      await reloadScan();
      if (r.failed.length > 0 && aliveRef.current) {
        const shown = r.failed.slice(0, 5).join("、");
        const more = r.failed.length > 5 ? " 等" : "";
        setScanError(
          `已整理 ${r.moved.length} 项；${r.failed.length} 项未移动：${shown}${more}`,
        );
      }
    } catch (e) {
      setScanError(friendlyErr(e));
    } finally {
      setOrganizing(false);
    }
  }, [reloadScan]);

  const pickImportLibrary = useCallback(async () => {
    setPicking(true);
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "选择要导入的 AxMusic 库目录",
      });
      // 取消选择：保留上次结果横幅/错误，不清空
      if (!picked || Array.isArray(picked)) return;
      const preview = await api.libraryImportPreview(picked);
      if (!aliveRef.current) return;
      setImportError(null);
      setImportResult(null);
      setImportPreview(preview);
    } catch (e) {
      if (aliveRef.current) setImportError(friendlyErr(e));
    } finally {
      if (aliveRef.current) setPicking(false);
    }
  }, []);

  return (
    <aside className="stats-panel" aria-label="库统计">
      <header className="stats-head">
        <h2>库统计</h2>
      </header>
      {!stats ? (
        <div className="tertiary">加载中…</div>
      ) : (
        <>
          {importResult && (
            <ImportResultBanner
              result={importResult}
              onClose={() => setImportResult(null)}
            />
          )}

          <section className="stats-block">
            <div className="stats-block-title">刮削进度</div>
            <div className="stats-big">
              <span className="mono stats-accent">{stats.linked_tracks}</span>
              <span className="tertiary"> / {stats.total_tracks} 首已关联</span>
            </div>
            <div className="stats-bar">
              <div className="stats-bar-fill" style={{ width: `${pct}%` }} />
            </div>
            <div className="tertiary stats-note">表中绿色字段 = 与 catalog 一致</div>
          </section>

          <section className="stats-block">
            <div className="stats-block-title">本地 catalog</div>
            <div className="stats-grid">
              <div className="stat-card">
                <div className="stat-num mono">{stats.catalog_albums}</div>
                <div className="stat-label">专辑</div>
              </div>
              <div className="stat-card">
                <div className="stat-num mono">{stats.catalog_artists}</div>
                <div className="stat-label">歌手</div>
              </div>
              <div className="stat-card">
                <div className="stat-num mono">{stats.catalog_tracks}</div>
                <div className="stat-label">曲目</div>
              </div>
            </div>
          </section>

          <section className="stats-block">
            <div className="stats-block-title">文件标签</div>
            <ul className="stats-list">
              <li>
                <span>库中曲目</span>
                <span className="mono">{stats.total_tracks}</span>
              </li>
              <li>
                <span>有封面</span>
                <span className="mono">{stats.with_cover}</span>
              </li>
              <li>
                <span>外挂歌词</span>
                <span className="mono">{stats.with_lrc}</span>
              </li>
              <li>
                <span>内嵌歌词</span>
                <span className="mono">{stats.with_lyrics}</span>
              </li>
            </ul>
          </section>

          <section className="stats-block">
            <div className="stats-block-title">库迁移</div>
            {importError && <div className="error-line">{importError}</div>}
            <div className="tertiary stats-note">
              从另一个 AxMusic 库导入刮削数据库、歌曲、歌词、封面、歌单、听歌记录
            </div>
            <button
              className="btn"
              disabled={picking}
              title="选择其他 AxMusic 库目录，对比后选择性导入"
              onClick={() => void pickImportLibrary()}
            >
              {picking ? (
                <Loader2 size={14} className="spin" />
              ) : (
                <FolderInput size={14} />
              )}
              导入其他库
            </button>
          </section>

          <section className="stats-block">
            <div className="stats-block-title">库文件扫描</div>
            {scanError && <div className="error-line">{scanError}</div>}
            {!rootScan && <div className="tertiary">扫描中…</div>}
            {rootScan?.ok && <div className="tertiary stats-note">库根目录整洁</div>}
            {notNil(rootScan) && !rootScan.ok && (
              <>
                <div className="stats-big">
                  <span className="mono stats-accent">{rootScan.items.length}</span>
                  <span className="tertiary"> 项待整理（挪入 Unarchived）</span>
                </div>
                <ul className="stats-stray">
                  {rootScan.items.map((it) => {
                    const Icon = KIND_ICON[it.kind] ?? File;
                    return (
                      <li key={it.path} title={it.path}>
                        <Icon size={12} />
                        <span className="stats-stray-name">{it.name}</span>
                        <span className="tertiary">{KIND_LABEL[it.kind] ?? it.kind}</span>
                      </li>
                    );
                  })}
                </ul>
                <button
                  className="btn btn-primary"
                  disabled={organizing}
                  title="把库根目录里不属于白名单的文件/文件夹全部挪进 Unarchived/"
                  onClick={() => void organize()}
                >
                  {organizing ? <Loader2 size={14} className="spin" /> : <FolderCheck size={14} />}
                  整理
                </button>
              </>
            )}
            <div className="tertiary stats-note">
              保留 archived / Unarchived / lrc / covers / playlists / axmusic.db
            </div>
          </section>
        </>
      )}

      {importPreview && (
        <ImportLibraryPanel
          preview={importPreview}
          onClose={() => setImportPreview(null)}
          onImported={(r) => {
            setImportPreview(null);
            setImportResult(r);
            void reloadScan();
            onImported?.();
          }}
        />
      )}
    </aside>
  );
}
