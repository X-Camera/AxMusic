import { listen } from "@tauri-apps/api/event";
import { isNil, notNil } from "../../lib/nil";
import { friendlyErr } from "../../lib/errors";
import { Loader2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { api } from "../../lib/api";
import { nextSearchId } from "../../lib/async";
import type {
  ApplyPlan,
  FieldChange,
  ScrapeBatch,
  ScrapeCandidate,
  TrackAlbum,
  TrackRow,
} from "../../lib/types";
import "./ScrapeWizard.css";

/** 刮削源展示名 */
const SOURCE_LABEL: Record<string, string> = {
  musicbrainz: "MB",
  itunes: "iTunes",
  netease: "网易云",
  qq: "QQ",
};

/** MB 恒排最前（后到也插队），其余按到达顺序（sort 稳定）。 */
function sortCandidatesMbFirst(items: ScrapeCandidate[]): ScrapeCandidate[] {
  return [...items].sort((a, b) => {
    const ra = a.source === "musicbrainz" ? 0 : 1;
    const rb = b.source === "musicbrainz" ? 0 : 1;
    return ra - rb;
  });
}

/** 专辑类型展示（MB release-group primary-type） */
function albumTypeLabel(t: string): string {
  if (!t) return "";
  const map: Record<string, string> = {
    Album: "专辑",
    Single: "单曲",
    EP: "EP",
    Compilation: "精选",
    Soundtrack: "原声",
    Live: "现场",
    Remix: "混音",
    Other: "其他",
  };
  return map[t] ?? t;
}

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

/** 页脚提示文案：未选候选 / 未匹配本地曲目 / 差异统计三态（避免嵌套三元）。 */
function footHint(plan: ApplyPlan | null, realChanges: number, changeCount: number): string {
  if (!plan) return "选中候选即可存入本地 catalog（不改音频文件）";
  if (plan.tracks.length === 0) {
    return "未匹配到本地曲目——可在曲目列挑一首，或直接整张存入 catalog";
  }
  return `与文件差异 ${realChanges} 处 · 共核对 ${changeCount} 行`;
}

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
  const [mode, setMode] = useState<"album" | "track">("album");
  const [loading, setLoading] = useState(false);
  const [candidates, setCandidates] = useState<ScrapeCandidate[]>([]);
  const [selectedCand, setSelectedCand] = useState<ScrapeCandidate | null>(null);
  /** 单曲模式：所属专辑列表（挑专辑） */
  const [trackAlbums, setTrackAlbums] = useState<TrackAlbum[]>([]);
  const [selectedAlbum, setSelectedAlbum] = useState<TrackAlbum | null>(null);
  const [plan, setPlan] = useState<ApplyPlan | null>(null);
  const [applying, setApplying] = useState(false);
  const [savedCount, setSavedCount] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [onlyChanged, setOnlyChanged] = useState(false);
  /** 各源错误（source → 错误信息），不打断其它源 */
  const [sourceErrs, setSourceErrs] = useState<Record<string, string>>({});
  /** 当前搜索代次（全局 id，向导重开后不与上一轮撞车） */
  const searchIdRef = useRef(0);
  /** 候选/远程曲目请求序号：连点只认最后一次 plan */
  const planSeqRef = useRef(0);

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

  // 四源流式结果：挂载期间常驻监听，按 searchId 只收当前搜索的批次
  useEffect(() => {
    let cancelled = false;
    let unBatch: (() => void) | undefined;
    let unDone: (() => void) | undefined;
    void (async () => {
      unBatch = await listen<ScrapeBatch>("scrape://batch", (e) => {
        if (cancelled || e.payload.searchId !== searchIdRef.current) return;
        if (e.payload.error) {
          setSourceErrs((m) => ({ ...m, [e.payload.source]: e.payload.error ?? "" }));
          return;
        }
        if (e.payload.items.length > 0) {
          setCandidates((prev) => sortCandidatesMbFirst([...prev, ...e.payload.items]));
        }
      });
      unDone = await listen<{ searchId: number }>("scrape://done", (e) => {
        if (cancelled || e.payload.searchId !== searchIdRef.current) return;
        setLoading(false);
      });
    })();
    return () => {
      cancelled = true;
      unBatch?.();
      unDone?.();
    };
  }, []);

  /** 切换模式：候选/计划/已存标记全部作废，避免跨模式残留误导。 */
  function switchMode(m: "album" | "track") {
    if (m === mode) return;
    setMode(m);
    setCandidates([]);
    setPlan(null);
    setSelectedCand(null);
    setTrackAlbums([]);
    setSelectedAlbum(null);
    setSavedCount(null);
    setError(null);
    setSourceErrs({});
  }

  async function doSearch() {
    planSeqRef.current += 1;
    const sid = nextSearchId();
    searchIdRef.current = sid;
    setLoading(true);
    setError(null);
    setPlan(null);
    setSelectedCand(null);
    setTrackAlbums([]);
    setSelectedAlbum(null);
    setSavedCount(null);
    setSourceErrs({});
    setCandidates([]);
    try {
      if (mode === "album") {
        await api.scrapeSearchAlbum(sid, albumQ, albumA);
      } else {
        await api.scrapeSearchTrack(sid, trackQ, trackA);
      }
    } catch (e) {
      setError(friendlyErr(e));
      setLoading(false);
    }
  }

  async function pickCandidate(c: ScrapeCandidate) {
    const seq = ++planSeqRef.current;
    setSelectedCand(c);
    setPlan(null);
    setSavedCount(null);
    setLoading(true);
    setError(null);
    setTrackAlbums([]);
    setSelectedAlbum(null);
    try {
      if (mode === "track") {
        // 先拉所属专辑（MB 可多条），默认选第一条并按专辑模式建计划（带曲目表）
        const albums = await api.scrapeTrackAlbums(c.source, c.release_id);
        if (seq !== planSeqRef.current) return;
        setTrackAlbums(albums);
        const first = albums[0] ?? null;
        if (first) {
          setSelectedAlbum(first);
          const p = await api.scrapeBuildPlan(first.source, first.release_id, [track.id], "album");
          if (seq !== planSeqRef.current) return;
          setPlan(p);
        } else {
          // 无所属专辑信息：退回单曲计划（只补 title/artist/album/year）
          const p = await api.scrapeBuildPlan(c.source, c.release_id, [track.id], "track");
          if (seq !== planSeqRef.current) return;
          setPlan(p);
        }
      } else {
        const p = await api.scrapeBuildPlan(c.source, c.release_id, [track.id], mode);
        if (seq !== planSeqRef.current) return;
        setPlan(p);
      }
    } catch (e) {
      if (seq !== planSeqRef.current) return;
      setPlan(null);
      setError(friendlyErr(e));
    } finally {
      if (seq === planSeqRef.current) setLoading(false);
    }
  }

  /** 单曲模式：换一个所属专辑 → 重拉整张曲目表 + 字段计划。 */
  async function pickTrackAlbum(al: TrackAlbum) {
    if (!selectedCand) return;
    const seq = ++planSeqRef.current;
    setSelectedAlbum(al);
    setLoading(true);
    setError(null);
    setSavedCount(null);
    try {
      const p = await api.scrapeBuildPlan(al.source, al.release_id, [track.id], "album");
      if (seq !== planSeqRef.current) return;
      setPlan(p);
    } catch (e) {
      if (seq !== planSeqRef.current) return;
      setPlan(null);
      setError(friendlyErr(e));
    } finally {
      if (seq === planSeqRef.current) setLoading(false);
    }
  }

  /** 自动匹配失败/匹配不对时，从专辑曲目列表手动指定本地曲目对应的一首。 */
  async function pickRemoteTrack(trackNo: number) {
    // 单曲模式已选所属专辑时，以该专辑为上下文（整张曲目表）；否则用当前候选
    const source = selectedAlbum?.source ?? selectedCand?.source;
    const releaseId = selectedAlbum?.release_id ?? selectedCand?.release_id;
    if (!source || !releaseId) return;
    const planMode: "album" | "track" = selectedAlbum ? "album" : mode;
    const seq = ++planSeqRef.current;
    setLoading(true);
    setError(null);
    setSavedCount(null);
    try {
      const p = await api.scrapeBuildPlan(source, releaseId, [track.id], planMode, trackNo);
      if (seq !== planSeqRef.current) return;
      setPlan(p);
    } catch (e) {
      if (seq !== planSeqRef.current) return;
      setPlan(null);
      setError(friendlyErr(e));
    } finally {
      if (seq === planSeqRef.current) setLoading(false);
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
      setError(friendlyErr(e));
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
  let rows: FieldChange[] = [];
  if (localPlan) {
    rows = onlyChanged
      ? localPlan.changes.filter((ch) => ch.old.trim() !== ch.new.trim())
      : localPlan.changes;
  }

  /** 中间列：整张曲目表（按轨号排序） */
  const albumTracks = useMemo(
    () => [...(plan?.catalog_tracks ?? [])].sort((a, b) => (a.track_no ?? 0) - (b.track_no ?? 0)),
    [plan],
  );
  /** 本地曲目当前配对到的远端（自动或手动）：MB 用录音 MBID，其它源用轨号——中间列高亮 */
  const matchedRef = useMemo(() => {
    const chs = plan?.tracks[0]?.changes;
    if (!chs) return null;
    const mb = chs.find((c) => c.field === "musicbrainz_recording")?.new;
    if (mb) return { mbid: mb, trackNo: null as number | null };
    const raw = chs.find((c) => c.field === "track_no")?.new ?? "";
    const n = parseInt(raw, 10);
    return { mbid: null as string | null, trackNo: Number.isFinite(n) ? n : null };
  }, [plan]);
  const isMatchedTrack = (mbid: string, trackNo: number | null) =>
    (notNil(matchedRef?.mbid) && mbid !== "" && mbid === matchedRef.mbid) ||
    (isNil(matchedRef?.mbid) && notNil(matchedRef?.trackNo) && trackNo === matchedRef.trackNo);

  /** 单曲模式已选出专辑，或专辑模式有曲目表 → 中间列展示曲目 */
  const showAlbumTracks =
    mode === "album"
      ? Boolean(plan && plan.catalog_tracks.length > 0)
      : Boolean(selectedAlbum && plan && plan.catalog_tracks.length > 0);

  return (
    <div className="scrape-overlay" role="dialog" aria-label="刮削向导">
      <div className="scrape-panel">
        <header className="scrape-head">
          <div>
            <h2>刮削</h2>
            <p className="tertiary">
              整张专辑拉取云端曲目表存入本地 catalog（不改音频）· MusicBrainz · 封面在边栏单独刮取
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
              className={`chip${mode === "album" ? " active" : ""}`}
              onClick={() => switchMode("album")}
            >
              按专辑
            </button>
            <button
              className={`chip${mode === "track" ? " active" : ""}`}
              onClick={() => switchMode("track")}
            >
              按曲名
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
        {Object.keys(sourceErrs).length > 0 && (
          <div className="scrape-src-errs tertiary">
            {Object.entries(sourceErrs).map(([s, e]) => (
              <span key={s}>
                {SOURCE_LABEL[s] ?? s}源：{e}
              </span>
            ))}
          </div>
        )}

        <div className="scrape-body">
          <section className="scrape-col">
            <h3>
              候选{loading && candidates.length > 0 ? "（陆续到达…）" : ""}
              <span className="tertiary scrape-hint-inline"> MB 优先</span>
            </h3>
            <div className="scrape-list">
              {candidates.length === 0 && !loading && (
                <div className="tertiary scrape-empty">
                  {searchIdRef.current > 0
                    ? "无候选。可改关键词后重试（四源聚合，MusicBrainz 限速约 1 次/秒）。"
                    : "搜索后显示四个来源的候选（MB 排最前）"}
                </div>
              )}
              {candidates.map((c, i) => (
                <button
                  key={`${c.source}:${c.id}:${c.release_id}:${i}`}
                  className={`scrape-item cand${
                    selectedCand?.release_id === c.release_id && selectedCand?.source === c.source
                      ? " active"
                      : ""
                  }`}
                  onClick={() => void pickCandidate(c)}
                >
                  <span className="ellipsis">
                    <span className={`scrape-src src-${c.source}`}>
                      {SOURCE_LABEL[c.source] ?? c.source}
                    </span>
                    {c.title}
                  </span>
                  <span className="tertiary ellipsis">
                    {c.artist}
                    {c.year ? ` · ${c.year}` : ""}
                    {c.track_count > 1 ? ` · ${c.track_count} 曲` : ""}
                    {c.country ? ` · ${c.country}` : ""}
                  </span>
                  {c.disambiguation && (
                    <span className="tertiary scrape-disamb">{c.disambiguation}</span>
                  )}
                </button>
              ))}
            </div>
          </section>

          <section className="scrape-col">
            {mode === "track" && (
              <div className="scrape-album-pick">
                <h3>
                  所属专辑
                  {trackAlbums.length > 0 ? `（${trackAlbums.length}）` : ""}
                </h3>
                <div className="scrape-album-list">
                  {!selectedCand && (
                    <div className="tertiary scrape-empty">
                      点左侧单曲候选后，这里列出它收在哪些专辑；点专辑看整张曲目
                    </div>
                  )}
                  {selectedCand && trackAlbums.length === 0 && !loading && (
                    <div className="tertiary scrape-empty">该候选未返回所属专辑信息</div>
                  )}
                  {trackAlbums.map((al) => {
                    const active =
                      selectedAlbum?.release_id === al.release_id &&
                      selectedAlbum?.source === al.source;
                    return (
                      <button
                        key={`${al.source}:${al.release_id}`}
                        className={`scrape-item${active ? " active" : ""}`}
                        disabled={loading && !active}
                        onClick={() => void pickTrackAlbum(al)}
                      >
                        <span className="ellipsis">{al.title || "（未命名专辑）"}</span>
                        <span className="tertiary ellipsis">
                          {[
                            albumTypeLabel(al.release_type),
                            al.year,
                            al.track_count > 0 ? `${al.track_count} 曲` : "",
                            al.country,
                            al.disambiguation,
                          ]
                            .filter(Boolean)
                            .join(" · ")}
                        </span>
                      </button>
                    );
                  })}
                </div>
              </div>
            )}

            <h3>
              {mode === "track" && selectedAlbum ? `「${selectedAlbum.title}」曲目` : "专辑曲目"}
              {showAlbumTracks && albumTracks.length > 0 ? `（${albumTracks.length} 首）` : ""}
            </h3>
            <div className="scrape-list">
              {!showAlbumTracks && (
                <div className="tertiary scrape-empty">
                  {mode === "track"
                    ? "选定所属专辑后，这里显示整张曲目，方便对照挑选"
                    : "点击左侧候选后显示整张曲目；点某一首可手动指定它就是本地曲目"}
                </div>
              )}
              {showAlbumTracks &&
                albumTracks.map((ct) => {
                  const matched = isMatchedTrack(ct.mbid, ct.track_no ?? null);
                  return (
                    <button
                      key={ct.mbid || ct.track_no || ct.title}
                      className={`scrape-item${matched ? " active" : ""}`}
                      disabled={loading || matched}
                      title="点选 = 本地曲目对应这一首"
                      onClick={() => notNil(ct.track_no) && void pickRemoteTrack(ct.track_no)}
                    >
                      <span className="ellipsis">
                        {notNil(ct.track_no) ? `${ct.track_no}. ` : ""}
                        {ct.title}
                      </span>
                      {ct.artist && <span className="tertiary ellipsis">{ct.artist}</span>}
                    </button>
                  );
                })}
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
              {plan && !localPlan && (
                <div className="tertiary scrape-empty">
                  候选「{plan.candidate_label}」没能自动匹配到本地曲目
                  {plan.unmatched.length > 0 ? `（${plan.unmatched.join("、")}）` : ""}
                  {plan.catalog_tracks.length > 1
                    ? "——简繁/译名差异时常见。在中间「专辑曲目」列挑一首即可核对字段；不挑也可以直接「存入本地 catalog」，整张曲目表会备档。"
                    : "。可换候选重试。"}
                </div>
              )}
              {plan && localPlan && (
                <>
                  <div className="scrape-plan-label">
                    采纳对象：
                    <span className={`scrape-src src-${plan.source}`}>
                      {SOURCE_LABEL[plan.source] ?? plan.source}
                    </span>
                    <strong>{plan.candidate_label}</strong>
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
                </>
              )}
            </div>
          </section>
        </div>

        <footer className="scrape-foot">
          {notNil(savedCount) ? (
            <span>
              已存入本地 catalog {savedCount} 条
              {plan && plan.catalog_tracks.length > 1
                ? "（整张专辑曲目表，本地没有的曲目也已备档）"
                : ""}
              。音频文件未改动，可在边栏逐字段写入。
            </span>
          ) : (
            <span className="muted">{footHint(plan, realChanges, changeCount)}</span>
          )}
          <div className="scrape-foot-actions">
            <button className="btn" onClick={onClose}>
              {notNil(savedCount) ? "完成" : "取消"}
            </button>
            <button
              className="btn btn-primary"
              disabled={!plan || applying || notNil(savedCount)}
              title={plan && plan.tracks.length === 0 ? "整张曲目表存入 catalog，不绑定本地曲目" : ""}
              onClick={() => void apply()}
            >
              {applying ? <Loader2 size={15} className="spin" /> : null}
              {notNil(savedCount) ? "已存入 catalog" : "存入本地 catalog"}
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}
