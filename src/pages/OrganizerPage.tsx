import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { useCatalog } from "../hooks/useCatalog";
import { formatBytes } from "../lib/backup-utils";
import type { ApplyPlanResult, PlanPreview, RuleRecord } from "../types/backup";
import { VirtualTable } from "../components/VirtualTable";

export function OrganizerPage() {
  const catalog = useCatalog();
  const [sourcePath, setSourcePath] = useState("");
  const [destinationRoot, setDestinationRoot] = useState("");
  const [operationType, setOperationType] = useState<"move" | "copy" | "rename">("move");
  const [conflictPolicy, setConflictPolicy] = useState("auto_number");
  const [renameTemplate, setRenameTemplate] = useState("");
  const [rules, setRules] = useState<RuleRecord[]>([]);
  const [ruleId, setRuleId] = useState("");
  const [plan, setPlan] = useState<PlanPreview | null>(null);
  const [message, setMessage] = useState("");
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    void invoke<RuleRecord[]>("rules_list").then(setRules).catch(() => undefined);
  }, []);

  async function browse(kind: "source" | "destination") {
    const selected = await open({ directory: true, multiple: false, title: kind === "source" ? "选择整理来源" : "选择整理目标" });
    if (typeof selected !== "string") return;
    if (kind === "source") setSourcePath(selected);
    else setDestinationRoot(selected);
  }

  async function scanSource() {
    if (!sourcePath.trim()) return;
    setPlan(null);
    setMessage("");
    await catalog.scan({ rootPath: sourcePath.trim(), sourceType: "downloads" });
  }

  async function createPlan() {
    if (catalog.files.length === 0) {
      setMessage("请先扫描来源目录。" );
      return;
    }
    if (!destinationRoot.trim()) {
      setMessage("请先选择目标目录。" );
      return;
    }
    setCreating(true);
    setMessage("");
    try {
      const nextPlan = await invoke<PlanPreview>("organizer_create_plan", {
        request: {
          files: catalog.files,
          destinationRoot: destinationRoot.trim(),
          operationType,
          renameTemplate: renameTemplate.trim() || null,
          conflictPolicy,
          reason: "按整理页生成的预览计划",
          ruleId: ruleId || null,
        },
      });
      setPlan(nextPlan);
      setMessage(nextPlan.status === "ready" ? `已生成 ${nextPlan.operations.length} 项计划，请确认预览。` : "计划包含需要处理的冲突或无效项。" );
    } catch (reason) {
      setMessage(String(reason));
    } finally {
      setCreating(false);
    }
  }

  async function applyPlan() {
    if (!plan || plan.status !== "ready") return;
    if (!window.confirm(`即将应用 ${plan.operations.length} 项文件变化，是否继续？`)) return;
    try {
      const result = await invoke<ApplyPlanResult>("organizer_apply_plan", { planId: plan.id });
      setMessage(`计划执行完成：成功 ${result.completed} 项，失败 ${result.failed} 项。` );
      setPlan({ ...plan, status: result.status, operations: result.operations });
    } catch (reason) {
      setMessage(String(reason));
    }
  }

  return (
    <div className="page-stack">
      <div className="page-toolbar">
        <div><h1>整理</h1><p>扫描 → 生成计划 → 预览 → 应用。整理器不会绕过 Planner 直接修改文件。</p></div>
        <div className="toolbar-summary">{catalog.files.length} 项 · {formatBytes(catalog.files.reduce((sum, file) => sum + file.size, 0))}</div>
      </div>

      <section className="organizer-controls">
        <div className="control-field wide"><label htmlFor="organizer-source">来源</label><div className="inline-input"><input id="organizer-source" value={sourcePath} onChange={(event) => setSourcePath(event.target.value)} placeholder="例如 Downloads" /><button type="button" className="btn-secondary" onClick={() => void browse("source")}>浏览…</button></div></div>
        <button type="button" className="btn-secondary align-end" onClick={() => void scanSource()} disabled={!sourcePath.trim() || catalog.scanning}>{catalog.scanning ? "扫描中…" : "扫描"}</button>
        <div className="control-field wide"><label htmlFor="organizer-destination">目标目录</label><div className="inline-input"><input id="organizer-destination" value={destinationRoot} onChange={(event) => setDestinationRoot(event.target.value)} placeholder="例如 D:\\Pictures" /><button type="button" className="btn-secondary" onClick={() => void browse("destination")}>浏览…</button></div></div>
        <div className="control-field"><label htmlFor="organizer-action">操作</label><select id="organizer-action" value={operationType} onChange={(event) => setOperationType(event.target.value as typeof operationType)}><option value="move">移动</option><option value="copy">复制</option><option value="rename">重命名</option></select></div>
        <div className="control-field"><label htmlFor="organizer-conflict">冲突策略</label><select id="organizer-conflict" value={conflictPolicy} onChange={(event) => setConflictPolicy(event.target.value)}><option value="auto_number">自动编号（默认）</option><option value="sequence">按序号避让</option><option value="skip">跳过</option><option value="keep_newer">保留更新时间较新者</option><option value="keep_larger">保留文件较大者</option><option value="manual">手动确认</option><option value="replace">替换（仍需确认）</option></select></div>
        <div className="control-field template-field"><label htmlFor="organizer-template">命名模板（可选）</label><input id="organizer-template" value={renameTemplate} onChange={(event) => setRenameTemplate(event.target.value)} placeholder="{{year}}-{{seq:03}}" /></div>
        <div className="control-field"><label htmlFor="organizer-rule">整理规则</label><select id="organizer-rule" value={ruleId} onChange={(event) => setRuleId(event.target.value)}><option value="">手动整理</option>{rules.filter((rule) => rule.enabled).sort((a, b) => a.priority - b.priority).map((rule) => <option key={rule.id} value={rule.id}>{rule.name}</option>)}</select></div>
        <button type="button" className="btn-primary align-end" onClick={() => void createPlan()} disabled={creating || catalog.scanning}>{creating ? "生成中…" : "生成预览计划"}</button>
      </section>

      {catalog.error || message ? <div className={catalog.error ? "inline-error" : "inline-info"} role={catalog.error ? "alert" : "status"}>{catalog.error || message}</div> : null}

      <section className="table-panel plan-panel">
        <div className="table-heading"><div><strong>计划预览</strong><span>{plan ? `${plan.operations.length} 项 · 状态 ${plan.status}` : "尚未生成"}</span></div>{plan?.status === "ready" ? <button type="button" className="btn-primary" onClick={() => void applyPlan()}>应用 {plan.operations.length} 项更改</button> : null}</div>
        {!plan ? <div className="empty-state compact"><h2>先扫描一个来源目录</h2><p>所有文件变化都会先出现在这里，确认后才会执行。</p></div> : (
          <VirtualTable
            items={plan.operations}
            rowKey={(operation) => operation.id}
            columnCount={3}
            ariaLabel="整理计划预览"
            headers={<tr><th scope="col">原路径</th><th scope="col">目标路径</th><th scope="col">状态</th></tr>}
            renderRow={(operation) => <tr key={operation.id}><td title={operation.sourcePath}>{operation.sourcePath}</td><td title={operation.destinationPath ?? ""}>{operation.destinationPath || "—"}</td><td><span className={`status-badge ${operation.status}`}>{operation.conflict?.message || operation.status}</span></td></tr>}
          />
        )}
      </section>
    </div>
  );
}
