import {
  Fragment,
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";

import { useLyricsEngine } from "./lyricsEngine/useLyricsEngine";
import type { LrcLine } from "./lrc";

export interface LyricsViewHandle {
  /** seek 后把滚动焦点拉回当前句 */
  resetScroll: () => void;
  activeIdx: number;
}

export interface LyricsViewProps {
  lines: LrcLine[];
  plain: string[];
  synced: boolean;
  /** 播放时钟（含 scrub 预览与轮询间隙外推） */
  getTimeMs: () => number;
  /** 点句跳转（引擎在「未拖动」时回调） */
  onSeekLine: (index: number, ms: number) => void;
  /** lyricsDisplayVars：字号/字体/行距 */
  style?: CSSProperties;
  /** 字号排版变化时重测行高 */
  layoutKey?: string;
  /** 切歌 / 单曲循环重播：整表重建 */
  rebuildKey?: string;
  /** 边栏紧凑排版（字号/行距/渐隐按小容器缩） */
  compact?: boolean;
  /** 距离模糊上限 px；compact 默认 1.2 */
  maxBlur?: number;
  /** 无内容时的占位 */
  empty?: ReactNode;
  className?: string;
}

/**
 * 满窗 / 主页右边栏共用的同步歌词视图。
 * 样式与动效同源（.np-lyrics / .np-line + useLyricsEngine），仅尺寸与墨色可不同。
 */
export const LyricsView = forwardRef<LyricsViewHandle, LyricsViewProps>(
  function LyricsView(
    {
      lines,
      plain,
      synced,
      getTimeMs,
      onSeekLine,
      style,
      layoutKey,
      rebuildKey,
      compact = false,
      maxBlur,
      empty = null,
      className = "",
    },
    ref,
  ) {
    const [flashIdx, setFlashIdx] = useState(-1);
    const flashTimerRef = useRef(0);

    const { containerRef, activeIdx, resetScroll } = useLyricsEngine({
      lines,
      synced,
      getTimeMs,
      onSeekLine: (i, ms) => {
        setFlashIdx(i);
        window.clearTimeout(flashTimerRef.current);
        flashTimerRef.current = window.setTimeout(() => setFlashIdx(-1), 400);
        onSeekLine(i, ms);
      },
      layoutKey,
      rebuildKey,
      maxBlur: maxBlur ?? (compact ? 1.2 : 5),
    });

    useImperativeHandle(ref, () => ({ resetScroll, activeIdx }), [resetScroll, activeIdx]);

    useEffect(
      () => () => {
        window.clearTimeout(flashTimerRef.current);
      },
      [],
    );

    const cls = ["np-lyrics", synced ? "synced" : "", compact ? "compact" : "", className]
      .filter(Boolean)
      .join(" ");

    return (
      <div className={cls} ref={containerRef} style={style}>
        {empty}
        {synced && lines.length > 0 && (
          <Fragment key={rebuildKey}>
            {lines.map((l: LrcLine, i) => {
              const on = i === activeIdx;
              let state = "next";
              if (on) state = "on";
              else if (i < activeIdx) state = "past";
              return (
                <div
                  key={`${l.timeMs}-${i}`}
                  data-i={i}
                  className={["np-line", state, flashIdx === i ? "flash" : ""]
                    .filter(Boolean)
                    .join(" ")}
                >
                  <div className="np-line-inner">
                    <div className="np-main">{l.text || "⋯"}</div>
                    {l.trans ? <div className="np-trans">{l.trans}</div> : null}
                  </div>
                </div>
              );
            })}
          </Fragment>
        )}
        {!synced && plain.length > 0 && (
          <div className="np-lines">
            <div className="np-line-spacer" aria-hidden />
            {plain.map((t, i) => (
              <div key={`p${i}`} className="np-line plain">
                <div className="np-main">{t}</div>
              </div>
            ))}
            <div className="np-line-spacer" aria-hidden />
          </div>
        )}
      </div>
    );
  },
);
