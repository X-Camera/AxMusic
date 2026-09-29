/** 后端/异常 → 用户可读中文提示。原始栈不进 UI。 */

function asMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message || String(e);
  if (e && typeof e === "object" && "message" in e) {
    const m = (e as { message?: unknown }).message;
    if (typeof m === "string" && m) return m;
  }
  try {
    return String(e);
  } catch {
    return "未知错误";
  }
}

/** 已知后端中文错误原样展示；异常/英文映射为可操作中文。 */
export function friendlyErr(e: unknown): string {
  const msg = asMessage(e).trim();
  if (!msg) return "操作失败";
  // Rust 侧业务错误已是中文短句，直接用
  if (/[一-鿿]/.test(msg)) return msg;

  const lower = msg.toLowerCase();
  if (lower.includes("failed to fetch") || lower.includes("networkerror")) {
    return "网络不可用，请检查网络后重试";
  }
  if (lower.includes("timeout") || lower.includes("timed out")) {
    return "请求超时，请稍后重试";
  }
  if (lower.includes("not found") || lower.includes("enoent")) {
    return "文件或资源不存在";
  }
  if (lower.includes("permission") || lower.includes("access is denied") || lower.includes("eacces")) {
    return "没有权限，请检查文件占用或目录权限";
  }
  if (lower.includes("invoke") || lower.includes("command")) {
    return "内部调用失败，请重试";
  }
  // 英文长句截断，避免刷屏
  return msg.length > 120 ? `${msg.slice(0, 120)}…` : msg;
}

/** 静默吞掉的异步错误至少打日志，避免 unhandled rejection。 */
export function logAsyncError(context: string): (e: unknown) => void {
  return (e: unknown) => {
    console.error(`[${context}]`, e);
  };
}
