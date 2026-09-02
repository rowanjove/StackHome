import { useEffect, useRef } from "react";
import type { BackupOptions } from "../types/backup";
import { defaultBuiltInRules } from "../lib/backup-utils";

type OptionsPanelProps = {
  isOpen: boolean;
  options: BackupOptions;
  customExcludeText: string;
  derivedOptions: BackupOptions;
  disabled: boolean;
  onClose: () => void;
  onToggleOption: <K extends keyof BackupOptions>(key: K) => void;
  onCustomTextChange: (text: string) => void;
  onRemovePattern: (pattern: string) => void;
  onOptionsChange: (newOptions: BackupOptions) => void;
};

export function OptionsPanel({
  isOpen,
  options,
  customExcludeText,
  derivedOptions,
  disabled,
  onClose,
  onToggleOption,
  onCustomTextChange,
  onRemovePattern,
  onOptionsChange,
}: OptionsPanelProps) {
  const closeButtonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!isOpen) return;
    closeButtonRef.current?.focus();
    const handleEscape = () => onClose();
    window.addEventListener("workspace-escape", handleEscape);
    return () => window.removeEventListener("workspace-escape", handleEscape);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal-content" role="dialog" aria-modal="true" aria-labelledby="backup-options-title" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <div className="modal-title">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
              <circle cx="12" cy="12" r="3" />
            </svg>
            <span id="backup-options-title">备份选项与过滤规则</span>
          </div>

          <button ref={closeButtonRef} type="button" className="modal-close-btn" onClick={onClose} aria-label="关闭备份选项">
            ×
          </button>
        </div>

        <div className="modal-body">
          <div className="option-section">
            <div className="option-section-title">备份方案与校验</div>
            <div className="control-field">
              <label className="field-title" htmlFor="backup-job-name">方案名称</label>
              <input
                id="backup-job-name"
                className="pattern-textarea"
                value={options.jobName}
                onChange={(event) => onOptionsChange({ ...options, jobName: event.target.value })}
                placeholder="个人文件"
                disabled={disabled}
              />
            </div>
            <div className="select-row">
              <div className="select-field">
                <label className="field-title" htmlFor="backup-verify-mode">校验模式</label>
                <select id="backup-verify-mode" className="custom-select" value={options.verifyMode} onChange={(event) => onOptionsChange({ ...options, verifyMode: event.target.value as "fast" | "full" })} disabled={disabled}>
                  <option value="fast">快速：存在性 + 大小</option>
                  <option value="full">完整：BLAKE3 Hash</option>
                </select>
              </div>
              <div className="select-field">
                <label className="field-title" htmlFor="backup-metadata-level">元数据保留</label>
                <select id="backup-metadata-level" className="custom-select" value={options.metadataPreserveLevel} onChange={(event) => onOptionsChange({ ...options, metadataPreserveLevel: event.target.value as "standard" | "windows" | "full" })} disabled={disabled}>
                  <option value="standard">Standard：修改时间</option>
                  <option value="windows">Windows：Windows 时间属性</option>
                  <option value="full">Full Fidelity：尽力保留</option>
                </select>
              </div>
            </div>
            <label className="toggle-row">
              <input type="checkbox" className="custom-checkbox" checked={options.incremental} onChange={() => onToggleOption("incremental")} disabled={disabled} />
              <div className="toggle-info"><span className="toggle-label">启用增量备份</span><span className="toggle-desc">复用上次 Snapshot 中未变化的文件，仍会生成新的 manifest。</span></div>
            </label>
          </div>

          <div className="option-section">
            <div className="option-section-title">排除过滤规则</div>
            <p className="option-section-desc">
              自动跳过临时文件、缓存和无需备份的依赖目录，提升备份速度并节省目标磁盘空间。
            </p>

            <label className="toggle-row">
              <input
                type="checkbox"
                className="custom-checkbox"
                checked={options.enableSmartExclude}
                onChange={() => onToggleOption("enableSmartExclude")}
                disabled={disabled}
              />
              <div className="toggle-info">
                <span className="toggle-label">启用智能内置排除</span>
                <span className="toggle-desc">
                  自动忽略系统缩略图、临时文件、代码依赖包等
                </span>
              </div>
            </label>

            {options.enableSmartExclude ? (
              <div className="chip-list">
                {defaultBuiltInRules.map((rule) => (
                  <span className="chip built-in" key={rule}>
                    {rule}
                  </span>
                ))}
              </div>
            ) : null}

            <div className="custom-pattern-wrap">
              <label className="field-title">自定义附加排除规则（按行分隔）</label>
              <textarea
                className="pattern-textarea"
                value={customExcludeText}
                onChange={(e) => onCustomTextChange(e.target.value)}
                placeholder="例如：
