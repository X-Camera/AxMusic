import { useEffect, useState } from "react";
import { friendlyErr } from "../../lib/errors";
import { Heart, ListPlus, X } from "lucide-react";

import { api } from "../../lib/api";
import type { PlaylistAddItem, PlaylistSummary } from "../../lib/types";
import { useFavorites } from "../../state/useFavorites";
import "./Playlists.css";

/** 「加入歌单」对话框：选现有歌单追加，或新建并加入。 */
export function PlaylistPicker({
  items,
  onClose,
  onAdded,
}: {
  items: PlaylistAddItem[];
  onClose: () => void;
  onAdded: (name: string) => void;
}) {
  const [list, setList] = useState<PlaylistSummary[]>([]);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .playlistList()
      .then(setList)
      .catch(() => setList([]));
  }, []);

  async function addTo(name: string, isFavorites = false) {
    setBusy(true);
    setError(null);
    try {
      await api.playlistAddTracks(name, items);
      if (isFavorites) void useFavorites.getState().reload();
      onAdded(name);
    } catch (e) {
      setError(friendlyErr(e));
    } finally {
      setBusy(false);
    }
  }

  async function createAndAdd() {
    const n = name.trim();
    if (!n) return;
    setBusy(true);
    setError(null);
    try {
      await api.playlistCreate(n, items);
      onAdded(n);
    } catch (e) {
      setError(friendlyErr(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="picker-overlay" role="dialog" aria-label="加入歌单" onClick={onClose}>
      <div className="picker-panel" onClick={(e) => e.stopPropagation()}>
        <div className="picker-head">
          <h2>加入 {items.length} 首到…</h2>
          <button className="mp-icon" title="关闭" onClick={onClose}>
            <X size={15} />
          </button>
        </div>
        <div className="picker-new">
          <input
            value={name}
            placeholder="新建歌单名称"
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void createAndAdd()}
          />
          <button
            className="btn btn-primary"
            disabled={busy || !name.trim()}
            onClick={() => void createAndAdd()}
          >
            <ListPlus size={15} /> 新建并加入
          </button>
        </div>
        {error && <div className="picker-error">{error}</div>}
        <div className="picker-list">
          {list.length === 0 ? (
            <div className="tertiary" style={{ padding: 10 }}>
              还没有歌单，上面输入名称新建。
            </div>
          ) : (
            list.map((p) => (
              <button
                key={p.name}
                className={`picker-item${p.is_favorites ? " favorites" : ""}`}
                disabled={busy}
                onClick={() => void addTo(p.name, p.is_favorites)}
              >
                {p.is_favorites ? (
                  <Heart size={15} className="pl-fav-icon" fill="currentColor" />
                ) : (
                  <ListPlus size={15} className="tertiary" />
                )}
                <span className="ellipsis" style={{ flex: 1 }}>
                  {p.name}
                </span>
                <span className="tertiary mono">{p.track_count} 首</span>
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
