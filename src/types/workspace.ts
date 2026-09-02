export type WorkspacePage =
  | "recent"
  | "files"
  | "organizer"
  | "duplicates"
  | "backup"
  | "restore"
  | "rules"
  | "history"
  | "settings";

export const pageMeta: Record<WorkspacePage, { label: string; description: string }> = {
  recent: { label: "最近", description: "查看最近的文件操作与任务入口" },
  files: { label: "文件", description: "浏览已建立索引的本地文件资产" },
  organizer: { label: "整理", description: "先生成计划与预览，再应用文件变化" },
  duplicates: { label: "重复项", description: "按文件大小与哈希识别重复文件" },
  backup: { label: "备份", description: "保护个人文件并保留现有备份能力" },
  restore: { label: "恢复", description: "从 Snapshot 预览并恢复文件" },
  rules: { label: "规则", description: "管理整理与命名规则" },
  history: { label: "历史", description: "查看操作 Journal 并撤销安全操作" },
  settings: { label: "设置", description: "外观、扫描和安全策略" },
};
