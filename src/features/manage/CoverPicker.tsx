import { useEffect, useState } from "react";
import { isNil, notNil } from "../../lib/nil";
import { friendlyErr } from "../../lib/errors";
import { Loader2, X } from "lucide-react";

import { api } from "../../lib/api";
import type { CoverCandidate } from "../../lib/types";
import "./CoverPicker.css";

const SOURCE_LABEL: Record<string, string> = {
  caa: "CAA",
  itunes: "iTunes",
  netease: "网易云",
  qq: "QQ音乐",
};

/** 「刮取封面」候选对话框：多源搜索结果缩略图，点选一张采纳。 */
export function CoverPicker({
  trackId,
  onClose,
  onApplied,
}: {
  trackId: number;
  onClose: () => void;
  /** 采纳完成，回传封面 data URL */
  onApplied: (coverData: string) => void;
}) {
  const [items, setItems] = useState<CoverCandidate[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [picking, setPicking] = useState<string | null>(null);
  const [broken, setBroken] = useState<Set<string>>(new Set());

  useEffect(() => {
    let cancelled = false;
    api
      .coverSearch(trackId)
      .then((list) => {
        if (!cancelled) setItems(list);
      })
      .catch((e) => {
        if (!cancelled) {
          setError(friendlyErr(e));
          setItems([]);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [trackId]);

  async function pick(c: CoverCandidate) {
    if (picking) return;
    setPicking(c.id);
    setError(null);
    try {
      const dataUrl = await api.coverApply(trackId, c.url);
      onApplied(dataUrl);
    } catch (e) {
      setError(friendlyErr(e));
      setPicking(null);
    }
  }

  return (
    <div className="cpk-overlay" role="dialog" aria-label="刮取封面" onClick={onClose}>
      <div className="cpk-panel" onClick={(e) => e.stopPropagation()}>
        <div className="cpk-head">
          <h2>刮取封面</h2>
          <button className="mp-icon" title="关闭" onClick={onClose}>
            <X size={15} />
          </button>
        </div>
        <p className="tertiary cpk-hint">
          CAA / iTunes / 网易云 / QQ音乐 并发搜索，点选一张采纳。只存到库 covers/，不改音频文件。
        </p>
        {error && <div className="cpk-error">{error}</div>}
        <div className="cpk-body">
          {isNil(items) ? (
            <div className="empty-state">
              <Loader2 size={16} className="spin" /> 四源搜索中…
            </div>
          ) : items.length === 0 ? (
            <div className="empty-state">
              <p className="muted">未搜到封面候选。</p>
              <p className="tertiary">
                可改好文件/ catalog 的专辑名后重试；若全部源都失败，可能是网络问题（CAA
                托管在 archive.org，国内可能需要代理）。
              </p>
            </div>
          ) : (
            <div className="cpk-grid">
              {items.map((c) => {
                const imgBroken = broken.has(c.id);
                return (
                  <div
                    key={c.id}
                    className={`cpk-card${notNil(picking) ? " busy" : ""}`}
                    role="button"
                    tabIndex={0}
                    title={`采纳这张封面（${SOURCE_LABEL[c.source] ?? c.source}）`}
                    onClick={() => void pick(c)}
                    onKeyDown={(e) => e.key === "Enter" && void pick(c)}
                  >
                    {imgBroken ? (
                      <div className="cpk-thumb cpk-thumb-broken tertiary">缩略图无法显示</div>
                    ) : (
                      <img
                        className="cpk-thumb"
                        src={c.thumb_url}
                        alt=""
                        referrerPolicy="no-referrer"
                        onError={() =>
                          setBroken((prev) => new Set(prev).add(c.id))
                        }
                      />
                    )}
                    <div className="cpk-meta">
                      <span className="chip">{SOURCE_LABEL[c.source] ?? c.source}</span>
                      <span className="ellipsis" title={c.title}>
                        {c.title || "—"}
                      </span>
                      <span className="tertiary ellipsis" title={c.artist}>
                        {c.artist || "—"}
                      </span>
                    </div>
                    {picking === c.id && (
                      <span className="cpk-picking">
                        <Loader2 size={16} className="spin" />
                      </span>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
