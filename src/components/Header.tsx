import type { BackupStatus } from "../types/backup";
import { statusLabelMap } from "../lib/backup-utils";
import { BrandMark } from "./BrandMark";

type HeaderProps = {
  status: BackupStatus;
  statusMessage: string;
  onOptionsClick: () => void;
  optionsOpen: boolean;
};

export function Header({
  status,
  statusMessage,
  onOptionsClick,
  optionsOpen,
}: HeaderProps) {
  return (
    <header className="app-header">
      <div className="header-brand">
        <div className="app-logo">
          <BrandMark />
        </div>
        <div>
          <div className="brand-title">归栈 · 备份</div>
          <div className="brand-subtitle">把重要文件留在手边，也留一条退路</div>
        </div>
      </div>

      <div className="header-right">
        <div className="status-indicator">
          <span className={`status-dot status-${status}`} />
          <span className="status-text">{statusLabelMap[status]}</span>
          <span className="status-hint">{statusMessage}</span>
        </div>

        <button
          type="button"
          className={`header-btn ${optionsOpen ? "active" : ""}`}
          onClick={onOptionsClick}
          title="备份选项设置"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
            <circle cx="12" cy="12" r="3" />
          </svg>
          <span>备份设置</span>
        </button>
      </div>
    </header>
  );
}
