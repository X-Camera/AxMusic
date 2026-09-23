import { listen } from "@tauri-apps/api/event";
import { Download, FileInput, FileOutput, Loader2, Search } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { api, formatTime } from "../../lib/api";
import type {
  LyricsBatch,
  LyricsCandidate,
  LyricsContent,
  LyricsCurrent,
  LyricsSources,
  LyricsTarget,
} from "../../lib/types";
import { emitLyricsSaved } from "../../lib/lyricsWindow";
import "./LyricsPanel.css";

const SOURCE_LABEL: Record<string, string> = {
  lrclib: "LRCLIB",
  netease: "网易云",
  qq: "QQ音乐",
};

const ALL_SOURCES = ["lrclib", "netease", "qq"] as const;

/** 库内传 trackId；库外（满窗播放）只传 path */
function lyricsRef(t: LyricsTarget): { trackId: number | null; path: string | null } {
  return t.id > 0 ? { trackId: t.id, path: null } : { trackId: null, path: t.path };
}

/**
 * 搜索歌词（独立子窗口内容，单首曲目）。
 * 打开后不自动搜索：歌手/歌名可改，点「搜索」手动触发。
 * 搜索是流式的：LRCLIB / 网易云 / QQ音乐 并发跑，哪个先回就先追加进列表。
 * 默认保存为外挂 .lrc，可选内嵌；支持嵌/挂互转。保存时优先取同步歌词。
 */
