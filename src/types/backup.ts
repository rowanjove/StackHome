export type BackupStatus =
  | "idle"
  | "scanning"
  | "ready"
  | "copying"
  | "compressing"
  | "cancelled"
  | "done"
  | "error";

export type BackupPhase =
  | "idle"
  | "scanning"
  | "copying"
  | "compressing"
  | "cancelled"
  | "done"
  | "error";

export type BackupOptions = {
  enableSmartExclude: boolean;
  customExcludePatterns: string[];
  compressAfterBackup: boolean;
  archiveFormat: "zip" | "sevenz";
  compressionLevel: number;
  sendNotification: boolean;
  verifyMode: "fast" | "full";
  metadataPreserveLevel: "standard" | "windows" | "full";
  incremental: boolean;
  jobName: string;
};

export type AppConfig = {
  targetRoot: string;
  options: BackupOptions;
  items?: BackupItem[];
  customItems?: BackupItem[];
};

export type AutomationConfig = {
  watchEnabled: boolean;
  watchPath?: string | null;
  watchDestinationRoot?: string | null;
  watchRuleId?: string | null;
  watchAutoApply: boolean;
  scheduledBackupEnabled: boolean;
  scheduledBackupIntervalMinutes: number;
  minimizeToTray: boolean;
};

export type AutomationStatus = {
  watchRunning: boolean;
  scheduledBackupRunning: boolean;
  watchPath?: string | null;
  nextScheduledRunAt?: number | null;
};

export type BackupItemCategory = "system" | "app" | "dev" | "custom";

export type BackupItem = {
  id: string;
  label: string;
  sourcePath: string;
  targetName: string;
  enabled: boolean;
  category?: BackupItemCategory | string;
  description?: string | null;
  isCustom: boolean;
  fileCount?: number | null;
  totalSize?: number | null;
};

export type ScanResult = {
  items: BackupItem[];
  totalFiles: number;
  totalBytes: number;
  targetDriveFreeBytes?: number | null;
  targetDriveName?: string | null;
  targetAccessible: boolean;
  targetWritable: boolean;
  targetKind: string;
  warnings: string[];
  sourceWarnings: string[];
  skippedByRuleCount: number;
};

export type BackupProgress = {
  taskId?: string | null;
  phase: BackupPhase;
  currentFolder: string;
  currentFile: string;
  currentFileSize?: number | null;
  currentFileCopied?: number | null;
  totalFiles: number;
  copiedFiles: number;
  totalBytes: number;
  copiedBytes: number;
  failedFiles: number;
  skippedByRuleCount: number;
  speedBytesPerSec: number;
  estimatedSecondsLeft: number;
  percent: number;
  status: BackupStatus;
};

export type TaskProgress = {
  taskId: string;
  taskType: string;
  phase: string;
  completedItems: number;
  totalItems: number;
  completedBytes: number;
  totalBytes: number;
  currentPath?: string | null;
  speedBytesPerSecond?: number | null;
  etaSeconds?: number | null;
};

export type FileCategory =
  | "image"
  | "video"
  | "audio"
  | "document"
  | "archive"
  | "installer"
  | "code"
  | "other";

export type FileRecord = {
  id: string;
  path: string;
  filename: string;
  stem: string;
  extension: string;
  size: number;
  createdAt?: number | null;
  modifiedAt?: number | null;
  accessedAt?: number | null;
  mime?: string | null;
  category: FileCategory | string;
  sourceType?: string | null;
  hash?: string | null;
  hashAlgorithm?: string | null;
  metadata?: FileMetadata | null;
  tags: string[];
};

export type FileMetadata = {
  width?: number | null;
  height?: number | null;
  orientation?: number | null;
  exifDate?: string | null;
  cameraMake?: string | null;
  cameraModel?: string | null;
  gpsLatitude?: string | null;
  gpsLongitude?: string | null;
  artist?: string | null;
  album?: string | null;
  title?: string | null;
  track?: number | null;
  year?: number | null;
  genre?: string | null;
  durationSeconds?: number | null;
  creationTime?: string | null;
  codec?: string | null;
  extensionMismatch: boolean;
  unsupported: boolean;
};

export type MetadataReadResult = { path: string; metadata: FileMetadata };

export type CatalogScanRequest = {
  rootPath: string;
  sourceType?: string | null;
  includeHidden?: boolean;
  includeSystemFiles?: boolean;
  customExcludePatterns?: string[];
};

