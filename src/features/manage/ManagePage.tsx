import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useMemo, useState } from "react";
import { FolderPlus, FolderSearch, ListPlus, ListRestart, Loader2, Play, Search } from "lucide-react";

import { api, trackRowToAddItem, trackRowToQueueItem } from "../../lib/api";
import type { LibraryRoot, LibraryStats, PlaylistAddItem, ScanProgress, ScanResult, TrackRow } from "../../lib/types";
import { useApp } from "../../state/useApp";
import { TopBar } from "../../components/TopBar";
import { TrackTable } from "./TrackTable";
import { ComparePanel } from "./ComparePanel";
import { StatsPanel } from "./StatsPanel";
import { ScrapeWizard } from "./ScrapeWizard";
import { LyricsPanel } from "./LyricsPanel";
import { PlaylistPicker } from "../playlists/PlaylistPicker";
import "./ManagePage.css";

export function ManagePage() {
  const playQueue = useApp((s) => s.playQueue);
  const [root, setRoot] = useState<LibraryRoot | null | undefined>(undefined);
  const [tracks, setTracks] = useState<TrackRow[]>([]);
  const [missingOnly, setMissingOnly] = useState(false);
  const [unlinkedOnly, setUnlinkedOnly] = useState(false);
  const [query, setQuery] = useState("");
  const [scanning, setScanning] = useState(false);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [lastScan, setLastScan] = useState<ScanResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [toast, setToast] = useState<string | null>(null);
  const [scrapeTrack, setScrapeTrack] = useState<TrackRow | null>(null);
  const [pickerItems, setPickerItems] = useState<PlaylistAddItem[] | null>(null);
  const [lyricsTrackId, setLyricsTrackId] = useState<number | null>(null);
  const [compareId, setCompareId] = useState<number | null>(null);
  const [compareVersion, setCompareVersion] = useState(0);
  const [stats, setStats] = useState<LibraryStats | null>(null);

  const reloadRoot = useCallback(async () => {
    try {
      setRoot(await api.getLibraryRoot());
    } catch {
      setRoot(null);
    }
  }, []);

  const reloadTracks = useCallback(async () => {
    try {
      // 统计与列表同刷：扫描/写回/刮削/补歌词后都会走到这里
      const [list, st] = await Promise.all([
        api.getTracks({
          missing_only: missingOnly,
          unlinked_only: unlinkedOnly,
          limit: 2000,
        }),
        api.getLibraryStats(),
      ]);
      setTracks(list);
      setStats(st);
    } catch {
      setTracks([]);
    }
  }, [missingOnly, unlinkedOnly]);

  useEffect(() => {
    void reloadRoot();
  }, [reloadRoot]);

  useEffect(() => {
    if (root) void reloadTracks();
  }, [root, reloadTracks]);

  useEffect(() => {
    // listen scan progress via polling refresh_scan side channel — events need @tauri-apps/api/event
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        const u1 = await listen<ScanProgress>("scan://progress", (e) => {
          if (!cancelled) setProgress(e.payload);
        });
        const u2 = await listen<ScanResult>("scan://done", () => {
          if (!cancelled) void reloadTracks();
        });
        unlisten = () => {
          u1();
          u2();
        };
      } catch {
        /* events optional */
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [reloadTracks]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return tracks;
    return tracks.filter(
      (t) =>
        t.title.toLowerCase().includes(q) ||
        t.artist.toLowerCase().includes(q) ||
        t.album.toLowerCase().includes(q) ||
        t.filename.toLowerCase().includes(q) ||
        t.path.toLowerCase().includes(q),
    );
  }, [tracks, query]);

  /** 边栏底部操作对象：优先多选，否则当前激活行 */
  const actionTracks = useMemo(() => {
    if (selected.size > 0) return filtered.filter((t) => selected.has(t.id));
    if (compareId != null) {
      const row = filtered.find((t) => t.id === compareId);
      return row ? [row] : [];
    }
    return [];
  }, [filtered, selected, compareId]);

  /** 当前激活的单曲（边栏刮削按钮） */
  const activeTrack = useMemo(
    () => (compareId != null ? (filtered.find((t) => t.id === compareId) ?? null) : null),
    [filtered, compareId],
  );

  async function onInit(mode: "new" | "existing") {
    setError(null);
    try {
      if (mode === "new") {
        const parent = await open({ directory: true, multiple: false, title: "选择父目录" });
        if (!parent || Array.isArray(parent)) return;
        const name = window.prompt("新建库文件夹名称", "AxMusic Library");
        if (!name) return;
        await api.initLibrary({ mode: "new", parent, name });
      } else {
        const path = await open({ directory: true, multiple: false, title: "选择已有目录作为库根" });
        if (!path || Array.isArray(path)) return;
        await api.initLibrary({ mode: "existing", path });
      }
      await reloadRoot();
    } catch (e) {
      setError(String(e));
    }
  }

  async function onChangeRoot() {
    setError(null);
    const path = await open({ directory: true, multiple: false, title: "更换库根目录" });
    if (!path || Array.isArray(path)) return;
    try {
      await api.initLibrary({ mode: "existing", path });
      await reloadRoot();
      setSelected(new Set());
    } catch (e) {
      setError(String(e));
    }
  }

  async function onRefresh() {
    setError(null);
    setScanning(true);
    setProgress(null);
    setLastScan(null);
    try {
      const r = await api.refreshScan();
      setLastScan(r);
      await reloadTracks();
      setSelected(new Set());
    } catch (e) {
      setError(String(e));
    } finally {
      setScanning(false);
    }
  }

  async function onPlayAction() {
    if (actionTracks.length === 0) return;
    const items = actionTracks.map(trackRowToQueueItem);
    await playQueue(items, 0);
  }

  function onPlaylistAction() {
    if (actionTracks.length === 0) return;
    setPickerItems(actionTracks.map(trackRowToAddItem));
  }

  // ── wizard ─────────────────────────────────────────────────────
  if (root === undefined) {
    return (
      <>
        <TopBar title="管理" />
        <div className="page-scroll">
          <div className="empty-state">加载中…</div>
        </div>
      </>
    );
  }

  if (root === null) {
    return (
      <>
        <TopBar title="管理" />
        <div className="page-scroll">
          <h2 className="display">设置库目录</h2>
          <p className="muted" style={{ maxWidth: 520, marginTop: -8 }}>
            洗库只面向库目录。新建一个文件夹，或选择已有音乐文件夹作为库根并初始化。之后手动放入音频，再「刷新扫描」。
          </p>
          <div className="wizard-actions">
            <button className="btn btn-primary wizard-btn" onClick={() => void onInit("new")}>
              <FolderPlus size={16} />
              新建目录
            </button>
            <button className="btn wizard-btn" onClick={() => void onInit("existing")}>
              <FolderSearch size={16} />
              选择已有目录
            </button>
          </div>
          {error && <div className="error-line">{error}</div>}
        </div>
      </>
    );
  }

  // ── archive table ──────────────────────────────────────────────
  return (
    <>
      <TopBar
        title="管理"
        actions={
          <>
            <div className="manage-root mono tertiary" title={root.path}>
              库根：{root.path}
            </div>
            <button className="btn" onClick={() => void onChangeRoot()}>
              更换库根
            </button>
            <button className="btn btn-primary" disabled={scanning} onClick={() => void onRefresh()}>
              {scanning ? <Loader2 size={15} className="spin" /> : <ListRestart size={15} />}
              {scanning ? "扫描中…" : "刷新扫描"}
            </button>
          </>
        }
      />

      <div className="manage-body">
        <div className="manage-main">
          <div className="manage-toolbar">
            <div className="manage-search">
              <Search size={14} className="tertiary" />
              <input
                placeholder="搜索曲目、歌手、专辑"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </div>
            <div className="manage-chips">
              <button
                className={`chip${missingOnly ? " active" : ""}`}
                onClick={() => setMissingOnly((v) => !v)}
              >
                仅缺字段
              </button>
              <button
                className={`chip${unlinkedOnly ? " active" : ""}`}
                onClick={() => setUnlinkedOnly((v) => !v)}
                title="只看未关联 catalog 的曲目（待刮削）"
              >
                未关联
              </button>
              <span className="tertiary">
                共 {filtered.length} 首
                {progress && scanning ? ` · ${progress.scanned}/${progress.totalFiles}` : ""}
                {lastScan
                  ? ` · 新增 ${lastScan.added} 更新 ${lastScan.updated} 失败 ${lastScan.errors}`
                  : ""}
              </span>
            </div>
          </div>

          {(scanning || progress) && (
            <div className="scan-bar">
              <div
                className="scan-fill"
                style={{
                  width: `${progress ? (progress.scanned / Math.max(progress.totalFiles, 1)) * 100 : 8}%`,
                }}
              />
            </div>
          )}

          {error && <div className="error-line manage-error">{error}</div>}
          {toast && (
            <div className="toast-line" onClick={() => setToast(null)}>
              {toast}
            </div>
          )}

          <div
            className="page-scroll manage-table-pane"
            style={{ paddingTop: 12 }}
            onClick={(e) => {
              // 点空白处取消选中（行内点击的 target 会落在 tr.row 内）
              if ((e.target as HTMLElement).closest("tr.row") == null) setCompareId(null);
            }}
          >
            {filtered.length === 0 ? (
              <div className="empty-state">
                <p className="muted">
                  库中还没有曲目。把 FLAC/MP3 手动放入库根，再点「刷新扫描」。
                </p>
                <button className="btn btn-primary" disabled={scanning} onClick={() => void onRefresh()}>
                  刷新扫描
                </button>
              </div>
            ) : (
              <TrackTable
                rows={filtered}
                selected={selected}
                activeId={compareId}
                onSelectedChange={setSelected}
                onPlay={(_row, idxInView) => void playQueue(filtered.map(trackRowToQueueItem), idxInView)}
                onActivate={(row) =>
                  // 再点已激活行 → 退出对比，右栏回到库统计
                  setCompareId((cur) => (cur === row.id ? null : row.id))
                }
              />
            )}
          </div>
        </div>

        <div className="manage-side">
          <div className="manage-side-scroll">
            {compareId != null ? (
              <ComparePanel
                key={`${compareId}-${compareVersion}`}
                trackId={compareId}
                onWritten={() => {
                  setCompareVersion((v) => v + 1);
                  void reloadTracks();
                }}
                onSearchLyrics={() => setLyricsTrackId(compareId)}
                onScrape={() => {
                  if (activeTrack) setScrapeTrack(activeTrack);
                }}
              />
            ) : (
              <StatsPanel stats={stats} />
            )}
          </div>
          <div className="side-actions">
            <button
              className="btn btn-primary"
              disabled={actionTracks.length === 0}
              title={actionTracks.length > 1 ? `播放所选 ${actionTracks.length} 首` : "播放"}
              onClick={() => void onPlayAction()}
            >
              <Play size={15} /> 播放
            </button>
            <button
              className="btn"
              disabled={actionTracks.length === 0}
              title="加入歌单"
              onClick={() => onPlaylistAction()}
            >
              <ListPlus size={15} /> 歌单
            </button>
          </div>
        </div>
      </div>

      {scrapeTrack && (
        <ScrapeWizard
          track={scrapeTrack}
          onClose={() => setScrapeTrack(null)}
          onApplied={() => {
            void reloadTracks();
            setCompareVersion((v) => v + 1);
          }}
        />
      )}

      {pickerItems && (
        <PlaylistPicker
          items={pickerItems}
          onClose={() => setPickerItems(null)}
          onAdded={(name) => {
            setToast(`已加入「${name}」`);
            setPickerItems(null);
          }}
        />
      )}

      {lyricsTrackId != null &&
        (() => {
          const lt = tracks.find((t) => t.id === lyricsTrackId);
          return lt ? (
            <LyricsPanel
              track={lt}
              onClose={() => setLyricsTrackId(null)}
              onSaved={() => void reloadTracks()}
            />
          ) : null;
        })()}
    </>
  );
}
