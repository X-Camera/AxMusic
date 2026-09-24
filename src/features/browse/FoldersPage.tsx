import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { MouseEvent as ReactMouseEvent, ReactNode } from "react";
import {
  ChevronDown,
  ChevronRight,
  Folder,
  FolderOpen,
  FolderSearch,
  ListPlus,
  ListTree,
  Loader2,
  Play,
  X,
} from "lucide-react";

import {
  api,
  folderFileToAddItem,
  folderFileToQueueItem,
  formatTime,
} from "../../lib/api";
import type { FolderDir, FolderFile, PlaylistAddItem } from "../../lib/types";
import { useApp } from "../../state/useApp";
import { TopBar } from "../../components/TopBar";
import { PlaylistPicker } from "../playlists/PlaylistPicker";
import { FavoriteHeart } from "../../components/FavoriteHeart";
import "./Folders.css";

const ROOT_KEY = "axmusic.folders.root";
const TREE_W_KEY = "axmusic.folders.treeWidth";

type TreeNode = {
  path: string;
  name: string;
  depth: number;
  expanded: boolean;
  loading: boolean;
  children: TreeNode[];
  loaded: boolean;
};

function makeNode(path: string, name: string, depth: number): TreeNode {
  return { path, name, depth, expanded: false, loading: false, children: [], loaded: false };
}

function mapNode(nodes: TreeNode[], path: string, fn: (n: TreeNode) => TreeNode): TreeNode[] {
  return nodes.map((n) => {
    if (n.path === path) return fn(n);
    if (n.children.length === 0) return n;
    const kids = mapNode(n.children, path, fn);
    return kids === n.children ? n : { ...n, children: kids };
  });
}

function setChildrenAt(nodes: TreeNode[], path: string, dirs: FolderDir[], expanded: boolean): TreeNode[] {
  return mapNode(nodes, path, (n) => ({
    ...n,
    expanded,
    loaded: true,
    children: dirs.map((d) => makeNode(d.path, d.name, n.depth + 1)),
  }));
}

function joinPath(base: string, part: string): string {
  const sep = base.includes("\\") && !base.includes("/") ? "\\" : "/";
  const trimmed = base.endsWith("\\") || base.endsWith("/") ? base : base + sep;
  return trimmed + part;
}

function normPath(p: string): string {
  return p.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
}

