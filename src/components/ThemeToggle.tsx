import { Moon, Sun } from "lucide-react";
import { useEffect, useState } from "react";

import { api } from "../lib/api";
import { applyThemeMode, readThemeMode } from "../lib/colorScheme";
import type { ThemeMode } from "../lib/types";

/** 顶栏深浅色快捷切换（设置页「外观」同一份 theme_mode）。 */
export function ThemeToggle() {
  const [mode, setMode] = useState<ThemeMode>(readThemeMode);

  useEffect(() => {
    const sync = () => {
      setMode(readThemeMode());
    };
    sync();
    const obs = new MutationObserver(sync);
    obs.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });
    return () => obs.disconnect();
  }, []);

  const nextLabel = mode === "light" ? "切换暗色" : "切换浅色";

  return (
    <button
      type="button"
      className="btn theme-toggle"
      title={nextLabel}
      aria-label={nextLabel}
      onClick={() => {
        const next: ThemeMode = mode === "light" ? "dark" : "light";
        applyThemeMode(next);
        void api.updateSettings({ theme_mode: next }).catch(() => {});
      }}
    >
      {mode === "light" ? <Moon size={15} /> : <Sun size={15} />}
    </button>
  );
}
