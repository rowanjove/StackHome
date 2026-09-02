import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { AutomationConfig, RuleRecord } from "../types/backup";

export type ThemeMode = "system" | "light" | "dark";

type SettingsPageProps = {
  theme: ThemeMode;
  onThemeChange: (theme: ThemeMode) => void;
};

const defaultAutomation: AutomationConfig = {
  watchEnabled: false,
  watchPath: null,
  watchDestinationRoot: null,
  watchRuleId: null,
  watchAutoApply: false,
  scheduledBackupEnabled: false,
  scheduledBackupIntervalMinutes: 60,
  minimizeToTray: false,
};

export function SettingsPage({ theme, onThemeChange }: SettingsPageProps) {
  const [automation, setAutomation] = useState<AutomationConfig>(defaultAutomation);
  const [rules, setRules] = useState<RuleRecord[]>([]);
  const [automationMessage, setAutomationMessage] = useState("正在读取自动化配置…");
  const [savingAutomation, setSavingAutomation] = useState(false);

  useEffect(() => {
    let mounted = true;
    Promise.all([
      invoke<AutomationConfig>("automation_get_config"),
      invoke<RuleRecord[]>("rules_list"),
    ])
      .then(([config, availableRules]) => {
        if (!mounted) return;
        setAutomation({ ...defaultAutomation, ...config });
        setRules(availableRules);
        setAutomationMessage("自动化配置已加载。");
      })
      .catch((error) => {
        if (!mounted) return;
        setAutomationMessage(`读取自动化配置失败：${String(error)}`);
      });
    return () => {
      mounted = false;
    };
  }, []);

  function updateAutomation<K extends keyof AutomationConfig>(
    key: K,
    value: AutomationConfig[K],
  ) {
    setAutomation((current) => ({ ...current, [key]: value }));
  }

  async function pickDirectory(key: "watchPath" | "watchDestinationRoot") {
    const selected = await open({
      directory: true,
      multiple: false,
      title: key === "watchPath" ? "选择监控目录" : "选择自动整理目标目录",
    });
    if (typeof selected === "string") updateAutomation(key, selected);
  }

  async function saveAutomation() {
    setSavingAutomation(true);
    try {
      const saved = await invoke<AutomationConfig>("automation_save_config", {
        config: automation,
      });
      setAutomation(saved);
      setAutomationMessage("自动化配置已保存并生效。");
    } catch (error) {
      setAutomationMessage(`保存自动化配置失败：${String(error)}`);
    } finally {
      setSavingAutomation(false);
    }
  }

  return (
    <div className="page-stack settings-page">
      <div className="page-toolbar">
        <div>
          <h1>设置</h1>
          <p>安全策略默认保持开启，配置保存在本机。</p>
        </div>
      </div>

      <section className="settings-section">
        <h2>外观</h2>
        <p>选择工作台的颜色模式。System 会跟随 Windows 设置。</p>
        <div className="radio-list">
          {(["system", "light", "dark"] as const).map((value) => (
            <label className="radio-row" key={value}>
              <input
                type="radio"
                name="theme"
                checked={theme === value}
                onChange={() => onThemeChange(value)}
              />
              <span>{value === "system" ? "系统" : value === "light" ? "浅色" : "深色"}</span>
            </label>
          ))}
        </div>
      </section>

      <section className="settings-section">
        <h2>文件扫描</h2>
        <p>Catalog 默认跳过隐藏文件、系统文件和安全排除目录。具体扫描页支持按次调整。</p>
        <div className="setting-state"><span>隐藏文件</span><strong>默认跳过</strong></div>
        <div className="setting-state"><span>系统文件</span><strong>默认跳过</strong></div>
      </section>

      <section className="settings-section">
        <h2>整理</h2>
        <p>所有 Rename / Move / Organize 操作必须先 Preview，且不提供关闭入口。</p>
        <div className="setting-state"><span>默认冲突策略</span><strong>自动编号</strong></div>
        <div className="setting-state"><span>永久删除</span><strong>默认禁止</strong></div>
      </section>

      <section className="settings-section automation-section">
        <div className="section-heading-row">
          <div>
            <h2>自动化</h2>
            <p>Watch Folder 发现稳定的新文件后先生成计划；只有明确开启可信规则自动应用才会执行移动。</p>
          </div>
          <span className="status-badge enabled">v0.6</span>
        </div>

        <div className="automation-grid">
          <label className="check-inline">
            <input
              type="checkbox"
              checked={automation.watchEnabled}
              onChange={(event) => updateAutomation("watchEnabled", event.target.checked)}
            />
            启用 Watch Folder
          </label>
          <label className="check-inline">
            <input
              type="checkbox"
              checked={automation.watchAutoApply}
              onChange={(event) => updateAutomation("watchAutoApply", event.target.checked)}
            />
            自动应用指定规则
          </label>
          <label className="control-field">
            <span>监控目录</span>
            <div className="input-with-action">
              <input value={automation.watchPath ?? ""} onChange={(event) => updateAutomation("watchPath", event.target.value || null)} placeholder="例如 C:\\Users\\me\\Downloads" />
              <button type="button" className="btn-secondary" onClick={() => void pickDirectory("watchPath")}>选择</button>
            </div>
          </label>
          <label className="control-field">
            <span>整理目标目录</span>
            <div className="input-with-action">
              <input value={automation.watchDestinationRoot ?? ""} onChange={(event) => updateAutomation("watchDestinationRoot", event.target.value || null)} placeholder="例如 D:\\Organized" />
              <button type="button" className="btn-secondary" onClick={() => void pickDirectory("watchDestinationRoot")}>选择</button>
            </div>
          </label>
          <label className="control-field">
            <span>自动规则</span>
            <select value={automation.watchRuleId ?? ""} onChange={(event) => updateAutomation("watchRuleId", event.target.value || null)}>
              <option value="">不指定规则（仅生成默认计划）</option>
              {rules.filter((rule) => rule.enabled).map((rule) => <option value={rule.id} key={rule.id}>{rule.name}</option>)}
            </select>
          </label>
          <label className="control-field compact-field">
            <span>定时备份间隔（分钟）</span>
            <input type="number" min={1} max={10080} value={automation.scheduledBackupIntervalMinutes} onChange={(event) => updateAutomation("scheduledBackupIntervalMinutes", Math.max(1, Number(event.target.value) || 1))} />
          </label>
        </div>

        <div className="automation-footer">
          <label className="check-inline">
            <input type="checkbox" checked={automation.scheduledBackupEnabled} onChange={(event) => updateAutomation("scheduledBackupEnabled", event.target.checked)} />
            启用定时备份（使用备份页当前来源、目标和选项）
          </label>
          <label className="check-inline">
            <input type="checkbox" checked={automation.minimizeToTray} onChange={(event) => updateAutomation("minimizeToTray", event.target.checked)} />
            关闭窗口时最小化到托盘
          </label>
          <button type="button" className="btn-primary" disabled={savingAutomation} onClick={() => void saveAutomation()}>{savingAutomation ? "保存中…" : "保存自动化设置"}</button>
        </div>
        <p className={automationMessage.includes("失败") ? "inline-error" : "inline-info"}>{automationMessage}</p>
      </section>

      <section className="settings-section">
        <h2>备份</h2>
        <p>备份选项中的压缩、排除、校验与通知会随备份配置保存。</p>
        <div className="setting-state"><span>本地处理</span><strong>已启用</strong></div>
      </section>

      <section className="settings-section">
        <h2>通知</h2>
        <p>备份完成通知在备份页的“备份设置”中配置；后台任务的当前状态始终显示在任务中心。</p>
        <div className="setting-state"><span>任务进度</span><strong>任务中心实时显示</strong></div>
        <div className="setting-state"><span>完成提醒</span><strong>由备份选项控制</strong></div>
      </section>

      <section className="settings-section">
        <h2>高级</h2>
        <p>工作台保持 Local First：索引、规则、Journal、Snapshot 和自动化配置都只写入本机应用数据目录。</p>
        <div className="setting-state"><span>数据库</span><strong>%APPDATA%\\WindowsEasyBackup\\app.db</strong></div>
        <div className="setting-state"><span>云端上传</span><strong>未启用</strong></div>
      </section>
    </div>
  );
}
