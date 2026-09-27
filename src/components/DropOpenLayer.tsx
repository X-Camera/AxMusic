import { useEffect, useRef, useState } from "react";

import { setupDropOpen } from "../lib/dropOpen";
import "./DropOpenLayer.css";

/**
 * 系统文件拖放层：全窗提示 + 队列区高亮由 body class 驱动。
 * 拖到队列面板追加，其它位置替换队列并播放。
 */
export function DropOpenLayer() {
  const [toast, setToast] = useState<string | null>(null);
  const toastTimer = useRef<number | null>(null);

  useEffect(() => {
    const unlisten = setupDropOpen((msg) => {
      if (toastTimer.current) window.clearTimeout(toastTimer.current);
      if (!msg) {
        setToast(null);
        return;
      }
      setToast(msg);
      toastTimer.current = window.setTimeout(() => setToast(null), 2400);
    });
    return () => {
      if (toastTimer.current) window.clearTimeout(toastTimer.current);
      unlisten();
    };
  }, []);

  return (
    <>
      <div className="drop-open-overlay" aria-hidden="true">
        <div className="drop-open-card">
          <div className="drop-open-title">
            拖入音频即可播放
            <span className="drop-open-mode"> → 替换队列</span>
            <span className="drop-open-mode drop-open-mode--queue"> → 追加到播放列表</span>
          </div>
          <div className="drop-open-sub">放到播放列表区域可追加到队列 · 支持文件夹（仅一层）</div>
        </div>
      </div>
      {/* live region 常驻 DOM，只改文本，读屏才能稳定播报 */}
      <button
        type="button"
        className={`drop-open-toast${toast ? "" : " drop-open-toast--hidden"}`}
        role="status"
        onClick={() => setToast(null)}
      >
        {toast}
      </button>
    </>
  );
}
