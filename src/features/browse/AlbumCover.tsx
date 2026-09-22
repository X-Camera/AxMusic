import { useEffect, useRef, useState } from "react";

import { loadCover, observeCover, peekCover, unobserveCover } from "../manage/coverCache";

/** 专辑封面：进视口懒加载（coverCache），无封面显示首字母占位。 */
export function AlbumCover({
  path,
  mtime,
  hasCover,
  initial,
  size = "card",
}: {
  path: string | null;
  mtime: number;
  hasCover: boolean;
  initial: string;
  /** card = 墙上 1:1；detail = 详情大图 */
  size?: "card" | "detail";
}) {
  const key = path ? `${path}:${mtime}` : "";
  const [src, setSrc] = useState<string | null>(() => (key ? peekCover(key) ?? null : null));
  const boxRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!hasCover || !path || !key) return;
    const cached = peekCover(key);
    if (cached !== undefined) {
      setSrc(cached);
      return;
    }
    const el = boxRef.current;
    if (!el) return;
    let alive = true;
    observeCover(el, () => {
      void loadCover(key, path).then((url) => {
        if (alive) setSrc(url);
      });
    });
    return () => {
      alive = false;
      unobserveCover(el);
    };
  }, [key, path, hasCover]);

  return (
    <div
      ref={boxRef}
      className={`album-cover ${size}${hasCover && src ? "" : " placeholder"}`}
    >
      {src ? (
        <img src={src} alt="" />
      ) : (
        <span className="album-initial">{initial}</span>
      )}
    </div>
  );
}
