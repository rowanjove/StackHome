import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type { TaskProgress } from "../types/backup";
import { isTauriRuntime } from "../lib/tauri-runtime";

const taskLabels: Record<string, string> = {
  scan: "扫描文件",
  organize: "整理文件",
  backup: "备份文件",
  duplicate: "扫描重复项",
  restore: "恢复文件",
  verify: "校验 Snapshot",
  similar: "分析相似图片",
};

export function TaskCenter() {
  const [progress, setProgress] = useState<TaskProgress | null>(null);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let mounted = true;
    const unlisten = listen<TaskProgress>("task-progress", (event) => {
      if (mounted) setProgress(event.payload);
    });
    const completed = listen<{ taskId: string }>("task-completed", (event) => {
      if (mounted) setProgress((current) => current?.taskId === event.payload.taskId ? null : current);
    });
    return () => {
      mounted = false;
      void unlisten.then((dispose) => dispose());
      void completed.then((dispose) => dispose());
    };
  }, []);

  if (!progress) {
    return <footer className="task-center quiet">没有正在运行的任务</footer>;
  }

  const denominator = progress.totalBytes || progress.totalItems;
  const numerator = progress.totalBytes ? progress.completedBytes : progress.completedItems;
  const percent = denominator ? Math.min(100, Math.round((numerator / denominator) * 100)) : 0;

  return (
    <footer className="task-center" aria-live="polite">
      <div className="task-center-title">
        <span className="spinner small" aria-hidden="true" />
        <span>任务</span>
        <strong>{taskLabels[progress.taskType] ?? progress.taskType}</strong>
      </div>
      <div className="task-center-progress">
        <div className="task-progress-track"><div style={{ width: `${percent}%` }} /></div>
        <span>{percent}%</span>
      </div>
      <span className="task-center-path" title={progress.currentPath ?? undefined}>{progress.currentPath || "准备中"}</span>
      <button type="button" className="btn-ghost" onClick={() => void invoke("task_cancel", { taskId: progress.taskId })}>取消</button>
    </footer>
  );
}
