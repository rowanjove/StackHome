import type { WorkspacePage } from "../types/workspace";
import { BrandMark } from "./BrandMark";

type SidebarProps = {
  page: WorkspacePage;
  onNavigate: (page: WorkspacePage) => void;
};

function NavIcon({ kind }: { kind: string }) {
  const path = {
    recent: "M4 5h16M4 12h16M4 19h10",
    files: "M4 5h16v14H4zM8 9h8M8 13h8",
    organizer: "M5 5h14v14H5zM8 9h8M8 13h5",
    duplicates: "M8 8h11v11H8zM5 5h11v3M5 5v11h3",
    backup: "M12 4v11M8 11l4 4 4-4M5 19h14",
    restore: "M5 12a7 7 0 1 0 2-5M5 5v5h5",
    rules: "M5 6h14M5 12h14M5 18h14M8 6v0M12 12v0M16 18v0",
    history: "M4 12a8 8 0 1 0 2-5M4 5v5h5M12 8v5l3 2",
    settings: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8",
  }[kind as WorkspacePage] ?? "M5 12h14";

  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
      <path d={path} />
    </svg>
  );
}

export function Sidebar({ page, onNavigate }: SidebarProps) {
  const primary: Array<[WorkspacePage, string]> = [
    ["recent", "最近"],
    ["files", "文件"],
    ["organizer", "整理"],
    ["duplicates", "重复项"],
  ];
  const protection: Array<[WorkspacePage, string]> = [
    ["backup", "备份"],
    ["restore", "恢复"],
  ];
  const governance: Array<[WorkspacePage, string]> = [
    ["rules", "规则"],
    ["history", "历史"],
  ];

  function renderItems(items: Array<[WorkspacePage, string]>) {
    return items.map(([key, label]) => (
      <button
        type="button"
        key={key}
        className={`nav-item ${page === key ? "active" : ""}`}
        aria-current={page === key ? "page" : undefined}
        onClick={() => onNavigate(key)}
      >
        <NavIcon kind={key} />
        <span>{label}</span>
      </button>
    ));
  }

  return (
    <aside className="sidebar" aria-label="主导航">
      <div className="sidebar-brand">
        <div className="sidebar-mark"><BrandMark title="StackHome 归栈" /></div>
        <div>
          <div className="sidebar-title">StackHome</div>
          <div className="sidebar-caption">归栈 · 本地文件工作台</div>
        </div>
        <span className="sidebar-edition">LOCAL</span>
      </div>
      <nav className="sidebar-nav">
        <div className="nav-section-label">工作区</div>
        {renderItems(primary)}
        <div className="nav-section-label">保护</div>
        {renderItems(protection)}
        <div className="nav-section-label">治理</div>
        {renderItems(governance)}
      </nav>
      <div className="sidebar-footer">
        {renderItems([["settings", "设置"]])}
        <div className="local-first-note">
          <span className="local-first-glyph">⌁</span>
          <span>文件不离开这台电脑</span>
        </div>
      </div>
    </aside>
  );
}
