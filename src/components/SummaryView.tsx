import { openPath } from "@tauri-apps/plugin-opener";
import type { BackupSummary } from "../types/backup";
import { formatBytes, formatDuration } from "../lib/backup-utils";

type SummaryViewProps = {
  summary: BackupSummary;
  onReset: () => void;
};

export function SummaryView({ summary, onReset }: SummaryViewProps) {
  const isDone = summary.status === "done";
  const isCancelled = summary.status === "cancelled";

  const statusTitle = isDone
    ? "备份任务已圆满完成！"
    : isCancelled
    ? "备份任务已中途停止"
    : "备份任务异常结束";

  const statusDesc = isDone
    ? summary.archiveError
      ? "文件已全部复制成功，但压缩打包阶段出现错误。"
      : "所有已勾选的文件与文件夹均已安全复制至目标目录。"
    : isCancelled
    ? "任务已按要求停止，已复制完成的文件均完整保留。"
    : "部分文件在备份过程中遇到错误，请查看报告与日志。";

  return (
    <section className="panel summary-panel">
      <div className={`summary-hero ${isDone ? "success" : isCancelled ? "warning" : "danger"}`}>
        <div className="summary-icon">
          {isDone ? (
            <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="20 6 9 17 4 12" />
            </svg>
          ) : isCancelled ? (
            <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="10" />
              <line x1="12" y1="8" x2="12" y2="12" />
              <line x1="12" y1="16" x2="12.01" y2="16" />
            </svg>
          ) : (
            <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="10" />
              <line x1="15" y1="9" x2="9" y2="15" />
              <line x1="9" y1="9" x2="15" y2="15" />
            </svg>
          )}
        </div>

        <div className="summary-hero-text">
          <h2>{statusTitle}</h2>
          <p>{statusDesc}</p>
        </div>
      </div>

      <div className="summary-metrics-grid">
        <div className="metric-item">
          <span className="metric-label">成功复制</span>
          <span className="metric-value success">{summary.successFiles} 个文件</span>
        </div>

        <div className="metric-item">
          <span className="metric-label">备份总大小</span>
          <span className="metric-value">{formatBytes(summary.copiedBytes)}</span>
        </div>

        <div className="metric-item">
          <span className="metric-label">任务耗时</span>
          <span className="metric-value">{formatDuration(summary.durationSeconds)}</span>
        </div>

        <div className="metric-item">
          <span className="metric-label">规则跳过</span>
          <span className="metric-value muted">{summary.skippedByRuleCount} 项</span>
        </div>

        <div className="metric-item">
          <span className="metric-label">失败文件</span>
          <span className={`metric-value ${summary.failedFiles > 0 ? "danger" : ""}`}>
            {summary.failedFiles} 个
          </span>
        </div>

        <div className="metric-item">
          <span className="metric-label">压缩归档</span>
          <span className="metric-value">
            {summary.archivePath ? `${summary.archiveFormat?.toUpperCase()} 已生成` : summary.archiveError ? "打包失败" : "未开启"}
          </span>
        </div>

        <div className="metric-item">
          <span className="metric-label">Snapshot 校验</span>
          <span className={`metric-value ${summary.verifyStatus?.includes("failed") ? "danger" : "success"}`}>
            {summary.verifyStatus || "未执行"}
          </span>
        </div>
      </div>

      <div className="output-access-card">
        <div className="output-title">输出位置与文件</div>

        <div className="output-path-row">
          <span className="path-label">备份主目录：</span>
          <code className="path-code" title={summary.backupRoot}>
            {summary.backupRoot}
          </code>
          <button
            type="button"
            className="btn-secondary btn-sm"
            onClick={() => openPath(summary.backupRoot)}
          >
            打开目录
          </button>
        </div>

        <div className="output-btn-group">
          <button
            type="button"
            className="btn-secondary"
            onClick={() => openPath(summary.reportPath)}
          >
            查看备份报告
          </button>

          <button
            type="button"
            className="btn-secondary"
            onClick={() => openPath(summary.logPath)}
          >
            查看运行日志
          </button>

          {summary.archivePath ? (
            <button
              type="button"
              className="btn-secondary"
              onClick={() => openPath(summary.archivePath!)}
            >
              打开压缩包
            </button>
          ) : null}

          {summary.manifestPath ? (
            <button type="button" className="btn-secondary" onClick={() => openPath(summary.manifestPath!)}>
              打开 manifest
            </button>
          ) : null}
        </div>
      </div>

      {summary.errors.length > 0 ? (
        <div className="error-preview-card">
          <div className="error-preview-title">跳过与失败文件列表 ({summary.errors.length} 项)</div>
          <div className="error-list">
            {summary.errors.slice(0, 8).map((err, idx) => (
              <div className="error-row" key={idx}>
                <span className="error-reason">[{err.reason}]</span>
                <span className="error-path" title={err.sourcePath}>
                  {err.sourcePath}
                </span>
              </div>
            ))}
            {summary.errors.length > 8 ? (
              <div className="error-more">还有 {summary.errors.length - 8} 项，请在日志中查看完整清单</div>
            ) : null}
          </div>
        </div>
      ) : null}

      <div className="summary-actions">
        <button type="button" className="btn-primary" onClick={onReset}>
          返回继续备份
        </button>
      </div>
    </section>
  );
}
