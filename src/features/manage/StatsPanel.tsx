import type { LibraryStats } from "../../lib/types";
import "./StatsPanel.css";

/** 右栏常驻面板：未选中曲目时显示库统计（选中后切换为 文件 vs catalog 对比）。 */
export function StatsPanel({ stats }: { stats: LibraryStats | null }) {
  const pct =
    stats && stats.total_tracks > 0
      ? Math.round((stats.linked_tracks / stats.total_tracks) * 100)
      : 0;

  return (
    <aside className="stats-panel" aria-label="库统计">
      <header className="stats-head">
        <h2>库统计</h2>
      </header>
      {!stats ? (
        <div className="tertiary">加载中…</div>
      ) : (
        <>
          <section className="stats-block">
            <div className="stats-block-title">刮削进度</div>
            <div className="stats-big">
              <span className="mono stats-accent">{stats.linked_tracks}</span>
              <span className="tertiary"> / {stats.total_tracks} 首已关联</span>
            </div>
            <div className="stats-bar">
              <div className="stats-bar-fill" style={{ width: `${pct}%` }} />
            </div>
            <div className="tertiary stats-note">表中绿色字段 = 与 catalog 一致</div>
          </section>

          <section className="stats-block">
            <div className="stats-block-title">本地 catalog</div>
            <div className="stats-grid">
              <div className="stat-card">
                <div className="stat-num mono">{stats.catalog_albums}</div>
                <div className="stat-label">专辑</div>
              </div>
              <div className="stat-card">
                <div className="stat-num mono">{stats.catalog_artists}</div>
                <div className="stat-label">歌手</div>
              </div>
              <div className="stat-card">
                <div className="stat-num mono">{stats.catalog_tracks}</div>
                <div className="stat-label">曲目</div>
              </div>
            </div>
          </section>

          <section className="stats-block">
            <div className="stats-block-title">文件标签</div>
            <ul className="stats-list">
              <li>
                <span>库中曲目</span>
                <span className="mono">{stats.total_tracks}</span>
              </li>
              <li>
                <span>有封面</span>
                <span className="mono">{stats.with_cover}</span>
              </li>
              <li>
                <span>外挂歌词</span>
                <span className="mono">{stats.with_lrc}</span>
              </li>
              <li>
                <span>内嵌歌词</span>
                <span className="mono">{stats.with_lyrics}</span>
              </li>
            </ul>
          </section>
        </>
      )}
    </aside>
  );
}
