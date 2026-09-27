import { listen } from "@tauri-apps/api/event";

import { api } from "./api";
import type { AppSettings, ColorScheme, ThemeMode } from "./types";

export type SkinPreview = {
  accent: string;
  /** 实心块用亮版（浅色下避免播放键过深） */
  solid: string;
};

export type ColorSchemeMeta = {
  id: ColorScheme;
  label: string;
  dark: SkinPreview;
  light: SkinPreview;
};

/** 外观 × 皮肤（与 tokens.css 的 data-theme / data-color-scheme 对应） */
export const THEME_MODES: { id: ThemeMode; label: string }[] = [
  { id: "dark", label: "暗色" },
  { id: "light", label: "浅色" },
];

/** 表面中性灰，皮肤只改 accent；色板预览画中性底 + 强调块 */
export const COLOR_SCHEMES: ColorSchemeMeta[] = [
  {
    id: "nebula",
    label: "星云紫",
    dark: { accent: "#7c6af2", solid: "#7c6af2" },
    light: { accent: "#6a58e8", solid: "#7c6af2" },
  },
  {
    id: "sky",
    label: "天空蓝",
    dark: { accent: "#5b9dff", solid: "#5b9dff" },
    light: { accent: "#3b82f6", solid: "#5b9dff" },
  },
  {
    id: "jade",
    label: "翡翠绿",
    dark: { accent: "#34d399", solid: "#34d399" },
    light: { accent: "#10b981", solid: "#34d399" },
  },
  {
    id: "rose",
    label: "玫瑰粉",
    dark: { accent: "#f472b6", solid: "#f472b6" },
    light: { accent: "#db2777", solid: "#f472b6" },
  },
  {
    id: "amber",
    label: "琥珀金",
    dark: { accent: "#f5a524", solid: "#f5a524" },
    light: { accent: "#d97706", solid: "#f5a524" },
  },
  {
    id: "coral",
    label: "珊瑚橙",
    dark: { accent: "#ff7a5c", solid: "#ff7a5c" },
    light: { accent: "#ea580c", solid: "#ff7a5c" },
  },
  {
    id: "graphite",
    label: "石墨灰",
    dark: { accent: "#a8b0be", solid: "#c5cad3" },
    light: { accent: "#5c6578", solid: "#8b93a5" },
  },
];

/** 外观与皮肤的兜底默认（与 Rust `AppSettings::default()` 对齐） */
export const DEFAULT_THEME_MODE: ThemeMode = "light";
export const DEFAULT_COLOR_SCHEME: ColorScheme = "jade";

/** 外观：暗色 / 浅色 */
export function applyThemeMode(mode: ThemeMode | string) {
  document.documentElement.dataset.theme = mode === "light" ? "light" : "dark";
}

/** 皮肤：accent 家族（表面中性，不随皮肤偏色） */
export function applyColorScheme(id: ColorScheme | string) {
  document.documentElement.dataset.colorScheme = id;
}

/** 一次应用外观 + 皮肤 */
export function applyAppearance(theme: ThemeMode | string, scheme: ColorScheme | string) {
  applyThemeMode(theme);
  applyColorScheme(scheme);
}

/** 读取当前 DOM 主题（未设置 data-theme 时按默认浅色） */
export function readThemeMode(): ThemeMode {
  return document.documentElement.dataset.theme === "dark" ? "dark" : "light";
}

/** 启动时拉设置并应用；监听 settings://changed 保持多窗口一致 */
export function bootstrapTheme() {
  void api
    .getSettings()
    .then((s: AppSettings) =>
      applyAppearance(s.theme_mode ?? DEFAULT_THEME_MODE, s.color_scheme ?? DEFAULT_COLOR_SCHEME),
    )
    .catch(() => applyAppearance(DEFAULT_THEME_MODE, DEFAULT_COLOR_SCHEME));

  void listen<AppSettings>("settings://changed", (e) => {
    const s = e.payload;
    applyAppearance(s.theme_mode ?? DEFAULT_THEME_MODE, s.color_scheme ?? DEFAULT_COLOR_SCHEME);
  });
}
