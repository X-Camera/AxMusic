import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { api } from "../lib/api";
import "./CloseDialog.css";

/** 主窗口点关闭且设置为「询问」时的退出 / 托盘选择。 */
export function CloseDialog() {
  const [open, setOpen] = useState(false);
  const [remember, setRemember] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen("app://close-requested", () => setOpen(true)).then((f) => {
      if (cancelled) f();
      else unlisten = f;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  async function choose(action: "tray" | "exit") {
    setBusy(true);
    try {
      await api.resolveWindowClose(action, remember);
      setOpen(false);
    } catch {
      setOpen(false);
    } finally {
      setBusy(false);
    }
  }

  if (!open) return null;

  return (
    <div
      className="close-overlay"
      role="dialog"
      aria-modal="true"
      aria-label="关闭 AxMusic"
      onClick={() => setOpen(false)}
    >
      <div className="close-panel" onClick={(e) => e.stopPropagation()}>
        <h2 className="close-title">关闭 AxMusic？</h2>
        <p className="close-desc muted">
          缩到托盘可随时从任务栏托盘图标回到播放器；退出会结束当前播放。
        </p>
        <label className="close-remember">
          <input
            type="checkbox"
            checked={remember}
            onChange={(e) => setRemember(e.target.checked)}
          />
          <span>下次不再询问，按本次选择执行</span>
        </label>
        <div className="close-actions">
          <button
            className="btn"
            disabled={busy}
            onClick={() => void choose("exit")}
          >
            退出程序
          </button>
          <button
            className="btn btn-primary"
            disabled={busy}
            onClick={() => void choose("tray")}
          >
            缩到托盘
          </button>
        </div>
      </div>
    </div>
  );
}
