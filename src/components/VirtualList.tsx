import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
  type RefObject,
} from "react";
import { useVirtualizer, type VirtualItem, type Virtualizer } from "@tanstack/react-virtual";

/** 列表行高（含行距）。必须与各列表 CSS 的行高逐像素一致，否则虚拟化错位。 */
export const LIST_ROW_HEIGHT = 38;
/** 播放队列行高。对应 QueuePanel.css `.queue-panel-item-wrap` */
export const QUEUE_ROW_HEIGHT = 40;
/** 管理表行高。必须与 tokens.css `--table-row-height` 一致 */
export const TABLE_ROW_HEIGHT = 40;
export const DEFAULT_OVERSCAN = 12;

/**
 * 测量列表容器相对滚动元素的纵向偏移（列表不在滚动区顶部时传给 virtual-core 的
 * `scrollMargin`，否则可见窗口会系统性下移）。
 */
function useScrollMargin(
  listRef: RefObject<HTMLElement | null>,
  getScrollElement: () => HTMLElement | null,
): number {
  const [margin, setMargin] = useState(0);
  const measure = useCallback(() => {
    const el = listRef.current;
    const scroller = getScrollElement();
    if (!el || !scroller) return;
    const next = Math.max(
      0,
      Math.round(
        el.getBoundingClientRect().top -
          scroller.getBoundingClientRect().top +
          scroller.scrollTop,
      ),
    );
    setMargin((prev) => (prev === next ? prev : next));
  }, [getScrollElement]);

  useLayoutEffect(() => {
    measure();
    const scroller = getScrollElement();
    if (!scroller) return;
    const ro = new ResizeObserver(measure);
    ro.observe(scroller);
    if (listRef.current) ro.observe(listRef.current);
    return () => ro.disconnect();
  }, [measure, getScrollElement, listRef]);

  return margin;
}

/**
 * 固定行高虚拟列表：只渲染视口 ± overscan 行。
 * 滚动容器由调用方提供（通常是 .page-scroll / 面板 scroll 区），
 * 本组件自身只占 totalSize 高度并绝对定位行。
 *
 * `rowHeight` 必须与 CSS 渲染行高逐像素一致（不测量、不校验）。
 */
export function VirtualList<T>({
  items,
  rowHeight,
  renderRow,
  getScrollElement,
  getItemKey,
  overscan = DEFAULT_OVERSCAN,
  className,
  style,
}: {
  items: readonly T[];
  /** 行高（含行间距），固定值；必须与 CSS 行高一致 */
  rowHeight: number;
  renderRow: (item: T, index: number) => ReactNode;
  getScrollElement: () => HTMLElement | null;
  /** 稳定行身份（过滤/删除/排序时避免 DOM 错位）；缺省用下标 */
  getItemKey?: (item: T, index: number) => string | number;
  overscan?: number;
  className?: string;
  style?: CSSProperties;
}) {
  const listRef = useRef<HTMLDivElement>(null);
  const scrollMargin = useScrollMargin(listRef, getScrollElement);
  const itemsRef = useRef(items);
  itemsRef.current = items;
  const keyFn = getItemKey;

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement,
    estimateSize: () => rowHeight,
    overscan,
    scrollMargin,
    getItemKey: (index) =>
      keyFn
        ? keyFn(itemsRef.current[index] as T, index)
        : index,
  });

  // estimateSize 只进首次缓存；rowHeight 变化（网格量出列宽、窗口缩放）必须作废重估，
  // 否则旧高度会让下一行盖住本行文案（卡片标题丢失）。布局前重估，避免闪一帧错位。
  useLayoutEffect(() => {
    virtualizer.measure();
  }, [rowHeight, virtualizer]);

  const total = virtualizer.getTotalSize();
  const vis = virtualizer.getVirtualItems();
  // vi.start 含 scrollMargin，定位时扣掉（容器自身已在该偏移处）
  const origin = scrollMargin;

  return (
    <div
      ref={listRef}
      className={className}
      style={{
        height: total,
        position: "relative",
        // 自成一层，保证 sticky 表头（更高 z-index）始终盖住滚动行
        zIndex: 0,
        ...style,
      }}
    >
      {vis.map((vi) => {
        const item = items[vi.index];
        if (item === undefined) return null;
        return (
          <div
            key={vi.key}
            data-index={vi.index}
            style={{
              position: "absolute",
              top: 0,
              left: 0,
              width: "100%",
              height: vi.size,
              transform: `translateY(${vi.start - origin}px)`,
            }}
          >
            {renderRow(item, vi.index)}
          </div>
        );
      })}
    </div>
  );
}

/** 表格 tbody 虚拟化：首尾 spacer 行撑开高度，只渲染可见 tr */
export function useTableVirtualizer({
  count,
  rowHeight,
  scrollRef,
  scrollMargin = 0,
  overscan = DEFAULT_OVERSCAN,
}: {
  count: number;
  rowHeight: number;
  scrollRef: RefObject<HTMLElement | null>;
  /** tbody 相对滚动容器的偏移（含 sticky 表头高度）；调用方测量 */
  scrollMargin?: number;
  overscan?: number;
}): {
  virtualizer: Virtualizer<HTMLElement, Element>;
  vis: VirtualItem[];
  paddingTop: number;
  paddingBottom: number;
} {
  const virtualizer = useVirtualizer({
    count,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan,
    scrollMargin,
    getItemKey: (index) => index,
  });
  const vis = virtualizer.getVirtualItems();
  const total = virtualizer.getTotalSize();
  // vi.start/end 含 scrollMargin；spacer 在 tbody 内，需扣掉该偏移
  const firstStart = vis.length > 0 ? vis[0].start - scrollMargin : 0;
  const lastEnd = vis.length > 0 ? vis[vis.length - 1].end - scrollMargin : 0;
  const paddingTop = Math.max(0, firstStart);
  const paddingBottom = Math.max(0, total - lastEnd);
  return { virtualizer, vis, paddingTop, paddingBottom };
}
