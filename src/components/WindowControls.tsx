import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import { useEffect, useState } from "react";

const win = getCurrentWindow();

/** 窗口最小化 / 最大化 / 关闭（无系统栏时顶栏右侧常驻）。 */
export function WindowControls() {
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
    <div className="window-controls">
      <button className="wc-btn" title="最小化" onClick={() => void win.minimize()}>
        <Minus size={15} />
      </button>
      <button
        className="wc-btn"
        title={maximized ? "还原" : "最大化"}
        onClick={() => void win.toggleMaximize()}
      >
        {maximized ? <Copy size={13} /> : <Square size={13} />}
      </button>
      <button className="wc-btn wc-close" title="关闭" onClick={() => void win.close()}>
        <X size={15} />
      </button>
    </div>
  );
}