export function FoldersPage() {
  const playQueue = useApp((s) => s.playQueue);
  const [rootPath, setRootPath] = useState<string | null>(null);
  const [tree, setTree] = useState<TreeNode[]>([]);
  const [currentPath, setCurrentPath] = useState<string | null>(null);
  const [listingParent, setListingParent] = useState<string | null>(null);
  const [files, setFiles] = useState<FolderFile[]>([]);
  const [recursive, setRecursive] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [pickerItems, setPickerItems] = useState<PlaylistAddItem[] | null>(null);
  const lastClickIdx = useRef<number | null>(null);
  /** 启动默认根只拉一次 */
  const bootstrapped = useRef(false);
  /** 后台补标签代数：换目录/卸载后丢弃过期批次 */
  const enrichGen = useRef(0);
  /** 补标签优先队列（路径）；可视行插队到队首 */
  const enrichQueue = useRef<string[]>([]);
  const enrichQueued = useRef(new Set<string>());
  const enrichRunning = useRef(false);
  /** 当前视口（含预取）内的路径 */
  const visiblePaths = useRef(new Set<string>());
  const listWrapRef = useRef<HTMLDivElement | null>(null);
  const [treeWidth, setTreeWidth] = useState(() => {
    const n = Number(window.localStorage.getItem(TREE_W_KEY));
    return Number.isFinite(n) && n >= 180 && n <= 480 ? n : 240;
  });

  const mergeMeta = useCallback((updates: FolderFile[]) => {
    if (updates.length === 0) return;
    const map = new Map(updates.map((u) => [u.path, u]));
    setFiles((prev) =>
      prev.map((f) => {
        const u = map.get(f.path);
        return u ? { ...f, title: u.title || f.title, artist: u.artist, duration_ms: u.duration_ms } : f;
      }),
    );
  }, []);

  /** 可视行插队：挪到队首（仅已在队列中的） */
  const prioritizeEnrich = useCallback((paths: string[]) => {
    const want = new Set(paths);
    const front = enrichQueue.current.filter((p) => want.has(p));
    if (front.length === 0) return;
    const rest = enrichQueue.current.filter((p) => !want.has(p));
    enrichQueue.current = [...front, ...rest];
  }, []);

  /** 消费优先队列：小批量，保证插队能快速生效 */
  const kickEnrich = useCallback(
    async (gen: number) => {
      if (enrichRunning.current) return;
      enrichRunning.current = true;
      try {
        while (enrichQueue.current.length > 0 && gen === enrichGen.current) {
          // 每批前把可视行顶到队首
          prioritizeEnrich([...visiblePaths.current]);
          const batch = enrichQueue.current.splice(0, 16);
          for (const p of batch) enrichQueued.current.delete(p);
          if (batch.length === 0) break;
          const got = await api.folderMetaRead(batch);
          if (gen !== enrichGen.current) return;
          mergeMeta(got);
        }
      } catch {
        /* 元数据失败不影响浏览 */
      } finally {
        enrichRunning.current = false;
      }
    },
    [mergeMeta, prioritizeEnrich],
  );

  /** 后台补标签：先缓存；miss 入队，首屏优先，再按队列/可视插队补 */
  const enrichFiles = useCallback(
    async (list: FolderFile[], gen: number) => {
      enrichQueue.current = [];
      enrichQueued.current.clear();
      visiblePaths.current.clear();
      const paths = list.map((f) => f.path);
      try {
        const cached = await api.folderMetaLookup(paths);
        if (gen !== enrichGen.current) return;
        mergeMeta(cached);
        const hit = new Set(cached.map((c) => c.path));
        const missing = paths.filter((p) => !hit.has(p));
        // 首屏（列表顶部约两屏）优先入队，其余排后
        const firstPage = missing.slice(0, 50);
        const rest = missing.slice(50);
        for (const p of [...firstPage, ...rest]) {
          if (!enrichQueued.current.has(p)) {
            enrichQueue.current.push(p);
            enrichQueued.current.add(p);
          }
        }
        // 首批直接给高优先
        prioritizeEnrich(firstPage);
        void kickEnrich(gen);
      } catch {
        /* 元数据失败不影响浏览 */
      }
    },
    [kickEnrich, mergeMeta, prioritizeEnrich],
  );

  const loadFiles = useCallback(
    async (path: string, rec: boolean) => {
      const gen = ++enrichGen.current;
      setLoading(true);
      setError(null);
      try {
        // 只要 parent/子目录；文件单独取（递归时不扫本层 files）
        const meta = await api.listDirTree(path, false);
        if (gen !== enrichGen.current) return;
        setListingParent(meta.parent);
        const list = rec
          ? await api.listDirAudioRecursive(path)
          : (await api.listDirTree(path, true)).files;
        if (gen !== enrichGen.current) return;
        setFiles(list);
        setSelected(new Set());
        lastClickIdx.current = null;
        // 列表先出，标签后台补（缓存优先）
        void enrichFiles(list, gen);
      } catch (e) {
        if (gen === enrichGen.current) {
          setFiles([]);
          setError(String(e));
        }
      } finally {
        if (gen === enrichGen.current) setLoading(false);
      }
    },
    [enrichFiles],
  );

  /** 打开新根目录；expandTo 可选，沿路径展开到该文件夹 */
  const openRoot = useCallback(
    async (path: string, expandTo?: string) => {
      setRootPath(path);
      setError(null);
      try {
        // 树只要子目录；文件由 loadFiles 拉
        const listing = await api.listDirTree(path, false);
        const root = makeNode(path, path, 0);
        root.loaded = true;
        root.expanded = true;
        root.children = listing.dirs.map((d) => makeNode(d.path, d.name, 1));
        let next: TreeNode[] = [root];
        let target = path;

        if (expandTo && normPath(expandTo) !== normPath(path) && normPath(expandTo).startsWith(normPath(path) + "/")) {
          const rel = expandTo.slice(path.length).replace(/^[\\/]+/, "");
          const parts = rel.split(/[\\/]/).filter(Boolean);
          let cur = path;
          for (const part of parts) {
            cur = joinPath(cur, part);
            const sub = await api.listDirTree(cur, false);
            next = setChildrenAt(next, cur, sub.dirs, true);
          }
          target = expandTo;
        }

        setTree(next);
        setCurrentPath(target);
        window.localStorage.setItem(ROOT_KEY, path);
        await loadFiles(target, recursive);
      } catch (e) {
        setTree([]);
        setCurrentPath(null);
        setFiles([]);
        setError(String(e));
      }
    },
    [loadFiles, recursive],
  );

  useEffect(() => {
    if (bootstrapped.current) return;
    bootstrapped.current = true;
    (async () => {
      const saved = window.localStorage.getItem(ROOT_KEY);
      let prefer = saved;
      if (!prefer) {
        try {
          prefer = (await api.getLibraryRoot())?.path ?? null;
        } catch {
          prefer = null;
        }
      }
      if (prefer) await openRoot(prefer);
    })();
  }, [openRoot]);

  // 卸载时打断在途补标签
  useEffect(() => () => {
    enrichGen.current += 1;
  }, []);

  // 可视行插队：进入视口（预取 320px）的未补歌曲优先
  useEffect(() => {
    const root = listWrapRef.current;
    if (!root || files.length === 0) return;
    const obs = new IntersectionObserver(
      (entries) => {
        let bumped = false;
        for (const e of entries) {
          const path = (e.target as HTMLElement).dataset.path;
          if (!path) continue;
          if (e.isIntersecting) {
            visiblePaths.current.add(path);
            if (enrichQueued.current.has(path)) {
              prioritizeEnrich([path]);
              bumped = true;
            }
          } else {
            visiblePaths.current.delete(path);
          }
        }
        if (bumped && enrichQueue.current.length > 0) {
          void kickEnrich(enrichGen.current);
        }
      },
      { root, rootMargin: "320px 0px" },
    );
    const rows = root.querySelectorAll<HTMLElement>(".folders-row[data-path]");
    rows.forEach((el) => obs.observe(el));
    return () => obs.disconnect();
  }, [files, kickEnrich, prioritizeEnrich]);

  // 切换「含子文件夹」时重载
  useEffect(() => {
    if (!currentPath || !rootPath) return;
    void loadFiles(currentPath, recursive);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [recursive]);

  const actionFiles = useMemo(() => {
    if (selected.size > 0) return files.filter((f) => selected.has(f.path));
    return files;
  }, [files, selected]);

  async function toggleExpand(node: TreeNode) {
    if (node.expanded) {
      setTree((t) => mapNode(t, node.path, (n) => ({ ...n, expanded: false })));
      return;
    }
    if (!node.loaded) {
      setTree((t) => mapNode(t, node.path, (n) => ({ ...n, loading: true })));
      try {
        const listing = await api.listDirTree(node.path, false);
        setTree((t) =>
          mapNode(t, node.path, (n) => ({
            ...n,
            loading: false,
            loaded: true,
            expanded: true,
            children: listing.dirs.map((d) => makeNode(d.path, d.name, n.depth + 1)),
          })),
        );
      } catch (e) {
        setTree((t) => mapNode(t, node.path, (n) => ({ ...n, loading: false })));
        setError(String(e));
      }
      return;
    }
    setTree((t) => mapNode(t, node.path, (n) => ({ ...n, expanded: true })));
  }

  function selectFolder(path: string) {
    setCurrentPath(path);
    void loadFiles(path, recursive);
  }

  function renderNodes(nodes: TreeNode[]): ReactNode {
    return nodes.map((n) => (
      <div key={n.path}>
        <div
          className={`ft-row${currentPath === n.path ? " active" : ""}`}
          style={{ paddingLeft: 8 + n.depth * 14 }}
          title={n.path}
          onClick={() => selectFolder(n.path)}
        >
          <button
            className="ft-twist"
            title={n.expanded ? "收起" : "展开"}
            onClick={(e) => {
              e.stopPropagation();
              void toggleExpand(n);
            }}
          >
            {n.loading ? (
              <Loader2 size={12} className="spin" />
            ) : n.expanded ? (
              <ChevronDown size={12} />
            ) : (
              <ChevronRight size={12} />
            )}
          </button>
          {n.expanded ? (
            <FolderOpen size={14} className="ft-icon" />
          ) : (
            <Folder size={14} className="ft-icon" />
          )}
          <span className="ellipsis">{n.depth === 0 ? n.path : n.name}</span>
        </div>
        {n.expanded && n.children.length > 0 && renderNodes(n.children)}
      </div>
    ));
  }

  function toggleOne(idx: number, shift: boolean) {
    setSelected((prev) => {
      const next = new Set(prev);
      const path = files[idx].path;
      if (shift && lastClickIdx.current != null) {
        const a = Math.min(lastClickIdx.current, idx);
        const b = Math.max(lastClickIdx.current, idx);
        for (let i = a; i <= b; i++) next.add(files[i].path);
      } else if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      lastClickIdx.current = idx;
      return next;
    });
  }

  function toggleAll() {
    if (files.length > 0 && selected.size === files.length) {
      setSelected(new Set());
      lastClickIdx.current = null;
    } else {
      setSelected(new Set(files.map((f) => f.path)));
    }
  }

  function clearSelection() {
    setSelected(new Set());
    lastClickIdx.current = null;
  }

  /** 点单曲：只播这一首，并替换当前播放队列 */
  function playOne(f: FolderFile) {
    void playQueue([folderFileToQueueItem(f)], 0);
  }

  function playAction() {
    const list = actionFiles;
    if (list.length === 0) return;
    void playQueue(list.map(folderFileToQueueItem), 0);
  }

  function addPlaylist() {
    const list = actionFiles;
    if (list.length === 0) return;
    setPickerItems(list.map(folderFileToAddItem));
  }

  async function onPickRoot() {
    const picked = await open({ directory: true, multiple: false, title: "选择文件夹" });
    if (!picked || Array.isArray(picked)) return;
    await openRoot(picked);
  }

  async function goUp() {
    if (!listingParent) return;
    if (currentPath && rootPath && currentPath !== rootPath) {
      selectFolder(listingParent);
      return;
    }
    await openRoot(listingParent);
  }

  function showToast(msg: string) {
    setToast(msg);
    window.setTimeout(() => setToast(null), 2000);
  }

  /** 树宽拖拽（记 localStorage，目录深时可拉宽） */
  const onSplitterDown = (e: ReactMouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = treeWidth;
    let latest = startW;
    const move = (ev: MouseEvent) => {
      latest = Math.min(480, Math.max(180, startW + (ev.clientX - startX)));
      setTreeWidth(latest);
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      window.localStorage.setItem(TREE_W_KEY, String(latest));
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  const hasFiles = files.length > 0;
  const selCount = selected.size;

  return (
    <>
      <TopBar
        title="目录"
        actions={
          <>
            {toast && <span className="ok-toast">{toast}</span>}
            <button
              className="btn"
              disabled={!hasFiles}
              title={
                selCount > 0
                  ? `加入歌单（已选 ${selCount} 首）`
                  : recursive
                    ? "全部加入歌单（含子文件夹）"
                    : "全部加入歌单"
              }
              onClick={addPlaylist}
            >
              <ListPlus size={15} /> {selCount > 0 ? "加入歌单" : "全部加入歌单"}
            </button>
            <button
              className="btn btn-primary"
              disabled={!hasFiles}
              title={
                selCount > 0
                  ? `播放所选 ${selCount} 首`
                  : recursive
                    ? "播放全部（含子文件夹）"
                    : "播放全部"
              }
              onClick={playAction}
            >
              <Play size={15} /> {selCount > 0 ? "播放选中" : "播放全部"}
            </button>
          </>
        }
      />

      {!rootPath ? (
        <div className="page-scroll">
          <div className="empty-state">
            <div className="display" style={{ fontSize: 20 }}>
              浏览文件夹
            </div>
            <p className="muted" style={{ maxWidth: 420 }}>
              不依赖库目录，任意路径均可浏览播放。选择一个音乐文件夹作为浏览根目录。
            </p>
            <button className="btn btn-primary" onClick={() => void onPickRoot()}>
              <FolderSearch size={16} /> 选择目录
            </button>
            {error && <div className="folders-error">{error}</div>}
          </div>
        </div>
      ) : (
        <div className="folders-body">
          <div className="folders-tree-col" style={{ width: treeWidth }}>
            {/* 按钮浮在页面空白上，不进树面板 */}
            <div className="folders-tree-actions">
              <button className="btn" title="更换根目录" onClick={() => void onPickRoot()}>
                <FolderSearch size={14} /> 选择目录
              </button>
              <button
                className={`chip${recursive ? " active" : ""}`}
                title="列出并处理当前文件夹下全部音频（含子目录）"
                onClick={() => setRecursive((v) => !v)}
              >
                <ListTree size={12} /> 含子文件夹
              </button>
            </div>
            <aside className="folders-tree panel">
              {/* 标题行与右侧列表表头同高、同基线 */}
              <div className="folders-tree-head">
                <span className="tertiary">目录树</span>
                <button
                  className="link-btn"
                  disabled={!listingParent}
                  title={listingParent ? "上一级" : "已在根"}
                  onClick={() => void goUp()}
                >
                  上一级
                </button>
              </div>
              <div className="folders-tree-scroll">{renderNodes(tree)}</div>
            </aside>
          </div>

          <div
            className="folders-splitter"
            title="拖动调整目录树宽度"
            onMouseDown={onSplitterDown}
          >
            <div className="folders-splitter-grip" />
          </div>

          <section className="folders-main">
            <div className="folders-toolbar">
              <div className="folders-chips">
                {selCount > 0 ? (
                  <>
                    <span className="tertiary">
                      已选 {selCount} / {files.length} 首
                    </span>
                    <button className="chip" onClick={clearSelection}>
                      <X size={12} /> 清除选择
                    </button>
                  </>
                ) : (
                  <span className="tertiary">共 {files.length} 首</span>
                )}
                <button className="chip" disabled={!hasFiles} onClick={toggleAll}>
                  {files.length > 0 && selCount === files.length ? "取消全选" : "全选"}
                </button>
              </div>
            </div>

            {error && <div className="folders-error">{error}</div>}

            <div className="folders-list-wrap" ref={listWrapRef}>
              {loading && files.length === 0 ? (
                <div className="empty-state">加载中…</div>
              ) : error && files.length === 0 ? (
                <div className="empty-state">
                  <p className="muted">读取文件夹失败：{error}</p>
                </div>
              ) : files.length === 0 ? (
                <div className="empty-state">
                  <p className="muted">
                    {recursive
                      ? "该文件夹（含子目录）没有音频文件。"
                      : "本层没有音频文件。可开「含子文件夹」，或点左侧其它目录。"}
                  </p>
                </div>
              ) : (
                <div className="folders-list">
                  <div className="folders-head tertiary">
                    <span />
                    <span>#</span>
                    <span />
                    <span>曲名</span>
                    <span>歌手</span>
                    <span>文件</span>
                    <span>时长</span>
                  </div>
                  {files.map((f, idx) => {
                    const checked = selected.has(f.path);
                    return (
                      <div
                        key={f.path}
                        data-path={f.path}
                        className={`folders-row${checked ? " selected" : ""}`}
                        title="点曲名播放 · 勾选多选 · Shift 连选"
                        onDoubleClick={() => playOne(f)}
                      >
                        {/* 只让 input 的 onChange 切换；span 空白区单独 onClick。
                            否则点到勾选框会 click+change 各切一次，表现为「点了没反应」 */}
                        <span
                          className="folders-check"
                          onClick={(e) => {
                            e.stopPropagation();
                            if (e.target === e.currentTarget) {
                              toggleOne(idx, e.shiftKey);
                            }
                          }}
                        >
                          <input
                            type="checkbox"
                            checked={checked}
                            onChange={(e) =>
                              toggleOne(idx, (e.nativeEvent as MouseEvent).shiftKey)
                            }
                            onClick={(e) => e.stopPropagation()}
                            aria-label={`选择 ${f.name}`}
                          />
                        </span>
                        <span className="tertiary mono">{String(idx + 1).padStart(2, "0")}</span>
                        <span className="fav-col">
                          <FavoriteHeart
                            item={{
                              path: f.path,
                              title: f.title || f.name,
                              artist: f.artist,
                              duration_ms: f.duration_ms,
                            }}
                          />
                        </span>
                        <span className="ellipsis folders-title" onClick={() => playOne(f)}>
                          {f.title || f.name}
                        </span>
                        <span className="tertiary ellipsis">{f.artist || "—"}</span>
                        <span className="tertiary mono ellipsis" title={f.path}>
                          {f.name}
                        </span>
                        <span className="tertiary mono">
                          {f.duration_ms > 0 ? formatTime(f.duration_ms) : "—"}
                        </span>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          </section>
        </div>
      )}

      {pickerItems && (
        <PlaylistPicker
          items={pickerItems}
          onClose={() => setPickerItems(null)}
          onAdded={(name) => {
            setPickerItems(null);
            showToast(`已加入「${name}」`);
          }}
        />
      )}
    </>
  );
}
