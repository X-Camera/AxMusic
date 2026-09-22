import { Loader2, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { api } from "../../lib/api";
import type { ApplyPlan, ScrapeCandidate, TrackRow } from "../../lib/types";
import "./ScrapeWizard.css";

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
  musicbrainz_releasegroup: "MB 专辑组",
  musicbrainz_artist: "MB 艺人",
};

export function ScrapeWizard({
  tracks,
  onClose,
  onApplied,
}: {
  tracks: TrackRow[];
  onClose: () => void;
  onApplied: () => void;
}) {
  const [mode, setMode] = useState<"album" | "track">("album");
  const [loading, setLoading] = useState(false);
  const [candidates, setCandidates] = useState<ScrapeCandidate[]>([]);
  const [selectedCand, setSelectedCand] = useState<ScrapeCandidate | null>(null);
  const [plan, setPlan] = useState<ApplyPlan | null>(null);
  const [writeCover, setWriteCover] = useState(true);
  const [applying, setApplying] = useState(false);
  const [savedCount, setSavedCount] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [activeLocal, setActiveLocal] = useState<number | null>(null);
  const [onlyChanged, setOnlyChanged] = useState(false);

  const seedAlbum = useMemo(() => {
    const t = tracks[0];
    return { album: t?.album || "", artist: t?.album_artist || t?.artist || "" };
  }, [tracks]);
  const seedTrack = useMemo(() => {
    const t = tracks[0];
    return { title: t?.title || "", artist: t?.artist || "" };
  }, [tracks]);

  const [albumQ, setAlbumQ] = useState(seedAlbum.album);
  const [albumA, setAlbumA] = useState(seedAlbum.artist);
  const [trackQ, setTrackQ] = useState(seedTrack.title);
  const [trackA, setTrackA] = useState(seedTrack.artist);

  useEffect(() => {
    setAlbumQ(seedAlbum.album);
    setAlbumA(seedAlbum.artist);
    setTrackQ(seedTrack.title);
    setTrackA(seedTrack.artist);
  }, [seedAlbum, seedTrack]);

  async function doSearch() {
    setLoading(true);
    setError(null);
    setPlan(null);
    setSelectedCand(null);
    try {
      const list =
        mode === "album"
          ? await api.scrapeSearchAlbum(albumQ, albumA)
          : await api.scrapeSearchTrack(trackQ, trackA);
      setCandidates(list);
      if (list.length === 0) setError("无候选。可改关键词后重试（MusicBrainz 免费，约 1 次/秒）。");
    } catch (e) {
      setCandidates([]);
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function pickCandidate(c: ScrapeCandidate) {
    setSelectedCand(c);
    setLoading(true);
    setError(null);
    try {
      const p = await api.scrapeBuildPlan(
        c.release_id,
        tracks.map((t) => t.id),
        mode,
        writeCover,
      );
      setPlan(p);
      if (p.tracks.length > 0) setActiveLocal(p.tracks[0].track_id);
    } catch (e) {
      setPlan(null);
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function apply() {
    if (!plan) return;
    setApplying(true);
    setError(null);
    try {
      // 只存入本地 catalog（+封面到 covers/），不修改音频文件。
      const ids = await api.catalogSave(plan, writeCover);
      setSavedCount(ids.length);
      onApplied();
    } catch (e) {
      setError(String(e));
    } finally {
      setApplying(false);
    }
  }

  const realChanges =
    plan?.tracks.reduce(
      (n, t) =>
        n + t.changes.filter((ch) => ch.old.trim() !== ch.new.trim()).length,
      0,
    ) ?? 0;
  const changeCount = plan ? plan.tracks.reduce((n, t) => n + t.changes.length, 0) : 0;

  return (
    <div className="scrape-overlay" role="dialog" aria-label="刮削向导">
      <div className="scrape-panel">
        <header className="scrape-head">
          <div>
            <h2>刮削</h2>
            <p className="tertiary">
              拉取云端字段存入本地 catalog（不改音频）· MusicBrainz + Cover Art Archive
            </p>
          </div>
          <div className="scrape-head-actions">
            <label className="scrape-check">
              <input
                type="checkbox"
                checked={writeCover}
                onChange={(e) => setWriteCover(e.target.checked)}
              />
              缓存封面到库 covers/
            </label>
            <button className="btn" onClick={onClose} title="关闭">
              <X size={16} />
            </button>
          </div>
        </header>

        <div className="scrape-search">
          <div className="scrape-tabs">
            <button
              className={`chip${mode === "album" ? " active" : ""}`}
              onClick={() => setMode("album")}
            >
              整张专辑
            </button>
            <button
              className={`chip${mode === "track" ? " active" : ""}`}
              onClick={() => setMode("track")}
            >
              单曲
            </button>
          </div>
          {mode === "album" ? (
            <div className="scrape-fields">
              <input
                placeholder="专辑名"
                value={albumQ}
                onChange={(e) => setAlbumQ(e.target.value)}
              />
              <input
                placeholder="歌手"
                value={albumA}
                onChange={(e) => setAlbumA(e.target.value)}
              />
            </div>
          ) : (
            <div className="scrape-fields">
              <input
                placeholder="曲名"
                value={trackQ}
                onChange={(e) => setTrackQ(e.target.value)}
              />
              <input
                placeholder="歌手"
                value={trackA}
                onChange={(e) => setTrackA(e.target.value)}
              />
            </div>
          )}
          <button className="btn btn-primary" disabled={loading} onClick={() => void doSearch()}>
            {loading ? <Loader2 size={15} className="spin" /> : null}
            搜索
          </button>
        </div>

        {error && <div className="error-line scrape-error">{error}</div>}

        <div className="scrape-body">
          <section className="scrape-col">
            <h3>本地列表</h3>
            <div className="scrape-list">
              {tracks.map((t) => (
                <button
                  key={t.id}
                  className={`scrape-item${activeLocal === t.id ? " active" : ""}`}
                  onClick={() => setActiveLocal(t.id)}
                >
                  <span className="ellipsis">
                    {t.track_no != null ? `${t.track_no}. ` : ""}
                    {t.title || t.filename}
                  </span>
                  <span className="tertiary ellipsis">{t.artist}</span>
                </button>
              ))}
            </div>
          </section>

          <section className="scrape-col">
            <h3>候选</h3>
            <div className="scrape-list">
              {candidates.length === 0 && !loading && (
                <div className="tertiary scrape-empty">搜索后显示 MusicBrainz 候选</div>
              )}
              {candidates.map((c) => (
                <button
                  key={c.id + c.release_id}
                  className={`scrape-item cand${
                    selectedCand?.release_id === c.release_id ? " active" : ""
                  }`}
                  onClick={() => void pickCandidate(c)}
                >
                  <span className="ellipsis">{c.title}</span>
                  <span className="tertiary ellipsis">
                    {c.artist}
                    {c.year ? ` · ${c.year}` : ""}
                    {c.track_count ? ` · ${c.track_count} 曲` : ""}
                    {c.country ? ` · ${c.country}` : ""}
                  </span>
                  {c.disambiguation && (
                    <span className="tertiary scrape-disamb">{c.disambiguation}</span>
                  )}
                </button>
              ))}
            </div>
          </section>

          <section className="scrape-col wide">
            <div className="scrape-col-head">
              <h3>字段核对（采纳后存入本地 catalog）</h3>
              <label className="scrape-check">
                <input
                  type="checkbox"
                  checked={onlyChanged}
                  onChange={(e) => setOnlyChanged(e.target.checked)}
                />
                只看有差异
              </label>
            </div>
            <div className="scrape-list">
              {!plan && (
                <div className="tertiary scrape-empty">
                  选择候选后，这里逐曲列出云端记录与文件当前字段的对照。采纳只写入本地
                  catalog，不改音频文件。
                </div>
              )}
              {plan && (
                <>
                  <div className="scrape-plan-label">
                    采纳对象：<strong>{plan.candidate_label}</strong>
                    {` · 整张 ${plan.catalog_tracks.length} 首存入 catalog`}
                    {writeCover ? " · 封面将缓存到库 covers/" : " · 不缓存封面"}
                  </div>
                  {plan.tracks.map((tp) => {
                    const rows = onlyChanged
                      ? tp.changes.filter((ch) => ch.old.trim() !== ch.new.trim())
                      : tp.changes;
                    const nChanged = tp.changes.filter(
                      (ch) => ch.old.trim() !== ch.new.trim(),
                    ).length;
                    const active = activeLocal == null || tp.track_id === activeLocal;
                    return (
                      <div
                        key={tp.track_id}
                        className={`scrape-diff${active ? "" : " dim"}`}
                        onClick={() => setActiveLocal(tp.track_id)}
                      >
                        <div className="scrape-diff-title">
                          {tp.display}
                          {nChanged > 0 && (
                            <span className="scrape-badge">{nChanged} 项差异</span>
                          )}
                        </div>
                        {tp.matched_title && tp.matched_title !== tp.display && (
                          <div className="tertiary">匹配到：{tp.matched_title}</div>
                        )}
                        {rows.length === 0 && (
                          <div className="tertiary">无差异（云端与文件一致）</div>
                        )}
                        <table className="scrape-table">
                          <thead>
                            <tr>
                              <th>字段</th>
                              <th>文件当前</th>
                              <th />
                              <th>云端</th>
                            </tr>
                          </thead>
                          <tbody>
                            {rows.map((ch, i) => {
                              const changed = ch.old.trim() !== ch.new.trim();
                              return (
                                <tr key={i} className={changed ? "changed" : ""}>
                                  <td className="f">{FIELD_LABEL[ch.field] ?? ch.field}</td>
                                  <td className="old">{ch.old || "—"}</td>
                                  <td className="arrow">→</td>
                                  <td className="new">{ch.new || "—"}</td>
                                </tr>
                              );
                            })}
                          </tbody>
                        </table>
                      </div>
                    );
                  })}
                  {plan.unmatched.length > 0 && (
                    <div className="tertiary scrape-unmatched">
                      未匹配（不入库）：{plan.unmatched.join("、")}
                    </div>
                  )}
                </>
              )}
            </div>
          </section>
        </div>

        <footer className="scrape-foot">
          {savedCount != null ? (
            <span>
              已存入本地 catalog {savedCount} 条{plan && plan.catalog_tracks.length > plan.tracks.length
                ? "（整张专辑曲目表，本地没有的曲目也已备档）"
                : ""}
              。音频文件未改动，回到管理表勾选后「写入文件」。
            </span>
          ) : (
            <span className="muted">
              {plan
                ? `匹配 ${plan.tracks.length} 首 · 与文件差异 ${realChanges} 处 · 共核对 ${changeCount} 行`
                : "选择候选后核对字段，采纳后仅存入本地 catalog"}
              {writeCover ? "（含封面缓存）" : ""}
            </span>
          )}
          <div className="scrape-foot-actions">
            <button className="btn" onClick={onClose}>
              {savedCount != null ? "完成" : "取消"}
            </button>
            <button
              className="btn btn-primary"
              disabled={!plan || applying || savedCount != null}
              onClick={() => void apply()}
            >
              {applying ? <Loader2 size={15} className="spin" /> : null}
              {savedCount != null ? "已存入 catalog" : "存入本地 catalog"}
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}
