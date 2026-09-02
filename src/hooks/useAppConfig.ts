import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  createCustomItem,
  createDerivedOptions,
  mergeItems,
} from "../lib/backup-utils";
import type {
  AppConfig,
  BackupItem,
  BackupOptions,
} from "../types/backup";

export const defaultOptions: BackupOptions = {
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

export function useAppConfig(onStatusMessage?: (msg: string) => void) {
  const [items, setItems] = useState<BackupItem[]>([]);
  const [targetRoot, setTargetRoot] = useState("");
  const [options, setOptions] = useState<BackupOptions>(defaultOptions);
  const [customExcludeText, setCustomExcludeText] = useState("");
  const [loading, setLoading] = useState(true);
  const [hydrated, setHydrated] = useState(false);

  const derivedOptions = useMemo(
    () => createDerivedOptions(options, customExcludeText),
    [customExcludeText, options],
  );

  useEffect(() => {
    let mounted = true;

    Promise.all([
      invoke<BackupItem[]>("get_default_backup_items"),
      invoke<AppConfig>("get_app_config").catch(() => ({
        targetRoot: "",
        options: defaultOptions,
        items: [],
      })),
    ])
      .then(([defaultItems, config]) => {
        if (!mounted) return;
        setItems(mergeItems(defaultItems, config));
        setTargetRoot(config.targetRoot ?? "");
        setOptions({
          ...defaultOptions,
          ...config.options,
        });
        setCustomExcludeText((config.options?.customExcludePatterns ?? []).join("\n"));
        onStatusMessage?.("已恢复备份项与预设配置。");
      })
      .catch((error) => {
        if (!mounted) return;
        console.error("加载配置失败:", error);
        onStatusMessage?.(`加载配置失败：${String(error)}`);
      })
      .finally(() => {
        if (!mounted) return;
        setLoading(false);
        setHydrated(true);
      });

    return () => {
      mounted = false;
    };
  }, []);

  // Save config on changes with debounce
  useEffect(() => {
    if (!hydrated || loading) return;

    const timer = window.setTimeout(() => {
      void invoke("save_app_config", {
        config: {
          targetRoot,
          options: derivedOptions,
          items,
        },
      }).catch((error) => {
        console.error("保存配置失败:", error);
      });
    }, 300);

    return () => window.clearTimeout(timer);
  }, [derivedOptions, hydrated, items, loading, targetRoot]);

  function toggleItem(id: string) {
    setItems((current) =>
      current.map((item) =>
        item.id === id ? { ...item, enabled: !item.enabled } : item,
      ),
    );
  }

  function toggleCategory(category: string, enabled: boolean) {
    setItems((current) =>
      current.map((item) =>
        (item.category || "custom") === category ? { ...item, enabled } : item,
      ),
    );
  }

  function toggleAll(enabled: boolean) {
    setItems((current) => current.map((item) => ({ ...item, enabled })));
  }

  function addCustomFolder(folderPath: string) {
    const nextItem = createCustomItem(folderPath);
    let added = false;
    setItems((current) => {
      if (current.some((item) => item.id === nextItem.id)) {
        return current;
      }
      added = true;
      return [...current, nextItem];
    });
    return added;
  }

  function removeCustomItem(id: string) {
    setItems((current) => current.filter((item) => item.id !== id));
  }

  function renameCustomItem(id: string, nextLabel: string) {
    setItems((current) =>
      current.map((item) => {
        if (item.id !== id) return item;
        const label = nextLabel.trim() || item.label;
        return {
          ...item,
          label,
          targetName: label,
        };
      }),
    );
  }

  function clearCustomItems() {
    setItems((current) => current.filter((item) => !item.isCustom));
  }

  function toggleOption<K extends keyof BackupOptions>(key: K) {
    setOptions((current) => ({ ...current, [key]: !current[key] }));
  }

  function removePattern(pattern: string) {
    const nextPatterns = derivedOptions.customExcludePatterns.filter(
      (item) => item.toLowerCase() !== pattern.toLowerCase(),
    );
    setCustomExcludeText(nextPatterns.join("\n"));
  }

  return {
    items,
    setItems,
    targetRoot,
    setTargetRoot,
    options,
    setOptions,
    customExcludeText,
    setCustomExcludeText,
    derivedOptions,
    loading,
    hydrated,
    toggleItem,
    toggleCategory,
    toggleAll,
    addCustomFolder,
    removeCustomItem,
    renameCustomItem,
    clearCustomItems,
    toggleOption,
    removePattern,
  };
}