dist
*.log
*.iso
cache"
                rows={4}
                disabled={disabled}
              />

              {derivedOptions.customExcludePatterns.length > 0 ? (
                <div className="chip-list">
                  {derivedOptions.customExcludePatterns.map((pat) => (
                    <button
                      type="button"
                      className="chip custom-removable"
                      key={pat}
                      onClick={() => onRemovePattern(pat)}
                      title="点击删除此排除项"
                      aria-label={`删除排除规则 ${pat}`}
                      disabled={disabled}
                    >
                      <span>{pat}</span>
                      <span className="chip-x">×</span>
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
          </div>

          <div className="option-section">
            <div className="option-section-title">归档与压缩</div>
            <p className="option-section-desc">
              默认仅复制源文件至备份目录。可选择在备份完成后打包为单个压缩文件。
            </p>

            <label className="toggle-row">
              <input
                type="checkbox"
                className="custom-checkbox"
                checked={options.compressAfterBackup}
                onChange={() => onToggleOption("compressAfterBackup")}
                disabled={disabled}
              />
              <div className="toggle-info">
                <span className="toggle-label">备份完成后自动打包压缩</span>
                <span className="toggle-desc">生成独立的 .zip 或 .7z 压缩归档</span>
              </div>
            </label>

            {options.compressAfterBackup ? (
              <div className="select-row">
                <div className="select-field">
                  <label className="field-title">归档格式</label>
                  <select
                    className="custom-select"
                    value={options.archiveFormat}
                    onChange={(e) =>
                      onOptionsChange({
                        ...options,
                        archiveFormat: e.target.value as "zip" | "sevenz",
                      })
                    }
                    disabled={disabled}
                  >
                    <option value="zip">ZIP (通用兼容)</option>
                    <option value="sevenz">7Z (更高压缩率)</option>
                  </select>
                </div>

                <div className="select-field">
                  <label className="field-title">压缩级别</label>
                  <select
                    className="custom-select"
                    value={options.compressionLevel}
                    onChange={(e) =>
                      onOptionsChange({
                        ...options,
                        compressionLevel: Number(e.target.value),
                      })
                    }
                    disabled={disabled}
                  >
                    <option value={0}>0 - 仅打包不压缩 (最快)</option>
                    <option value={3}>3 - 快速压缩</option>
                    <option value={6}>6 - 标准均衡 (推荐)</option>
                    <option value={9}>9 - 极限高压</option>
                  </select>
                </div>
              </div>
            ) : null}
          </div>

          <div className="option-section">
            <div className="option-section-title">通知提示</div>
            <label className="toggle-row">
              <input
                type="checkbox"
                className="custom-checkbox"
                checked={options.sendNotification}
                onChange={() => onToggleOption("sendNotification")}
                disabled={disabled}
              />
              <div className="toggle-info">
                <span className="toggle-label">任务完成后发送系统气泡通知</span>
                <span className="toggle-desc">
                  备份结束时在右下角提醒完成状态与文件统计
                </span>
              </div>
            </label>
          </div>
        </div>

        <div className="modal-footer">
          <button type="button" className="btn-primary" onClick={onClose}>
            完成设置
          </button>
        </div>
      </div>
    </div>
  );
}
