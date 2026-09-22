/** LRC 解析与定位 — 正在播放页自用，不引第三方库（逻辑很薄，效果要自定义）。 */

export interface LrcLine {
  timeMs: number;
  /** 主句（通常是原文） */
  text: string;
  /** 同一时间戳的第二行 → 译文 */
  trans?: string;
}

const TIME_RE = /\[(\d{1,2}):(\d{2})(?:[.:](\d{1,3}))?\]/g;

/** 解析一行里可能出现的多个时间戳；无时间戳的元信息行忽略。 */
export function parseLrc(raw: string | null | undefined): LrcLine[] {
  if (!raw) return [];
  const out: LrcLine[] = [];
  for (const line of raw.split(/\r?\n/)) {
    const tags = [...line.matchAll(TIME_RE)];
    if (tags.length === 0) continue;
    const text = line.replace(TIME_RE, "").trim();
    for (const t of tags) {
      const mm = Number(t[1]);
      const ss = Number(t[2]);
      const frac = t[3] ?? "0";
      // [mm:ss.xx] → centis；[mm:ss.xxx] → millis
      const fracMs =
        frac.length <= 2 ? Number(frac.padEnd(2, "0")) * 10 : Number(frac.padEnd(3, "0"));
      out.push({ timeMs: mm * 60000 + ss * 1000 + fracMs, text });
    }
  }
  out.sort((a, b) => a.timeMs - b.timeMs || a.text.localeCompare(b.text));
  return mergeBilingual(out);
}

/** 同一时间戳多行 → 主句 + 译文（参考 Apple Music 双语歌词）。 */
function mergeBilingual(lines: LrcLine[]): LrcLine[] {
  const merged: LrcLine[] = [];
  for (const l of lines) {
    const last = merged[merged.length - 1];
    if (last && last.timeMs === l.timeMs && !last.trans && l.text) {
      last.trans = l.text;
      continue;
    }
    merged.push({ ...l });
  }
  return merged;
}

/** 当前行下标；无歌词或未到第一句返回 -1。 */
export function findLrcIndex(lines: LrcLine[], ms: number): number {
  if (lines.length === 0) return -1;
  let lo = 0;
  let hi = lines.length - 1;
  let ans = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (lines[mid].timeMs <= ms) {
      ans = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return ans;
}

/** 优先带时间戳的外挂/内嵌；否则纯文本按行展示（无同步）。 */
export function pickLyrics(embedded: string | null, sidecar: string | null): {
  lines: LrcLine[];
  plain: string[];
  synced: boolean;
} {
  const candidates = [sidecar, embedded];
  for (const c of candidates) {
    if (!c) continue;
    const lines = parseLrc(c);
    if (lines.length > 0) return { lines, plain: [], synced: true };
  }
  const text = embedded || sidecar || "";
  const plain = text
    .split(/\r?\n/)
    .map((s) => s.replace(/\[[\d:.]+\]/g, "").trim())
    .filter((s) => s.length > 0);
  return { lines: [], plain, synced: false };
}
