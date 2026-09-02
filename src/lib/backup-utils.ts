import type {
  AppConfig,
  BackupItem,
  BackupItemCategory,
  BackupOptions,
  BackupStatus,
  BackupSummary,
  ScanResult,
} from "../types/backup";

export const defaultBuiltInRules = [
  "node_modules",
  ".git",
  ".cache",
  "Thumbs.db",
  "Desktop.ini",
  "*.tmp",
  "~$*",
  "Temp",
  "tmp",
];

export const statusLabelMap: Record<BackupStatus, string> = {
  idle: "待命",
  scanning: "正在扫描",
  ready: "准备就绪",
  copying: "正在备份",
  compressing: "正在压缩",
  cancelled: "已取消",
  done: "已完成",
  error: "执行异常",
};

export const categoryMetaMap: Record<
  BackupItemCategory,
  { label: string; description: string }
> = {
  system: {
    label: "系统常用目录",
    description: "桌面、下载、文档、图片、视频、音乐等个人核心资料",
  },
  app: {
    label: "应用数据",
    description: "微信聊天接收文件、QQ 离线传输与浏览器书签等",
  },
  dev: {
    label: "开发与配置",
    description: "SSH 密钥凭证、VS Code 设置与代码片段",
  },
  custom: {
    label: "自定义目录",
    description: "用户自行添加的独立分区或项目文件夹",
  },
};

export function formatBytes(value?: number | null): string {
  if (value == null) return "--";
  if (value === 0) return "0 B";

  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = value;
  let unitIndex = 0;

  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }

  return `${size.toFixed(size >= 10 || unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`;
}

export function formatDuration(seconds: number): string {
  if (seconds < 0) return "--";
  if (seconds < 60) return `${seconds} 秒`;
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  if (mins < 60) return `${mins} 分 ${secs} 秒`;
  const hours = Math.floor(mins / 60);
  const remainMins = mins % 60;
  return `${hours} 小时 ${remainMins} 分`;
}

export type CapacityAnalysis = {
  totalBytes: number;
  freeBytes: number;
  hasEnoughSpace: boolean;
  usageRatio: number;
  spaceDeficitBytes: number;
};

export function analyzeCapacity(
  totalBytes?: number | null,
  freeBytes?: number | null,
): CapacityAnalysis | null {
  if (totalBytes == null || freeBytes == null) return null;
  const hasEnoughSpace = freeBytes >= totalBytes;
  const spaceDeficitBytes = Math.max(0, totalBytes - freeBytes);
  const totalScope = freeBytes + totalBytes;
  const usageRatio =
    totalScope === 0 ? 0 : Math.min(100, Math.round((totalBytes / totalScope) * 100));

  return {
    totalBytes,
    freeBytes,
    hasEnoughSpace,
    usageRatio,
    spaceDeficitBytes,
  };
}

export function normalizePathKey(path: string): string {
  return path.replace(/[\\/]+$/, "").toLowerCase();
}

export function deriveFolderName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const segments = trimmed.split(/[\\/]/).filter(Boolean);
  return segments[segments.length - 1] || "自定义目录";
}

export function createCustomItem(path: string): BackupItem {
  const trimmed = path.replace(/[\\/]+$/, "");
  const folderName = deriveFolderName(trimmed);

  return {
    id: `custom:${normalizePathKey(trimmed)}`,
    label: folderName,
    sourcePath: trimmed,
    targetName: folderName,
    enabled: true,
    category: "custom",
    description: "用户自定义添加的文件夹",
    isCustom: true,
    fileCount: null,
    totalSize: null,
  };
}

export function parseCustomPatterns(text: string): string[] {
  const seen = new Set<string>();
  const patterns: string[] = [];

  text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .forEach((line) => {
      const key = line.toLowerCase();
      if (!seen.has(key)) {
        seen.add(key);
        patterns.push(line);
      }
    });

  return patterns;
}

export function mergeItems(defaultItems: BackupItem[], config: AppConfig): BackupItem[] {
  const savedItems = config.items ?? config.customItems ?? [];
  const savedById = new Map(savedItems.map((item) => [item.id, item]));
  const defaultPathKeys = new Set(defaultItems.map((item) => normalizePathKey(item.sourcePath)));

  const mergedDefaults = defaultItems.map((item) => {
    const saved = savedById.get(item.id);
    if (!saved) return item;

    return {
      ...item,
      enabled: saved.enabled,
      label: saved.label || item.label,
      targetName: saved.targetName || item.targetName,
    };
  });

  const customItems = savedItems
    .filter((item) => item.isCustom)
    .filter((item) => !defaultPathKeys.has(normalizePathKey(item.sourcePath)))
    .map((item) => ({
      ...createCustomItem(item.sourcePath),
      enabled: item.enabled,
      label: item.label?.trim() || deriveFolderName(item.sourcePath),
      targetName: item.targetName?.trim() || item.label?.trim() || deriveFolderName(item.sourcePath),
    }));

  return [...mergedDefaults, ...customItems];
}

export function getCompletionMessage(summary: BackupSummary): string {
  if (summary.status === "done") {
    return summary.archiveError ? "备份已完成，但压缩归档失败。" : "备份已顺利完成。";
  }

  if (summary.status === "cancelled") {
    return "备份任务已中途取消，已复制的文件已保留。";
  }

  return "备份任务异常终止。";
}

export function getTargetStatus(scanResult: ScanResult | null): string {
  if (!scanResult) return "等待检查";

  const kind = scanResult.targetKind === "network" ? "网络共享" : "本地磁盘";
  const writable = scanResult.targetWritable ? "可写入" : "不可写";
  return `${kind} · ${writable}`;
}

export function createDerivedOptions(
  options: BackupOptions,
  customExcludeText: string,
): BackupOptions {
  return {
    ...options,
    customExcludePatterns: parseCustomPatterns(customExcludeText),
  };
}
