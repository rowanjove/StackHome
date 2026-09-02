import { describe, expect, it } from "vitest";
import type { AppConfig, BackupItem, BackupOptions, BackupSummary, ScanResult } from "../types/backup";
import {
  analyzeCapacity,
  createCustomItem,
  createDerivedOptions,
  formatBytes,
  formatDuration,
  getCompletionMessage,
  getTargetStatus,
  mergeItems,
  parseCustomPatterns,
} from "./backup-utils";

const defaultOptions: BackupOptions = {
  enableSmartExclude: true,
  customExcludePatterns: [],
  compressAfterBackup: false,
  archiveFormat: "zip",
  compressionLevel: 6,
  sendNotification: true,
  verifyMode: "fast",
  metadataPreserveLevel: "windows",
  incremental: false,
  jobName: "个人文件",
};

function item(overrides: Partial<BackupItem>): BackupItem {
  return {
    id: "documents",
    label: "文档",
    sourcePath: "C:\\Users\\Tester\\Documents",
    targetName: "文档",
    enabled: true,
    category: "system",
    description: null,
    isCustom: false,
    fileCount: null,
    totalSize: null,
    ...overrides,
  };
}

describe("backup-utils", () => {
  it("formats bytes with compact units", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(null)).toBe("--");
  });

  it("formats duration humanly", () => {
    expect(formatDuration(-1)).toBe("--");
    expect(formatDuration(45)).toBe("45 秒");
    expect(formatDuration(125)).toBe("2 分 5 秒");
    expect(formatDuration(3665)).toBe("1 小时 1 分");
  });

  it("analyzes capacity and detect deficits", () => {
    const enough = analyzeCapacity(1000, 5000);
    expect(enough?.hasEnoughSpace).toBe(true);
    expect(enough?.spaceDeficitBytes).toBe(0);

    const notEnough = analyzeCapacity(5000, 1000);
    expect(notEnough?.hasEnoughSpace).toBe(false);
    expect(notEnough?.spaceDeficitBytes).toBe(4000);
  });

  it("parses custom patterns uniquely and trims spaces", () => {
    expect(parseCustomPatterns(" dist \n*.log\nDIST\n\ncoverage ")).toEqual([
      "dist",
      "*.log",
      "coverage",
    ]);
  });

  it("creates derived options with parsed patterns", () => {
    expect(createDerivedOptions(defaultOptions, "dist\n*.tmp").customExcludePatterns).toEqual([
      "dist",
      "*.tmp",
    ]);
  });

  it("creates custom item from a path", () => {
    expect(createCustomItem("D:\\Projects\\Demo\\")).toMatchObject({
      label: "Demo",
      targetName: "Demo",
      isCustom: true,
      category: "custom",
    });
  });

  it("merges saved config with default and custom items", () => {
    const defaults = [
      item({ id: "desktop", label: "桌面", sourcePath: "C:\\Users\\Tester\\Desktop", targetName: "桌面" }),
      item({ id: "documents", label: "文档" }),
    ];
    const config: AppConfig = {
      targetRoot: "D:\\Backup",
      options: defaultOptions,
      items: [
        item({ id: "documents", enabled: false, label: "文档资料" }),
        item({
          id: "custom:d:/projects",
          label: "项目",
          sourcePath: "D:\\Projects",
          targetName: "项目",
          isCustom: true,
        }),
      ],
    };

    const merged = mergeItems(defaults, config);

    expect(merged).toHaveLength(3);
    expect(merged.find((entry) => entry.id === "documents")?.enabled).toBe(false);
    expect(merged.find((entry) => entry.id === "documents")?.label).toBe("文档资料");
    expect(merged.find((entry) => entry.isCustom)?.sourcePath).toBe("D:\\Projects");
  });

  it("builds completion message by result", () => {
    const done: BackupSummary = {
      status: "done",
      totalFiles: 1,
      successFiles: 1,
      failedFiles: 0,
      skippedByRuleCount: 0,
      totalBytes: 10,
      copiedBytes: 10,
      durationSeconds: 1,
      errors: [],
      archiveFormat: null,
      archiveError: null,
      reportPath: "",
      logPath: "",
      backupRoot: "",
      archivePath: null,
    };

    expect(getCompletionMessage(done)).toBe("备份已顺利完成。");
    expect(getCompletionMessage({ ...done, archiveError: "7z failed" })).toBe("备份已完成，但压缩归档失败。");
    expect(getCompletionMessage({ ...done, status: "cancelled" })).toBe("备份任务已中途取消，已复制的文件已保留。");
  });

  it("renders target status from scan result", () => {
    const result: ScanResult = {
      items: [],
      totalFiles: 0,
      totalBytes: 0,
      targetDriveFreeBytes: null,
      targetDriveName: null,
      targetAccessible: true,
      targetWritable: true,
      targetKind: "network",
      warnings: [],
      sourceWarnings: [],
      skippedByRuleCount: 0,
    };

    expect(getTargetStatus(result)).toBe("网络共享 · 可写入");
    expect(getTargetStatus(null)).toBe("等待检查");
  });
});
