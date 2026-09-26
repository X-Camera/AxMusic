import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

/**
 * 全局右键菜单：portal 到 body，渲染后量尺寸夹进窗口（右/下缘各留 8px），
 * 避免在窗口边缘右键时菜单越界被截断。
 * 默认主题跟随；onCover 用于满窗播放等封面叠层场景（始终深色玻璃，不跟主题）。
 */
export function ContextMenu({
  x,
  y,
  onCover,
  children,
}: {
  x: number;
  y: number;
  onCover?: boolean;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  /** null = 尚未测量（先隐身渲染，避免在鼠标点闪一下再跳走） */
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({
      left: Math.max(8, Math.min(x, window.innerWidth - r.width - 8)),
      top: Math.max(8, Math.min(y, window.innerHeight - r.height - 8)),
    });
  }, [x, y]);

  return createPortal(
    <div
      ref={ref}
      className={`np-ctx-menu${onCover ? " on-cover" : ""}`}
      style={{
        left: pos?.left ?? x,
        top: pos?.top ?? y,
        visibility: pos ? "visible" : "hidden",
      }}
      role="menu"
      onClick={(e) => e.stopPropagation()}
    >
      {children}
    </div>,
    document.body,
  );
}
