import { ArrowLeft, Search } from "lucide-react";
import type { ReactNode } from "react";

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
    <header className="topbar">
      {onBack && (
        <button className="btn topbar-back" onClick={onBack} title="返回">
          <ArrowLeft size={15} /> 返回
        </button>
      )}
      {title != null && title !== "" && <h1 className="topbar-title">{title}</h1>}
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
      <div className="topbar-actions">{actions}</div>
    </header>
  );
}
