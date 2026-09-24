import type { CSSProperties } from "react";

import type { LyricsFont } from "./types";

/** 满窗歌词字体选项（Windows 常见中文字体，label 中文短名） */
export const LYRICS_FONTS: { id: LyricsFont; label: string; stack: string }[] = [
  { id: "display", label: "默认", stack: "var(--font-display)" },
  {
    id: "yahei",
    label: "微软雅黑",
    stack: '"Microsoft YaHei UI", "Microsoft YaHei", sans-serif',
  },
  {
    id: "dengxian",
    label: "等线",
    stack: '"DengXian", "等线", "Microsoft YaHei", sans-serif',
  },
  { id: "kaiti", label: "楷体", stack: '"KaiTi", "楷体", "STKaiti", serif' },
  { id: "songti", label: "宋体", stack: '"SimSun", "宋体", serif' },
  { id: "heiti", label: "黑体", stack: '"SimHei", "黑体", "Microsoft YaHei", sans-serif' },
];

export function lyricsFontStack(font: LyricsFont | undefined): string {
  return LYRICS_FONTS.find((f) => f.id === font)?.stack ?? LYRICS_FONTS[0].stack;
}

/** 满窗歌词显示参数 → CSS 自定义属性（挂在 .np-lyrics） */
export function lyricsDisplayVars(opts: {
  fontScale: number;
  font: LyricsFont;
  lineHeight: number;
}): CSSProperties {
  return {
    ["--lyrics-scale" as string]: String(opts.fontScale),
    ["--lyrics-lh" as string]: String(opts.lineHeight),
    ["--lyrics-font" as string]: lyricsFontStack(opts.font),
  } as CSSProperties;
}