export type CatalogScanResult = {
  taskId: string;
  rootPath: string;
  totalFiles: number;
  totalBytes: number;
  indexedFiles: number;
  skippedFiles: number;
  warnings: string[];
};

export type CatalogQuery = {
  search: string;
  rootPath?: string | null;
  category?: string | null;
  sourceType?: string | null;
  limit?: number;
  offset?: number;
};

export type ConflictInfo = {
  kind: string;
  message: string;
  suggestedPath?: string | null;
};

export type PlannedOperation = {
  id: string;
  type: "rename" | "move" | "copy" | "tag" | "recycle" | "restore" | string;
  sourcePath: string;
  destinationPath?: string | null;
  reason: string;
  ruleId?: string | null;
  conflict?: ConflictInfo | null;
  status: "ready" | "conflict" | "invalid" | string;
  sourceSize?: number | null;
  sourceModifiedAt?: number | null;
  tags?: string[];
};

export type PlanPreview = {
  id: string;
  taskId: string;
  createdAt: number;
  status: string;
  operations: PlannedOperation[];
};

export type ApplyPlanResult = {
  taskId: string;
  planId: string;
  status: string;
  completed: number;
  failed: number;
  operations: PlannedOperation[];
};

export type OperationHistoryItem = {
  id: string;
  planId?: string | null;
  taskId?: string | null;
  type: string;
  sourcePath: string;
  destinationPath?: string | null;
  status: string;
  error?: string | null;
  executedAt?: number | null;
  undoStatus: string;
};

export type BackupError = {
  sourcePath: string;
  targetPath: string;
  reason: string;
};

export type BackupSummary = {
  status: "done" | "cancelled" | "error";
  totalFiles: number;
  successFiles: number;
  failedFiles: number;
  skippedByRuleCount: number;
  totalBytes: number;
  copiedBytes: number;
  durationSeconds: number;
  errors: BackupError[];
  archiveFormat?: string | null;
  archiveError?: string | null;
  reportPath: string;
  logPath: string;
  backupRoot: string;
  archivePath?: string | null;
  snapshotId?: string | null;
  manifestPath?: string | null;
  verifyStatus?: string | null;
};

export type RuleSource = { sourceType?: string | null; pathContains?: string | null };
export type RuleAction = {
  type: "rename" | "move" | "copy" | "tag" | "ignore";
  destinationTemplate?: string | null;
  renameTemplate?: string | null;
  tags: string[];
};
export type RuleDefinition = {
  source?: RuleSource | null;
  condition: Record<string, unknown>;
  action: RuleAction;
};
export type RuleRecord = {
  id: string;
  name: string;
  enabled: boolean;
  priority: number;
  ruleType: string;
  definition: RuleDefinition;
  createdAt: number;
  updatedAt: number;
};

export type DuplicateGroup = {
  id: string;
  hash: string;
  size: number;
  files: FileRecord[];
  reclaimableSize: number;
};
export type DuplicateScanResult = {
  taskId: string;
  rootPath: string;
  status: string;
  totalFiles: number;
  duplicateFiles: number;
  reclaimableSize: number;
  groups: DuplicateGroup[];
};
export type SimilarScanResult = {
  taskId: string;
  rootPath: string;
  status: string;
  totalImages: number;
  groups: { id: string; distance: number; files: FileRecord[]; reclaimableSize: number }[];
};

export type BackupJobRecord = {
  id: string;
  name: string;
  sourceConfig: unknown;
  targetPath: string;
  policy: unknown;
  createdAt: number;
};
export type SnapshotRecord = {
  id: string;
  backupJobId?: string | null;
  snapshotTime: number;
  fileCount: number;
  totalSize: number;
  manifestPath?: string | null;
  status: string;
};
export type SnapshotFileRecord = {
  snapshotId: string;
  sourcePath: string;
  backupPath: string;
  size: number;
  mtime?: number | null;
  hash?: string | null;
};
export type SnapshotManifest = {
  snapshotId: string;
  createdAt: number;
  files: SnapshotFileRecord[];
};
export type SnapshotVerifyResult = {
  taskId: string;
  snapshotId: string;
  mode: string;
  checkedFiles: number;
  failedFiles: number;
  status: string;
  errors: string[];
};
