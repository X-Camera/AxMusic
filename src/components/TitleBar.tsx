import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import { useEffect, useState } from "react";

import "./TitleBar.css";

const win = getCurrentWindow();

/** 自绘标题栏：隐藏系统栏后顶替（拖拽/双击最大化由 data-tauri-drag-region 内建处理）。 */
export function TitleBar() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let disposed = false;
    void win.isMaximized().then((v) => !disposed && setMaximized(v));
    const unlisten = win.onResized(() => {
      void win.isMaximized().then((v) => !disposed && setMaximized(v));
    });
    return () => {
      disposed = true;
      void unlisten.then((f) => f());
    };
  }, []);

  return (
    <header className="title-bar" data-tauri-drag-region="deep">
      <div className="tb-brand">AxMusic</div>
      <div className="tb-spacer" />
      <div className="tb-controls">
        <button className="tb-btn" title="最小化" onClick={() => void win.minimize()}>
          <Minus size={15} />
        </button>
        <button
          className="tb-btn"
          title={maximized ? "还原" : "最大化"}
          onClick={() => void win.toggleMaximize()}
        >
          {maximized ? <Copy size={13} /> : <Square size={13} />}
        </button>
        <button className="tb-btn tb-close" title="关闭" onClick={() => void win.close()}>
          <X size={15} />
        </button>
      </div>
    </header>
  );
}
