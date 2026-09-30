import { ArrowLeft, PictureInPicture2, Search } from "lucide-react";
import { notNil } from "../lib/nil";
import type { ReactNode } from "react";

import { enterMiniMode, exitMiniMode } from "../lib/miniMode";
import { useApp } from "../state/useApp";
import { ThemeToggle } from "./ThemeToggle";
import { WindowControls } from "./WindowControls";
import "./TopBar.css";

export function TopBar({
  title,
  actions,
  searchValue,
  onSearch,
  onBack,
}: {
  /** 详情页可省略（标题已在内容区展示） */
  title?: string;
  actions?: ReactNode;
  searchValue?: string;
  onSearch?: (v: string) => void;
  /** 有则在左上角显示「返回」 */
  onBack?: () => void;
}) {
  return (
    <header className="topbar" data-tauri-drag-region="deep">
      {onBack && (
        <button className="btn topbar-back" onClick={onBack} title="返回">
          <ArrowLeft size={15} /> 返回
        </button>
      )}
      {notNil(title) && title !== "" && <h1 className="topbar-title">{title}</h1>}
      {onSearch && (
        <div className="topbar-search">
          <Search size={14} className="tertiary" />
          <input
            placeholder="搜索曲目、歌手、专辑"
            value={searchValue ?? ""}
            onChange={(e) => onSearch(e.target.value)}
          />
        </div>
      )}
      <div className="topbar-actions">
        {actions}
        <button
          type="button"
          className="btn topbar-icon"
          title="迷你模式"
          aria-label="迷你模式"
          onClick={() => {
            const s = useApp.getState();
            s.setFullPlayer(false);
            s.setMiniMode(true);
            void enterMiniMode().catch(() => {
              // 几何切换失败：还原窗口参数并回滚 UI，避免小卡拉伸占满整窗
              void exitMiniMode()
                .catch(() => undefined)
                .finally(() => useApp.getState().setMiniMode(false));
            });
          }}
        >
          <PictureInPicture2 size={15} />
        </button>
        <ThemeToggle />
      </div>
      <WindowControls />
    </header>
  );
}
