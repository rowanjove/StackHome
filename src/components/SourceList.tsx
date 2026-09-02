import type { BackupItem, BackupItemCategory } from "../types/backup";
import { categoryMetaMap, formatBytes } from "../lib/backup-utils";

type SourceListProps = {
  items: BackupItem[];
  disabled: boolean;
  onToggleItem: (id: string) => void;
  onToggleCategory: (category: string, enabled: boolean) => void;
  onToggleAll: (enabled: boolean) => void;
  onAddCustomFolder: () => void;
  onRemoveCustomItem: (id: string) => void;
  onRenameCustomItem: (id: string, nextLabel: string) => void;
  onClearCustomItems: () => void;
};

const categoryOrder: BackupItemCategory[] = ["system", "app", "dev", "custom"];

function getCategoryIcon(cat: BackupItemCategory | string) {
  switch (cat) {
    case "system":
      return (
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
        </svg>
      );
    case "app":
      return (
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <rect x="3" y="3" width="7" height="7" rx="1" />
          <rect x="14" y="3" width="7" height="7" rx="1" />
          <rect x="14" y="14" width="7" height="7" rx="1" />
          <rect x="3" y="14" width="7" height="7" rx="1" />
        </svg>
      );
    case "dev":
      return (
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <polyline points="16 18 22 12 16 6" />
          <polyline points="8 6 2 12 8 18" />
        </svg>
      );
    default:
      return (
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M12 5v14M5 12h14" />
        </svg>
      );
  }
}

export function SourceList({
  items,
  disabled,
  onToggleItem,
  onToggleCategory,
  onToggleAll,
  onAddCustomFolder,
  onRemoveCustomItem,
  onRenameCustomItem,
  onClearCustomItems,
}: SourceListProps) {
  const selectedCount = items.filter((item) => item.enabled).length;
  const allSelected = selectedCount === items.length && items.length > 0;
  const customItems = items.filter((item) => item.isCustom);

  return (
    <section className="panel source-panel">
      <div className="panel-header">
        <div>
          <h2>选择备份项目</h2>
          <p className="panel-desc">勾选您需要备份的个人文件、软件数据与开发配置</p>
        </div>

        <div className="panel-toolbar">
          <button
            type="button"
            className="btn-ghost"
            onClick={() => onToggleAll(!allSelected)}
            disabled={disabled || items.length === 0}
          >
            {allSelected ? "取消全选" : "全选"}
          </button>
          <button
            type="button"
            className="btn-secondary"
            onClick={onAddCustomFolder}
            disabled={disabled}
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="12" y1="5" x2="12" y2="19" />
              <line x1="5" y1="12" x2="19" y2="12" />
            </svg>
            <span>添加文件夹</span>
          </button>
        </div>
      </div>

      <div className="category-groups">
        {categoryOrder.map((categoryKey) => {
          const categoryItems = items.filter(
            (item) => (item.category || (item.isCustom ? "custom" : "system")) === categoryKey,
          );

          if (categoryItems.length === 0 && categoryKey !== "custom") {
            return null;
          }

          const meta = categoryMetaMap[categoryKey];
          const catSelectedCount = categoryItems.filter((item) => item.enabled).length;
          const catAllSelected = catSelectedCount === categoryItems.length && categoryItems.length > 0;

          return (
            <div className="category-group" key={categoryKey}>
              <div className="category-header">
                <div className="category-title-wrap">
                  <span className="category-icon">{getCategoryIcon(categoryKey)}</span>
                  <div>
                    <div className="category-title">
                      <span>{meta?.label || categoryKey}</span>
                      <span className="badge-count">
                        {catSelectedCount} / {categoryItems.length}
                      </span>
                    </div>
                    <div className="category-desc">{meta?.description}</div>
                  </div>
                </div>

                <div className="category-actions">
                  {categoryItems.length > 0 ? (
                    <button
                      type="button"
                      className="btn-link"
                      onClick={() => onToggleCategory(categoryKey, !catAllSelected)}
                      disabled={disabled}
                    >
                      {catAllSelected ? "取消本组" : "全选本组"}
                    </button>
                  ) : null}

                  {categoryKey === "custom" && customItems.length > 0 ? (
                    <button
                      type="button"
                      className="btn-link text-danger"
                      onClick={onClearCustomItems}
                      disabled={disabled}
                    >
                      清空自定义
                    </button>
                  ) : null}
                </div>
              </div>

              {categoryItems.length > 0 ? (
                <div className="item-card-grid">
                  {categoryItems.map((item) => (
                    <label
                      key={item.id}
                      className={`source-card ${item.enabled ? "checked" : ""} ${
                        disabled ? "disabled" : ""
                      }`}
                    >
                      <div className="card-left">
                        <input
                          type="checkbox"
                          className="custom-checkbox"
                          checked={item.enabled}
                          disabled={disabled}
                          onChange={() => onToggleItem(item.id)}
                        />

                        <div className="card-info">
                          <div className="card-title-row">
                            {item.isCustom ? (
                              <input
                                type="text"
                                className="inline-edit-input"
                                value={item.label}
                                disabled={disabled}
                                onChange={(e) => onRenameCustomItem(item.id, e.target.value)}
                                onClick={(e) => e.stopPropagation()}
                                placeholder="自定义名称"
                              />
                            ) : (
                              <span className="item-label">{item.label}</span>
                            )}

                            {item.isCustom ? (
                              <button
                                type="button"
                                className="remove-btn"
                                title="移除此自定义项"
                                aria-label={`移除自定义备份项 ${item.label || item.sourcePath}`}
                                disabled={disabled}
                                onClick={(e) => {
                                  e.preventDefault();
                                  e.stopPropagation();
                                  onRemoveCustomItem(item.id);
                                }}
                              >
                                ×
                              </button>
                            ) : null}
                          </div>

                          {item.description ? (
                            <div className="item-desc">{item.description}</div>
                          ) : null}
                          <div className="item-path" title={item.sourcePath}>
                            {item.sourcePath}
                          </div>
                        </div>
                      </div>

                      <div className="card-stats">
                        {item.fileCount != null ? (
                          <span className="stat-files">{item.fileCount} 个文件</span>
                        ) : null}
                        {item.totalSize != null ? (
                          <span className="stat-size">{formatBytes(item.totalSize)}</span>
                        ) : null}
                      </div>
                    </label>
                  ))}
                </div>
              ) : categoryKey === "custom" ? (
                <button type="button" className="custom-empty-hint" onClick={onAddCustomFolder} disabled={disabled} aria-label="添加自定义备份文件夹">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <line x1="12" y1="5" x2="12" y2="19" />
                    <line x1="5" y1="12" x2="19" y2="12" />
                  </svg>
                  <span>点击添加其他个人目录或项目文件夹</span>
                </button>
              ) : null}
            </div>
          );
        })}
      </div>
    </section>
  );
}
