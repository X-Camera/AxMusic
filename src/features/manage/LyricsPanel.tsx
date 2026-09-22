import { listen } from "@tauri-apps/api/event";
import { Download, FileInput, FileOutput, Loader2, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { api } from "../../lib/api";
import type { LyricsBatch, LyricsCandidate, LyricsContent, LyricsCurrent, TrackRow } from "../../lib/types";
import "./LyricsPanel.css";

const SOURCE_LABEL: Record<string, string> = {
  lrclib: "LRCLIB",
  netease: "网易云",
  qq: "QQ音乐",
};

const ALL_SOURCES = ["lrclib", "netease", "qq"] as const;

/**
 * 补歌词 + 嵌/挂互转。
 * 搜索是流式的：LRCLIB / 网易云 / QQ音乐 并发跑，哪个先回就先追加进列表。
 * 默认保存为外挂 .lrc，可选内嵌。
 */
export function LyricsPanel({
  tracks,
  onClose,
  onSaved,
}: {
  tracks: TrackRow[];
  onClose: () => void;
  onSaved: () => void;
}) {
  const [idx, setIdx] = useState(0);
  const track = tracks[idx];

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

  const refreshCurrent = useCallback(async (t: TrackRow) => {
    try {
      setCurrent(await api.lyricsCurrent(t.id));
    } catch {
      setCurrent(null);
    }
  }, []);

  // 流式搜索：订阅事件 + 触发；track 切换时重置
  useEffect(() => {
    if (!track) return;
    aliveRef.current = true;
    setMessage(null);
    setError(null);
    setCandidates([]);
    setCandId(null);
    setPreview(null);
    setSourceDone(Object.fromEntries(ALL_SOURCES.map((s) => [s, "run"])));
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
      // 触发搜索（事件先订阅好，避免竞态丢批）
      api.lyricsSearch(track.id).catch((e) => {
        if (!cancelled) setError(String(e));
      });
    })();

    return () => {
      cancelled = true;
      aliveRef.current = false;
      unBatch?.();
      unDone?.();
    };
  }, [track, refreshCurrent]);

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

  async function save(mode: "sidecar" | "embed") {
    if (!track || candId == null) return;
    setSaving(true);
    setError(null);
    try {
      const msg = await api.lyricsSave(track.id, candId, mode);
      setMessage(msg);
      await refreshCurrent(track);
      onSaved();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function convert(dir: "export" | "embed") {
    if (!track) return;
    setSaving(true);
    setError(null);
    try {
      const msg =
        dir === "export"
          ? await api.lyricsExportSidecar(track.id, true)
          : await api.lyricsEmbedSidecar(track.id);
      setMessage(msg);
      await refreshCurrent(track);
      onSaved();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  if (!track) return null;

  const previewText = preview?.synced ?? preview?.plain ?? "";
  const hasEmbedded = !!current?.embedded?.trim();
  const hasSidecar = !!current?.sidecar?.trim();
  const allDone = ALL_SOURCES.every((s) => sourceDone[s] !== "run");

  return (
    <div className="lyr-overlay" role="dialog" aria-label="补歌词">
      <div className="lyr-panel">
        <header className="lyr-head">
          <div>
            <h2>补歌词</h2>
            <p className="tertiary">
              {idx + 1}/{tracks.length} · {track.title || track.filename}
              {track.artist ? ` — ${track.artist}` : ""}
            </p>
          </div>
          <button className="btn" onClick={onClose} title="关闭">
            <X size={16} />
          </button>
        </header>

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
              title="把外挂 .lrc 内嵌进文件标签（写前备份）"
              onClick={() => void convert("embed")}
            >
              <FileInput size={13} /> 外挂 → 内嵌
            </button>
          </span>
        </div>

        {error && <div className="error-line">{error}</div>}
        {message && <div className="toast-line" onClick={() => setMessage(null)}>{message}</div>}

        <div className="lyr-body">
          <section className="lyr-col">
            <div className="lyr-col-head">
              <h3>在线候选</h3>
              <div className="lyr-sources">
                {ALL_SOURCES.map((s) => (
                  <span
                    key={s}
                    className={`lyr-src ${sourceDone[s] ?? "run"}`}
                    title={
                      sourceDone[s] === "run"
                        ? "搜索中"
                        : sourceDone[s] === "err"
                          ? "该源失败"
                          : "已返回"
                    }
                  >
                    {SOURCE_LABEL[s]}
                    {sourceDone[s] === "run" && <Loader2 size={10} className="spin" />}
                  </span>
                ))}
              </div>
            </div>
            <div className="lyr-list">
              {candidates.length === 0 && !allDone && (
                <div className="tertiary lyr-empty">
                  <Loader2 size={14} className="spin" /> 等待各源返回…
                </div>
              )}
              {candidates.length === 0 && allDone && (
                <div className="tertiary lyr-empty">所有源都没有找到歌词</div>
              )}
              {candidates.map((c) => (
                <button
                  key={c.id}
                  className={`lyr-item${candId === c.id ? " active" : ""}`}
                  onClick={() => void pickCandidate(c)}
                >
                  <span className="ellipsis">
                    <span className={`lyr-src-badge ${c.source}`}>{SOURCE_LABEL[c.source]}</span>
                    {c.track_name}
                  </span>
                  <span className="tertiary ellipsis">
                    {c.artist_name}
                    {c.album_name ? ` · ${c.album_name}` : ""}
                    {c.duration > 0 ? ` · ${Math.round(c.duration / 60)}:${String(Math.round(c.duration % 60)).padStart(2, "0")}` : ""}
                  </span>
                  <span className="lyr-badges">
                    {c.has_synced && <span className="lyr-badge sync">同步</span>}
                    {!c.has_synced && c.has_plain && <span className="lyr-badge">纯文本</span>}
                  </span>
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
                className="btn btn-primary"
                disabled={candId == null || saving || !previewText}
                title="写入同目录同名 .lrc（推荐，兼容性好，不动音频文件）"
                onClick={() => void save("sidecar")}
              >
                {saving ? <Loader2 size={14} className="spin" /> : <Download size={14} />}
                外挂 .lrc（默认）
              </button>
              <button
                className="btn"
                disabled={candId == null || saving || !previewText}
                title="写入文件标签（LYRICS/USLT，写前备份）"
                onClick={() => void save("embed")}
              >
                内嵌到文件
              </button>
            </div>
          </section>
        </div>

        <footer className="lyr-foot">
          <button className="btn" disabled={idx === 0} onClick={() => setIdx((i) => i - 1)}>
            上一首
          </button>
          <button
            className="btn"
            disabled={idx >= tracks.length - 1}
            onClick={() => setIdx((i) => i + 1)}
          >
            下一首
          </button>
          <button className="btn" onClick={onClose}>
            完成
          </button>
        </footer>
      </div>
    </div>
  );
}
