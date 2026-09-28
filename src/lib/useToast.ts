import { useCallback, useEffect, useRef, useState } from "react";

/**
 * 轻提示：连续弹出时清掉上一定时器（避免提前消失），卸载时清理（避免 setState 警告）。
 */
export function useToast(durationMs = 2000) {
  const [toast, setToast] = useState<string | null>(null);
  const timerRef = useRef(0);

  useEffect(() => () => window.clearTimeout(timerRef.current), []);

  const showToast = useCallback(
    (msg: string) => {
      window.clearTimeout(timerRef.current);
      setToast(msg);
      timerRef.current = window.setTimeout(() => setToast(null), durationMs);
    },
    [durationMs],
  );

  const clearToast = useCallback(() => {
    window.clearTimeout(timerRef.current);
    setToast(null);
  }, []);

  return { toast, showToast, clearToast };
}
