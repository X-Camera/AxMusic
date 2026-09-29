import { useEffect, useState } from "react";

import { friendlyErr } from "./errors";
import "./dialog.css";

type PromptReq = {
  kind: "prompt" | "alert" | "confirm";
  title: string;
  defaultValue?: string;
  message?: string;
  resolve: (v: string | null | boolean) => void;
};

let pending: PromptReq | null = null;
let notify: (() => void) | null = null;

function push(req: PromptReq) {
  // 已有未关闭请求：先取消旧的，避免 Promise 永久悬挂
  if (pending) {
    const prev = pending;
    pending = null;
    prev.resolve(prev.kind === "confirm" ? false : prev.kind === "prompt" ? null : null);
  }
  pending = req;
  notify?.();
}

/** Tauri WebView 的 window.prompt/alert 不可靠/阻塞：统一走自绘对话框。 */
export function promptText(title: string, defaultValue = ""): Promise<string | null> {
  return new Promise((resolve) => {
    push({
      kind: "prompt",
      title,
      defaultValue,
      resolve: (v) => resolve(typeof v === "string" ? v : null),
    });
  });
}

export function alertText(message: string): Promise<void> {
  return new Promise((resolve) => {
    push({
      kind: "alert",
      title: "提示",
      message,
      resolve: () => resolve(),
    });
  });
}

export function confirmText(title: string, message: string): Promise<boolean> {
  return new Promise((resolve) => {
    push({ kind: "confirm", title, message, resolve: (v) => resolve(v === true) });
  });
}

/** 错误 → 中文提示对话框。 */
export function alertError(e: unknown): Promise<void> {
  return alertText(friendlyErr(e));
}

/** 挂在 App 根部，一次只处理一个请求。 */
export function DialogHost() {
  const [req, setReq] = useState<PromptReq | null>(pending);
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    notify = () => setReq(pending);
    return () => {
      notify = null;
    };
  }, []);

  useEffect(() => {
    if (req?.kind === "prompt") setText(req.defaultValue ?? "");
    setError(null);
  }, [req]);

  if (!req) return null;

  const close = (v: string | null | boolean) => {
    const r = pending;
    pending = null;
    setReq(null);
    r?.resolve(v);
  };

  return (
    <div className="dialog-overlay" role="presentation" onClick={() => close(req.kind === "confirm" ? false : null)}>
      <div
        className="dialog-card"
        role="dialog"
        aria-modal="true"
        aria-label={req.title}
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className="dialog-title">{req.title}</h2>
        {req.kind === "prompt" ? (
          <>
            <input
              className="dialog-input"
              value={text}
              autoFocus
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  const v = text.trim();
                  if (!v) {
                    setError("名称不能为空");
                    return;
                  }
                  close(v);
                }
                if (e.key === "Escape") close(null);
              }}
            />
            {error && <p className="dialog-error">{error}</p>}
          </>
        ) : (
          <p className="dialog-message">{req.message}</p>
        )}
        <div className="dialog-actions">
          {req.kind !== "alert" && (
            <button type="button" className="dialog-btn" onClick={() => close(req.kind === "confirm" ? false : null)}>
              取消
            </button>
          )}
          <button
            type="button"
            className="dialog-btn primary"
            autoFocus={req.kind !== "prompt"}
            onClick={() => {
              if (req.kind === "prompt") {
                const v = text.trim();
                if (!v) {
                  setError("名称不能为空");
                  return;
                }
                close(v);
              } else if (req.kind === "confirm") {
                close(true);
              } else {
                close(null);
              }
            }}
          >
            {req.kind === "alert" ? "知道了" : "确定"}
          </button>
        </div>
      </div>
    </div>
  );
}
