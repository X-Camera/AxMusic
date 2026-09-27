import type { ReplayGainInfo, ReplayGainSource } from "../lib/types";
import "./ReplayGainBadge.css";

/** 判零阈值：显示为 0 dB / 不渲染共用，避免口径漂移 */
const DB_EPSILON = 0.05;

const SOURCE_LABEL: Record<ReplayGainSource, string> = {
  none: "无标签",
  track: "曲目标签",
  album: "专辑标签",
};

/** 格式化带符号 dB：+2.3 dB / -1.8 dB / 0 dB（ASCII 负号，便于复制与日志） */
function formatDb(db: number): string {
  const rounded = Math.round(db * 10) / 10;
  if (Math.abs(rounded) < DB_EPSILON) return "0 dB";
  const sign = rounded > 0 ? "+" : "-";
  return `${sign}${Math.abs(rounded).toFixed(1)} dB`;
}

function tooltipFor(rg: ReplayGainInfo): string {
  const lines: string[] = [];
  lines.push(`响度均衡 ${formatDb(rg.applied_gain_db)}`);
  const source = SOURCE_LABEL[rg.source];
  if (rg.peak_limited) {
    lines.push(
      `来源：${source} · 原 ${formatDb(rg.requested_gain_db)}，峰值限幅防削波`,
    );
  } else {
    lines.push(`来源：${source}`);
  }
  if (rg.applied_gain_db > 0) {
    lines.push("本曲偏小，已调高音量与其他歌曲拉平。");
  } else {
    lines.push("本曲偏大，已调低音量与其他歌曲拉平。");
  }
  lines.push("设置 → 播放 → 响度均衡 可关闭或切换曲目/专辑。");
  return lines.join("\n");
}

/**
 * 响度均衡标识：有实际增益补偿时显示带符号 dB，悬浮给出说明。
 * 未启用 / 无标签 / 增益约 0 时不渲染。
 */
export function ReplayGainBadge({
  info,
  size = "sm",
}: {
  info: ReplayGainInfo | null | undefined;
  size?: "sm" | "md";
}) {
  if (!info?.active) return null;
  const db = info.applied_gain_db;
  const rounded = Math.round(db * 10) / 10;
  // 0 dB 不必占地方；真有调高/调低才亮标识（与 formatDb 同一口径）
  if (Math.abs(rounded) < DB_EPSILON) return null;
  const sign = rounded > 0 ? "up" : "down";
  return (
    <span
      className={`rg-badge rg-badge-${sign}${size === "md" ? " rg-badge-md" : ""}`}
      title={tooltipFor(info)}
      aria-label={`响度均衡 ${formatDb(db)}`}
    >
      {formatDb(db)}
    </span>
  );
}
