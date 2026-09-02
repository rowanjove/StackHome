import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCompletionMessage } from "../lib/backup-utils";
import { isTauriRuntime } from "../lib/tauri-runtime";
import type {
  BackupItem,
  BackupOptions,
  BackupProgress,
  BackupStatus,
  BackupSummary,
  ScanResult,
  TaskProgress,
} from "../types/backup";

export const emptyProgress: BackupProgress = {
  phase: "idle",
  currentFolder: "",
  currentFile: "",
  totalFiles: 0,
  copiedFiles: 0,
  totalBytes: 0,
  copiedBytes: 0,
  failedFiles: 0,
  skippedByRuleCount: 0,
  speedBytesPerSec: 0,
  estimatedSecondsLeft: -1,
  percent: 0,
  status: "idle",
};

export function useBackupEngine(onItemsUpdate?: (updated: BackupItem[]) => void) {
  const [status, setStatus] = useState<BackupStatus>("idle");
  const [progress, setProgress] = useState<BackupProgress>(emptyProgress);
  const [summary, setSummary] = useState<BackupSummary | null>(null);
  const [scanResult, setScanResult] = useState<ScanResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [statusMessage, setStatusMessage] = useState<string>("就绪。请选择备份源和目标位置。");
  const progressTaskIdRef = useRef<string | null>(null);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let mounted = true;

    const unlistenPromise = listen<TaskProgress>("task-progress", (event) => {
      if (!mounted) return;
      if (event.payload.taskType !== "backup") return;
      progressTaskIdRef.current = event.payload.taskId;
      const currentPath = event.payload.currentPath ?? "";
      const isCompressing = event.payload.phase === "verifying" && currentPath.includes("压缩");
      const total = event.payload.totalBytes || event.payload.totalItems;
      const completed = event.payload.totalBytes ? event.payload.completedBytes : event.payload.completedItems;
      const percent = total > 0 ? Math.min(100, Math.round((completed / total) * 100)) : 0;
      setProgress((current) => ({
        ...current,
        taskId: event.payload.taskId,
        phase: isCompressing ? "compressing" : "copying",
        currentFolder: isCompressing ? "压缩归档" : "正在备份",
        currentFile: currentPath,
        totalFiles: event.payload.totalItems || current.totalFiles,
        copiedFiles: event.payload.completedItems,
        totalBytes: event.payload.totalBytes || current.totalBytes,
        copiedBytes: event.payload.completedBytes,
        speedBytesPerSec: event.payload.speedBytesPerSecond ?? 0,
        estimatedSecondsLeft: event.payload.etaSeconds ?? -1,
        percent,
        status: "copying",
      }));
      setStatus("copying");
    }).catch((error) => {
      console.error("进度监听失败:", error);
      return () => undefined;
    });
    const errorPromise = listen<{ taskId: string; error: string }>("task-error", (event) => {
      if (!mounted) return;
      if (event.payload.taskId === progressTaskIdRef.current) {
        setStatus("error");
        setStatusMessage(event.payload.error);
      }
    }).catch((error) => {
      console.error("任务错误监听失败:", error);
      return () => undefined;
    });

    return () => {
      mounted = false;
      void unlistenPromise.then((unlisten) => unlisten());
      void errorPromise.then((unlisten) => unlisten());
    };
  }, []);

  function resetToIdle() {
    setStatus("idle");
    setProgress(emptyProgress);
    setSummary(null);
    setScanResult(null);
    setStatusMessage("准备就绪。");
  }

  async function handleScan(
    items: BackupItem[],
    targetRoot: string,
    options: BackupOptions,
  ): Promise<ScanResult | null> {
    const enabledItems = items.filter((item) => item.enabled);

    if (!targetRoot.trim()) {
      setStatusMessage("请先指定备份目标位置。");
      return null;
    }

    if (enabledItems.length === 0) {
      setStatusMessage("请至少勾选一个要备份的项目。");
      return null;
    }

    setBusy(true);
    setStatus("scanning");
    setSummary(null);
    setStatusMessage("正在快速扫描文件与检查磁盘空间...");

    try {
      const result = await invoke<ScanResult>("scan_backup_items", {
        items: enabledItems,
        targetRoot: targetRoot.trim(),
        options,
      });

      if (onItemsUpdate) {
        onItemsUpdate(result.items);
      }
      setScanResult(result);
      setStatus("ready");
      setStatusMessage(`扫描完成：共 ${result.totalFiles} 个文件。可以开始备份。`);
      return result;
    } catch (error) {
      setStatus("error");
      setStatusMessage(String(error));
      return null;
    } finally {
      setBusy(false);
    }
  }

  async function handleStartBackup(
    items: BackupItem[],
    targetRoot: string,
    options: BackupOptions,
  ): Promise<BackupSummary | null> {
    const enabledItems = items.filter((item) => item.enabled);

    if (!targetRoot.trim()) {
      setStatusMessage("请先指定备份目标位置。");
      return null;
    }

    if (enabledItems.length === 0) {
      setStatusMessage("请至少勾选一个要备份的项目。");
      return null;
    }

    setBusy(true);
    setStatus("copying");
    setSummary(null);
    setProgress({
      ...emptyProgress,
      phase: "copying",
      status: "copying",
      totalFiles: scanResult?.totalFiles ?? 0,
      totalBytes: scanResult?.totalBytes ?? 0,
    });
    setStatusMessage("备份任务正在执行，复制中...");

    try {
      const result = await invoke<BackupSummary>("start_backup", {
        items: enabledItems,
        targetRoot: targetRoot.trim(),
        options,
      });

      setSummary(result);
      setStatus(result.status as BackupStatus);
      setStatusMessage(getCompletionMessage(result));
      return result;
    } catch (error) {
      setStatus("error");
      setStatusMessage(`备份失败：${String(error)}`);
      return null;
    } finally {
      setBusy(false);
    }
  }

  async function handleCancel() {
    try {
      await invoke("cancel_backup", { taskId: progress.taskId ?? null });
      setStatusMessage("正在停止备份任务，当前文件处理完成后将安全退出...");
    } catch (error) {
      setStatusMessage(`取消请求失败：${String(error)}`);
    }
  }

  return {
    status,
    setStatus,
    progress,
    summary,
    scanResult,
    setScanResult,
    busy,
    statusMessage,
    setStatusMessage,
    handleScan,
    handleStartBackup,
    handleCancel,
    resetToIdle,
  };
}
