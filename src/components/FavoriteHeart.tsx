import { Heart } from "lucide-react";
import { useEffect } from "react";

import type { PlaylistAddItem } from "../lib/types";
import { favKey, useFavorites } from "../state/useFavorites";
import "./FavoriteHeart.css";

/** 全局可点的心形「喜爱」开关：空心未喜爱，实心已喜爱。 */
export function FavoriteHeart({
  item,
  size = 14,
  className = "",
  onToggle,
}: {
  /** 至少要有 path；title/artist/duration 供加入喜爱时写入歌单 */
  item: PlaylistAddItem;
  size?: number;
  className?: string;
  /** 切换完成（含失败回滚后）回调，favorited = 切换后状态 */
  onToggle?: (favorited: boolean) => void;
}) {
  const keys = useFavorites((s) => s.keys);
  const toggle = useFavorites((s) => s.toggle);
  const ensure = useFavorites((s) => s.ensure);
  const active = keys.has(favKey(item.path));

  useEffect(() => {
    void ensure();
  }, [ensure]);

  return (
    <button
      type="button"
      className={`fav-heart${active ? " on" : ""}${className ? ` ${className}` : ""}`}
      title={active ? "取消喜爱" : "加入喜爱"}
      aria-label={active ? "取消喜爱" : "加入喜爱"}
      aria-pressed={active}
      onClick={(e) => {
        e.stopPropagation();
        e.preventDefault();
        void ensure().then(() =>
          toggle(item)
            .then((fav) => onToggle?.(fav))
            .catch(() => {}),
        );
      }}
    >
      <Heart size={size} fill={active ? "currentColor" : "none"} strokeWidth={active ? 0 : 1.75} />
    </button>
  );
}
