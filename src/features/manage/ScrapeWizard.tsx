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

/** 单曲刮削：拉取云端字段存入本地 catalog（不改音频文件）。 */
export function ScrapeWizard({
  track,
  onClose,
  onApplied,
}: {
  track: TrackRow;
  onClose: () => void;
  onApplied: () => void;
}) {
  const [mode, setMode] = useState<"album" | "track">("track");
  const [loading, setLoading] = useState(false);
  const [candidates, setCandidates] = useState<ScrapeCandidate[]>([]);
  const [selectedCand, setSelectedCand] = useState<ScrapeCandidate | null>(null);
  const [plan, setPlan] = useState<ApplyPlan | null>(null);
  const [applying, setApplying] = useState(false);
  const [savedCount, setSavedCount] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [onlyChanged, setOnlyChanged] = useState(false);

  const seedAlbum = useMemo(
    () => ({ album: track.album || "", artist: track.album_artist || track.artist || "" }),
    [track],
  );
  const seedTrack = useMemo(
    () => ({ title: track.title || track.filename || "", artist: track.artist || "" }),
    [track],
  );

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
      const p = await api.scrapeBuildPlan(c.release_id, [track.id], mode);
      setPlan(p);
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
      // 只存入本地 catalog（文字），不修改音频文件；封面在边栏单独刮取。
      const ids = await api.catalogSave(plan);
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
      (n, t) => n + t.changes.filter((ch) => ch.old.trim() !== ch.new.trim()).length,
      0,
    ) ?? 0;
  const changeCount = plan ? plan.tracks.reduce((n, t) => n + t.changes.length, 0) : 0;
  const localPlan = plan?.tracks[0] ?? null;
  const rows = localPlan
    ? onlyChanged
      ? localPlan.changes.filter((ch) => ch.old.trim() !== ch.new.trim())
      : localPlan.changes
    : [];

  return (
    <div className="scrape-overlay" role="dialog" aria-label="刮削向导">
      <div className="scrape-panel">
        <header className="scrape-head">
          <div>
            <h2>刮削</h2>
            <p className="tertiary">
              单曲拉取云端字段存入本地 catalog（不改音频）· MusicBrainz · 封面在边栏单独刮取
            </p>
            <p className="scrape-track-line">
              本地曲目：<strong>{track.title || track.filename}</strong>
              {track.artist ? <span className="tertiary"> · {track.artist}</span> : null}
            </p>
          </div>
          <div className="scrape-head-actions">
            <button className="btn" onClick={onClose} title="关闭">
              <X size={16} />
            </button>
          </div>
        </header>

        <div className="scrape-search">
          <div className="scrape-tabs">
            <button
              className={`chip${mode === "track" ? " active" : ""}`}
              onClick={() => setMode("track")}
            >
              按曲名
            </button>
            <button
              className={`chip${mode === "album" ? " active" : ""}`}
              onClick={() => setMode("album")}
            >
              按专辑
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

        <div className="scrape-body two-col">
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
                  选择候选后，这里列出云端记录与文件当前字段的对照。采纳只写入本地
                  catalog，不改音频文件。
                </div>
              )}
              {plan && localPlan && (
                <>
                  <div className="scrape-plan-label">
                    采纳对象：<strong>{plan.candidate_label}</strong>
                    {plan.catalog_tracks.length > 1
                      ? ` · 整张 ${plan.catalog_tracks.length} 首存入 catalog`
                      : ""}
                  </div>
                  <div className="scrape-diff">
                    <div className="scrape-diff-title">
                      {localPlan.display}
                      {realChanges > 0 && <span className="scrape-badge">{realChanges} 项差异</span>}
                    </div>
                    {localPlan.matched_title && localPlan.matched_title !== localPlan.display && (
                      <div className="tertiary">匹配到：{localPlan.matched_title}</div>
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
                  {plan.tracks.length === 0 && (
                    <div className="tertiary scrape-unmatched">
                      云端未匹配到这首曲目。可换候选，或用「按曲名」再搜。
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
              已存入本地 catalog {savedCount} 条
              {plan && plan.catalog_tracks.length > 1
                ? "（整张专辑曲目表，本地没有的曲目也已备档）"
                : ""}
              。音频文件未改动，可在边栏逐字段写入。
            </span>
          ) : (
            <span className="muted">
              {plan
                ? `与文件差异 ${realChanges} 处 · 共核对 ${changeCount} 行`
                : "选择候选后核对字段，采纳后仅存入本地 catalog"}
            </span>
          )}
          <div className="scrape-foot-actions">
            <button className="btn" onClick={onClose}>
              {savedCount != null ? "完成" : "取消"}
            </button>
            <button
              className="btn btn-primary"
              disabled={!plan || plan.tracks.length === 0 || applying || savedCount != null}
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
