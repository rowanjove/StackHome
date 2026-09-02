import type { BackupItem, ScanResult } from "../types/backup";
import { analyzeCapacity, formatBytes, getTargetStatus } from "../lib/backup-utils";

type TargetSelectorProps = {
  targetRoot: string;
  items: BackupItem[];
  scanResult: ScanResult | null;
  busy: boolean;
  disabled: boolean;
  onPickTarget: () => void;
  onTargetChange: (path: string) => void;
  onScan: () => void;
  onStartBackup: () => void;
};

export function TargetSelector({
  targetRoot,
  items,
  scanResult,
  busy,
  disabled,
  onPickTarget,
  onTargetChange,
  onScan,
  onStartBackup,
}: TargetSelectorProps) {
  const enabledItems = items.filter((item) => item.enabled);
  const totalEstimatedBytes = scanResult
    ? scanResult.totalBytes
    : items
        .filter((i) => i.enabled && i.totalSize != null)
        .reduce((sum, i) => sum + (i.totalSize || 0), 0);

  const capacity = analyzeCapacity(
    totalEstimatedBytes || null,
    scanResult?.targetDriveFreeBytes,
  );

  const targetStatusText = getTargetStatus(scanResult);
  const isTargetEmpty = !targetRoot.trim();
  const noItemSelected = enabledItems.length === 0;

  return (
    <section className="panel target-panel">
      <div className="panel-header">
        <div>
          <h2>备份目标位置</h2>
          <p className="panel-desc">选择外置移动硬盘、数据盘或 NAS 网络共享位置</p>
        </div>

        {scanResult ? (
          <span className="target-media-tag">{targetStatusText}</span>
        ) : null}
      </div>

      <div className="target-input-row">
        <div className="input-wrapper">
          <svg className="input-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
          </svg>
          <input
            type="text"
            className="target-path-input"
            value={targetRoot}
            onChange={(e) => onTargetChange(e.target.value)}
            placeholder="例如 D:\Backup 或 \\192.168.1.100\backup"
            disabled={disabled}
          />
        </div>

        <button
          type="button"
          className="btn-secondary pick-btn"
          onClick={onPickTarget}
          disabled={disabled}
        >
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
            <polyline points="9 22 9 12 15 12 15 22" />
          </svg>
          <span>浏览位置...</span>
        </button>
      </div>

      <div className="capacity-card">
        <div className="capacity-header">
          <div className="capacity-stat">
            <span className="label">已选备份大小</span>
            <span className="val highlight">
              {totalEstimatedBytes > 0 ? formatBytes(totalEstimatedBytes) : "待计算"}
            </span>
          </div>

          <div className="capacity-stat right">
            <span className="label">目标可用空间</span>
            <span className="val">
              {scanResult?.targetDriveFreeBytes != null
                ? formatBytes(scanResult.targetDriveFreeBytes)
                : targetRoot
                ? "就绪时自动检测"
                : "未指定目标"}
            </span>
          </div>
        </div>

        {capacity ? (
          <>
            <div className="capacity-bar-track">
              <div
                className={`capacity-bar-fill ${
                  !capacity.hasEnoughSpace ? "danger" : capacity.usageRatio > 80 ? "warning" : "normal"
                }`}
                style={{ width: `${Math.max(4, Math.min(100, capacity.usageRatio))}%` }}
              />
            </div>

            <div className="capacity-footer">
              {capacity.hasEnoughSpace ? (
                <span className="capacity-note success">
                  ✓ 目标磁盘空间充足 (预计占用空间约 {capacity.usageRatio}%)
                </span>
              ) : (
                <span className="capacity-note danger">
                  ⚠ 目标磁盘空间不足！预计还缺少 {formatBytes(capacity.spaceDeficitBytes)}
                </span>
              )}
            </div>
          </>
        ) : null}
      </div>

      {scanResult?.warnings.length ? (
        <div className="notice-box warning">
          {scanResult.warnings.map((w) => (
            <p key={w}>⚠ {w}</p>
          ))}
        </div>
      ) : null}

      {scanResult?.sourceWarnings.length ? (
        <div className="notice-box info">
          {scanResult.sourceWarnings.map((w) => (
            <p key={w}>ℹ {w}</p>
          ))}
        </div>
      ) : null}

      <div className="target-actions">
        <button
          type="button"
          className="btn-ghost"
          onClick={onScan}
          disabled={disabled || isTargetEmpty || noItemSelected}
          title="重新检测已选文件数量与目标磁盘空间"
        >
          {busy ? "正在检查..." : "重新预检容量"}
        </button>

        <button
          type="button"
          className="btn-primary main-start-btn"
          onClick={onStartBackup}
          disabled={disabled || isTargetEmpty || noItemSelected}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polygon points="5 3 19 12 5 21 5 3" />
          </svg>
          <span>
            {totalEstimatedBytes > 0
              ? `开始备份 (${formatBytes(totalEstimatedBytes)})`
              : "开始备份"}
          </span>
        </button>
      </div>
    </section>
  );
}
