import { Search } from "lucide-react";
import type { ReactNode } from "react";

import "./TopBar.css";

export function TopBar({
  title,
  actions,
  searchValue,
  onSearch,
}: {
  title: string;
  actions?: ReactNode;
  searchValue?: string;
  onSearch?: (v: string) => void;
}) {
  return (
    <header className="topbar">
      <h1 className="topbar-title">{title}</h1>
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