export function LyricsPanel({
  track,
  onClose,
  onSaved,
}: {
  track: LyricsTarget;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [artistInput, setArtistInput] = useState(track.artist);
  const [titleInput, setTitleInput] = useState(track.title || track.filename);
  const [searched, setSearched] = useState(false);
  const [defaultSave, setDefaultSave] = useState<"sidecar" | "embed">("sidecar");
  const [enabledSources, setEnabledSources] = useState<LyricsSources>({
    lrclib: true,
    netease: true,
    qq: true,
  });

  const [current, setCurrent] = useState<LyricsCurrent | null>(null);
  const [candidates, setCandidates] = useState<LyricsCandidate[]>([]);
  const [sourceDone, setSourceDone] = useState<Record<string, "run" | "ok" | "err">>({});
  const [candId, setCandId] = useState<string | null>(null);
  const [preview, setPreview] = useState<LyricsContent | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const aliveRef = useRef(true);

  const refreshCurrent = useCallback(async (t: LyricsTarget) => {
    try {
      const { trackId, path } = lyricsRef(t);
      setCurrent(await api.lyricsCurrent(trackId, path));
    } catch {
      setCurrent(null);
    }
  }, []);

  // 只订阅事件，不触发搜索（搜索由「搜索」按钮手动发起；先订阅好避免竞态丢批）
  useEffect(() => {
    aliveRef.current = true;
    void api.getSettings().then((s) => {
      if (!aliveRef.current) return;
      setDefaultSave(s.lyrics_save_mode);
      setEnabledSources(s.lyrics_sources);
    });
    void refreshCurrent(track);

    let unBatch: (() => void) | undefined;
    let unDone: (() => void) | undefined;
    let cancelled = false;

    (async () => {
      unBatch = await listen<LyricsBatch>("lyrics://batch", (e) => {
        if (cancelled || e.payload.trackId !== track.id) return;
        if (e.payload.error) {
          setSourceDone((m) => ({ ...m, [e.payload.source]: "err" }));
          return;
        }
        setSourceDone((m) => ({ ...m, [e.payload.source]: "ok" }));
        if (e.payload.items.length > 0) {
          setCandidates((prev) => [...prev, ...e.payload.items]);
        }
      });
      unDone = await listen<{ trackId: number }>("lyrics://done", (e) => {
        if (cancelled || e.payload.trackId !== track.id) return;
        setSourceDone((m) => {
          const next = { ...m };
          for (const s of ALL_SOURCES) if (next[s] === "run") next[s] = "ok";
          return next;
        });
      });
    })();

    return () => {
      cancelled = true;
      aliveRef.current = false;
      unBatch?.();
      unDone?.();
    };
  }, [track, refreshCurrent]);

  const searching = ALL_SOURCES.some((s) => sourceDone[s] === "run");
  const allDone = searched && !searching;

  async function runSearch() {
    const artist = artistInput.trim();
    const title = titleInput.trim();
    if (!artist && !title) {
      setError("歌手和歌名至少填一个");
      return;
    }
    setError(null);
    setMessage(null);
    setSearched(true);
    setCandidates([]);
    setCandId(null);
    setPreview(null);
    setSourceDone(Object.fromEntries(ALL_SOURCES.map((s) => [s, "run"])));
    try {
      const { trackId, path } = lyricsRef(track);
      await api.lyricsSearch(trackId, artist, title, path);
    } catch (e) {
      setError(String(e));
      setSourceDone({});
    }
  }

  async function pickCandidate(c: LyricsCandidate) {
    setCandId(c.id);
    setPreviewLoading(true);
    setError(null);
    try {
      const content = await api.lyricsFetch(c.id);
      if (!aliveRef.current) return;
      setPreview(content);
    } catch (e) {
      if (aliveRef.current) setError(String(e));
    } finally {
      if (aliveRef.current) setPreviewLoading(false);
    }
  }

  function afterWrite(msg: string) {
    setMessage(msg);
    emitLyricsSaved(track);
    void refreshCurrent(track);
    onSaved();
  }

  async function save(mode: "sidecar" | "embed") {
    if (candId == null) return;
    setSaving(true);
    setError(null);
    try {
      const { trackId, path } = lyricsRef(track);
      const msg = await api.lyricsSave(trackId, candId, mode, path);
      afterWrite(msg);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function convert(dir: "export" | "embed") {
    setSaving(true);
    setError(null);
    try {
      const { trackId, path } = lyricsRef(track);
      const msg =
        dir === "export"
          ? await api.lyricsExportSidecar(trackId, true, path)
          : await api.lyricsEmbedSidecar(trackId, path);
      afterWrite(msg);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  const previewText = preview?.synced ?? preview?.plain ?? "";
  const hasEmbedded = !!current?.embedded?.trim();
  const hasSidecar = !!current?.sidecar?.trim();

  return (
    <div className="lyr-panel" role="dialog" aria-label="搜索歌词">
      <div className="lyr-status">
        <span className={`lyr-tag${hasEmbedded ? " ok" : ""}`}>
          内嵌 {hasEmbedded ? "有" : "无"}
        </span>
        <span className={`lyr-tag${hasSidecar ? " ok" : ""}`}>
          外挂 .lrc {hasSidecar ? "有" : "无"}
        </span>
        <span className="lyr-convert">
          <button
            className="link-btn"
            disabled={!hasEmbedded || saving}
            title="把内嵌歌词导出为同目录 .lrc 文件"
            onClick={() => void convert("export")}
          >
            <FileOutput size={13} /> 内嵌 → 外挂
          </button>
          <button
            className="link-btn"
            disabled={!hasSidecar || saving}
            title="把外挂 .lrc 内嵌进文件标签"
            onClick={() => void convert("embed")}
          >
            <FileInput size={13} /> 外挂 → 内嵌
          </button>
        </span>
      </div>

      <div className="lyr-search-row">
        <input
          value={artistInput}
          onChange={(e) => setArtistInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && !searching && void runSearch()}
          placeholder="歌手"
          aria-label="歌手"
        />
        <input
          value={titleInput}
          onChange={(e) => setTitleInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && !searching && void runSearch()}
          placeholder="歌名"
          aria-label="歌名"
        />
        <button
          className="btn btn-primary"
          disabled={searching}
          onClick={() => void runSearch()}
        >
          {searching ? <Loader2 size={14} className="spin" /> : <Search size={14} />}
          {searching ? "搜索中…" : "搜索"}
        </button>
      </div>

      {error && <div className="error-line">{error}</div>}
      {message && (
        <div className="toast-line" onClick={() => setMessage(null)}>
          {message}
        </div>
      )}

      <div className="lyr-body">
        <section className="lyr-col">
          <div className="lyr-col-head">
            <h3>在线候选</h3>
            <div className="lyr-sources">
              {ALL_SOURCES.filter((s) => enabledSources[s] !== false).map((s) => (
                <span
                  key={s}
                  className={`lyr-src ${sourceDone[s] ?? "idle"}`}
                  title={
                    sourceDone[s] === "run"
                      ? "搜索中"
                      : sourceDone[s] === "err"
                        ? "该源失败"
                        : sourceDone[s] === "ok"
                          ? "已返回"
                          : "未搜索"
                  }
                >
                  {SOURCE_LABEL[s]}
                  {sourceDone[s] === "run" && <Loader2 size={10} className="spin" />}
                </span>
              ))}
            </div>
          </div>
          <div className="lyr-list">
            {!searched && (
              <div className="tertiary lyr-empty">填好歌手、歌名后点「搜索」</div>
            )}
            {searched && candidates.length === 0 && !allDone && (
              <div className="tertiary lyr-empty">
                <Loader2 size={14} className="spin" /> 等待各源返回…
              </div>
            )}
            {searched && candidates.length === 0 && allDone && (
              <div className="tertiary lyr-empty">
                所有源都没有找到歌词，可改歌手/歌名后再试
              </div>
            )}
            {candidates.map((c) => (
              <button
                key={c.id}
                className={`lyr-item${candId === c.id ? " active" : ""}`}
                onClick={() => void pickCandidate(c)}
              >
                <div className="lyr-item-main">
                  <span className="lyr-item-title ellipsis">{c.track_name || "—"}</span>
                  <span className="lyr-item-dur">
                    {c.duration > 0 ? formatTime(c.duration * 1000) : "—"}
                  </span>
                </div>
                <div className="lyr-item-sub">
                  <span className="lyr-item-artist ellipsis">{c.artist_name || "—"}</span>
                  <span className={`lyr-src-badge ${c.source}`}>{SOURCE_LABEL[c.source]}</span>
                </div>
              </button>
            ))}
          </div>
        </section>

        <section className="lyr-col wide">
          <h3>预览</h3>
          <div className="lyr-preview">
            {previewLoading && (
              <div className="tertiary lyr-empty">
                <Loader2 size={14} className="spin" /> 拉取中…
              </div>
            )}
            {!previewLoading && !preview && (
              <div className="tertiary lyr-empty">选择左侧候选后预览歌词</div>
            )}
            {!previewLoading && preview && (
              <pre className="lyr-text">{previewText || "（该候选无歌词内容）"}</pre>
            )}
          </div>
          {preview?.translation && (
            <div className="tertiary lyr-trans-hint">该候选含翻译歌词（保存时暂不合并）</div>
          )}
          <div className="lyr-save">
            <span className="tertiary">保存为：</span>
            <button
              className={defaultSave === "sidecar" ? "btn btn-primary" : "btn"}
              disabled={candId == null || saving || !previewText}
              title={
                hasSidecar
                  ? "替换同目录同名 .lrc（推荐，兼容性好，不动音频文件）"
                  : "写入同目录同名 .lrc（推荐，兼容性好，不动音频文件）"
              }
              onClick={() => void save("sidecar")}
            >
              {saving ? <Loader2 size={14} className="spin" /> : <Download size={14} />}
              外挂 .lrc{defaultSave === "sidecar" ? "（默认）" : ""}
            </button>
            <button
              className={defaultSave === "embed" ? "btn btn-primary" : "btn"}
              disabled={candId == null || saving || !previewText}
              title={
                hasEmbedded
                  ? "替换文件标签内嵌歌词（LYRICS/USLT）"
                  : "写入文件标签（LYRICS/USLT）"
              }
              onClick={() => void save("embed")}
            >
              内嵌到文件{defaultSave === "embed" ? "（默认）" : ""}
            </button>
          </div>
        </section>
      </div>

      <footer className="lyr-foot">
        <button className="btn" onClick={onClose}>
          完成
        </button>
      </footer>
    </div>
  );
}
