import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { OptionsPanel } from "../components/OptionsPanel";
import { ProgressView } from "../components/ProgressView";
import { SourceList } from "../components/SourceList";
import { SummaryView } from "../components/SummaryView";
import { TargetSelector } from "../components/TargetSelector";
import { useAppConfig } from "../hooks/useAppConfig";
import { useBackupEngine } from "../hooks/useBackupEngine";

export function BackupPage() {
  const [optionsOpen, setOptionsOpen] = useState(false);
  const config = useAppConfig();
  const engine = useBackupEngine((updatedItems) => {
    config.setItems((current) =>
      current.map((item) => updatedItems.find((updated) => updated.id === item.id) ?? item),
    );
  });

  async function pickTarget() {
    const selected = await open({ directory: true, multiple: false, title: "选择备份目标文件夹" });
    if (typeof selected === "string") {
      config.setTargetRoot(selected);
      engine.setStatusMessage("目标路径已更新。请先执行预检。" );
    }
  }

  async function addCustomFolder() {
    const selected = await open({ directory: true, multiple: false, title: "选择要备份的文件夹" });
    if (typeof selected !== "string") return;
    if (!config.addCustomFolder(selected)) {
      engine.setStatusMessage("该文件夹已在备份列表中。" );
    } else {
      engine.setStatusMessage("已添加自定义文件夹。" );
    }
  }

  const running = engine.status === "copying" || engine.status === "compressing";
  const showSummary = engine.summary != null && ["done", "cancelled", "error"].includes(engine.status);

  return (
    <div className="page-stack backup-page">
      <div className="page-toolbar">
        <div>
          <h1>备份</h1>
          <p>个人文件 · 开发环境 · 浏览器配置 · 自定义来源</p>
        </div>
        <div className="toolbar-status">
          <span className={`status-dot status-${engine.status}`} />
          <span>{engine.statusMessage}</span>
          <button type="button" className="btn-secondary" onClick={() => setOptionsOpen(true)} disabled={engine.busy}>
            备份设置
          </button>
        </div>
      </div>

      {running ? (
        <ProgressView progress={engine.progress} onCancel={engine.handleCancel} />
      ) : showSummary && engine.summary ? (
        <SummaryView summary={engine.summary} onReset={engine.resetToIdle} />
      ) : (
        <div className="workbench-layout">
          <SourceList
            items={config.items}
            disabled={engine.busy || config.loading}
            onToggleItem={config.toggleItem}
            onToggleCategory={config.toggleCategory}
            onToggleAll={config.toggleAll}
            onAddCustomFolder={addCustomFolder}
            onRemoveCustomItem={config.removeCustomItem}
            onRenameCustomItem={config.renameCustomItem}
            onClearCustomItems={config.clearCustomItems}
          />
          <TargetSelector
            targetRoot={config.targetRoot}
            items={config.items}
            scanResult={engine.scanResult}
            busy={engine.busy}
            disabled={engine.busy || config.loading}
            onPickTarget={pickTarget}
            onTargetChange={config.setTargetRoot}
            onScan={() => void engine.handleScan(config.items, config.targetRoot, config.derivedOptions)}
            onStartBackup={() => void engine.handleStartBackup(config.items, config.targetRoot, config.derivedOptions)}
          />
        </div>
      )}

      <OptionsPanel
        isOpen={optionsOpen}
        options={config.options}
        customExcludeText={config.customExcludeText}
        derivedOptions={config.derivedOptions}
        disabled={engine.busy}
        onClose={() => setOptionsOpen(false)}
        onToggleOption={config.toggleOption}
        onCustomTextChange={config.setCustomExcludeText}
        onRemovePattern={config.removePattern}
        onOptionsChange={config.setOptions}
      />
    </div>
  );
}
