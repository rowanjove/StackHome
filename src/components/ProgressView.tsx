import type { BackupProgress } from "../types/backup";
import { formatBytes, formatDuration } from "../lib/backup-utils";

type ProgressViewProps = {
  progress: BackupProgress;
  onCancel: () => void;
};

export function ProgressView({ progress, onCancel }: ProgressViewProps) {
  const isCompressing = progress.phase === "compressing";
  const currentFileName = progress.currentFile || "准备传输...";
  const currentFolder = progress.currentFolder || (isCompressing ? "压缩归档" : "正在备份");

  return (
    <section className="panel progress-panel">
      <div className="progress-header">
        <div className="progress-title-wrap">
          <div className="spinner" />
          <div>
            <h2>{isCompressing ? "正在压缩归档" : "正在执行备份"}</h2>
            <p className="progress-folder">当前分类：{currentFolder}</p>
          </div>
        </div>

        <div className="percent-display">{progress.percent}%</div>
      </div>

      <div className="progress-bar-wrap">
        <div
          className="progress-bar-fill animated"
          style={{ width: `${Math.max(2, Math.min(100, progress.percent))}%` }}
        />
      </div>

      <div className="current-file-box">
        <span className="file-icon">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z" />
            <polyline points="13 2 13 9 20 9" />
          </svg>
        </span>
        <span className="file-name" title={currentFileName}>
          {currentFileName}
        </span>
      </div>

      <div className="progress-stats-grid">
        <div className="stat-card">
          <span className="stat-label">文件进度</span>
          <span className="stat-value">
            {progress.copiedFiles} / {progress.totalFiles}
          </span>
        </div>

        <div className="stat-card">
          <span className="stat-label">数据总量</span>
          <span className="stat-value">
            {formatBytes(progress.copiedBytes)} / {formatBytes(progress.totalBytes)}
          </span>
        </div>

        <div className="stat-card">
          <span className="stat-label">传输速度</span>
          <span className="stat-value highlight">
            {formatBytes(progress.speedBytesPerSec)}/s
          </span>
        </div>

        <div className="stat-card">
          <span className="stat-label">预计剩余时间</span>
          <span className="stat-value">
            {formatDuration(progress.estimatedSecondsLeft)}
          </span>
        </div>

        <div className="stat-card">
          <span className="stat-label">规则跳过项</span>
          <span className="stat-value muted">{progress.skippedByRuleCount}</span>
        </div>

        <div className="stat-card">
          <span className="stat-label">遇到错误</span>
          <span className={`stat-value ${progress.failedFiles > 0 ? "text-danger" : ""}`}>
            {progress.failedFiles}
          </span>
        </div>
      </div>

      <div className="progress-actions">
        <button
          type="button"
          className="btn-secondary btn-danger-ghost"
          onClick={onCancel}
        >
          取消备份
        </button>
      </div>
    </section>
  );
}
