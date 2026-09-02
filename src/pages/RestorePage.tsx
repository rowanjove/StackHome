import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { formatBytes } from "../lib/backup-utils";
import type { ApplyPlanResult, PlanPreview, PlannedOperation, SnapshotManifest, SnapshotRecord } from "../types/backup";
import { VirtualTable } from "../components/VirtualTable";

function localDate(value: number) { return new Date(value).toLocaleString("zh-CN", { hour12: false }); }

export function RestorePage() {
  const [snapshots, setSnapshots] = useState<SnapshotRecord[]>([]);
  const [snapshotId, setSnapshotId] = useState("");
  const [manifest, setManifest] = useState<SnapshotManifest | null>(null);
  const [selectedPaths, setSelectedPaths] = useState<string[]>([]);
  const [destinationMode, setDestinationMode] = useState<"original" | "specified">("original");
  const [destinationRoot, setDestinationRoot] = useState("");
  const [conflictPolicy, setConflictPolicy] = useState("manual");
  const [keepCount, setKeepCount] = useState(10);
  const [plan, setPlan] = useState<PlanPreview | null>(null);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);

  async function loadSnapshots() {
    try {
      const values = await invoke<SnapshotRecord[]>("snapshot_list", { limit: 200 });
      setSnapshots(values);
      if (values[0]) setSnapshotId(values[0].id);
    } catch (reason) { setMessage(String(reason)); }
  }

  useEffect(() => { void loadSnapshots(); }, []);
  useEffect(() => {
    if (!snapshotId) { setManifest(null); return; }
    void invoke<SnapshotManifest>("snapshot_manifest", { snapshotId }).then((value) => { setManifest(value); setSelectedPaths(value.files.map((file) => file.sourcePath)); }).catch((reason) => setMessage(String(reason)));
  }, [snapshotId]);
  useEffect(() => {
    function selectAllSnapshotFiles() {
      if (!manifest || plan) return;
      setSelectedPaths(manifest.files.map((file) => file.sourcePath));
    }

    window.addEventListener("workspace-select-all", selectAllSnapshotFiles);
    return () => window.removeEventListener("workspace-select-all", selectAllSnapshotFiles);
  }, [manifest, plan]);

  async function browse() {
    const value = await open({ directory: true, multiple: false, title: "选择恢复目标目录" });
    if (typeof value === "string") setDestinationRoot(value);
  }

  async function createPlan() {
    if (!snapshotId || (destinationMode === "specified" && !destinationRoot.trim())) {
      setMessage("请选择 Snapshot，并填写恢复目标目录。");
      return;
    }
    setBusy(true);
    try {
      if (selectedPaths.length === 0) { setMessage("请至少选择一个 Snapshot 文件。"); return; }
      const next = await invoke<PlanPreview>("restore_create_plan", { request: { snapshotId, sourcePaths: selectedPaths, destinationRoot: destinationMode === "specified" ? destinationRoot.trim() : null, conflictPolicy } });
      setPlan(next);
      setMessage(next.status === "ready" ? "恢复计划已生成，请确认预览。" : "计划包含冲突，请选择安全的冲突策略后重新生成。" );
    } catch (reason) { setMessage(String(reason)); } finally { setBusy(false); }
  }

  async function verify(mode: "fast" | "full") {
    if (!snapshotId) return;
    setBusy(true);
    try {
      const value = await invoke<{ status: string; failedFiles: number }>("snapshot_verify", { snapshotId, mode });
      setMessage(`${mode === "full" ? "完整" : "快速"}校验完成：${value.status}，失败 ${value.failedFiles} 项。`);
      await loadSnapshots();
    } catch (reason) { setMessage(String(reason)); } finally { setBusy(false); }
  }

  async function pruneSnapshots() {
    const jobId = snapshots.find((snapshot) => snapshot.id === snapshotId)?.backupJobId;
    if (!jobId) { setMessage("当前 Snapshot 没有关联的备份方案，无法按方案清理。"); return; }
    if (!window.confirm(`只保留该备份方案最新 ${keepCount} 个 Snapshot，并删除更旧的备份目录，是否继续？`)) return;
    setBusy(true);
    try {
      const removed = await invoke<number>("snapshot_prune", { jobId, keep: keepCount });
      setMessage(`已清理 ${removed} 个旧 Snapshot。`);
      await loadSnapshots();
    } catch (reason) { setMessage(String(reason)); } finally { setBusy(false); }
  }

  async function applyPlan() {
    if (!plan || plan.status !== "ready") return;
    if (!window.confirm(`将恢复 ${plan.operations.length} 个文件，是否继续？`)) return;
    setBusy(true);
    try {
      const applied = await invoke<ApplyPlanResult>("organizer_apply_plan", { planId: plan.id });
      setPlan({ ...plan, status: applied.status, operations: applied.operations });
      setMessage(`恢复完成：成功 ${applied.completed} 项，失败 ${applied.failed} 项。`);
    } catch (reason) { setMessage(String(reason)); } finally { setBusy(false); }
  }

  const previewRows: PlannedOperation[] = plan?.operations ?? manifest?.files.map((file, index) => ({
    id: `restore-preview-${index}-${file.sourcePath}`,
    type: "restore",
    sourcePath: file.sourcePath,
    destinationPath: file.backupPath,
    reason: "Snapshot 恢复预览",
    status: "selected",
  })) ?? [];

  return <div className="page-stack"><div className="page-toolbar"><div><h1>恢复</h1><p>从 Snapshot 选择内容，先预览目标与冲突，再写回原位置或指定目录。</p></div><div className="toolbar-summary">{manifest ? `${selectedPaths.length}/${manifest.files.length} 项 · ${formatBytes(manifest.files.filter((file) => selectedPaths.includes(file.sourcePath)).reduce((sum, file) => sum + file.size, 0))}` : "尚未选择"}</div></div><section className="toolbar-panel restore-controls"><div className="control-field"><label htmlFor="snapshot-select">Snapshot</label><select id="snapshot-select" value={snapshotId} onChange={(event) => { setSnapshotId(event.target.value); setPlan(null); }}><option value="">选择 Snapshot</option>{snapshots.map((snapshot) => <option key={snapshot.id} value={snapshot.id}>{localDate(snapshot.snapshotTime)} · {snapshot.fileCount} 项 · {snapshot.status}</option>)}</select></div><div className="control-field"><label>恢复位置</label><div className="radio-list"><label className="radio-row"><input type="radio" checked={destinationMode === "original"} onChange={() => setDestinationMode("original")} />原位置</label><label className="radio-row"><input type="radio" checked={destinationMode === "specified"} onChange={() => setDestinationMode("specified")} />指定目录</label></div></div>{destinationMode === "specified" ? <div className="control-field wide"><label htmlFor="restore-destination">目标目录</label><div className="inline-input"><input id="restore-destination" value={destinationRoot} onChange={(event) => setDestinationRoot(event.target.value)} placeholder="例如 D:\\Restored" /><button type="button" className="btn-secondary" onClick={() => void browse()}>浏览…</button></div></div> : null}<div className="control-field"><label htmlFor="restore-conflict">冲突策略</label><select id="restore-conflict" value={conflictPolicy} onChange={(event) => setConflictPolicy(event.target.value)}><option value="manual">手动确认（默认）</option><option value="auto_number">自动编号</option><option value="skip">跳过已存在文件</option></select></div><div className="panel-toolbar align-end"><button type="button" className="btn-secondary" onClick={() => void verify("fast")} disabled={busy || !snapshotId}>快速校验</button><button type="button" className="btn-secondary" onClick={() => void verify("full")} disabled={busy || !snapshotId}>完整校验</button><button type="button" className="btn-primary" onClick={() => void createPlan()} disabled={busy || !snapshotId || selectedPaths.length === 0}>生成恢复预览</button></div></section>{message ? <div className={message.includes("失败") || message.includes("错误") ? "inline-error" : "inline-info"} role={message.includes("失败") || message.includes("错误") ? "alert" : "status"}>{message}</div> : null}<section className="table-panel plan-panel"><div className="table-heading"><div><strong>恢复预览</strong><span>{plan ? `${plan.operations.length} 项 · ${plan.status}` : "先选择需要恢复的文件"}</span></div><div className="panel-toolbar"><label className="compact-field">保留 <input type="number" min={1} max={999} value={keepCount} onChange={(event) => setKeepCount(Math.max(1, Number(event.target.value) || 1))} /> 个</label><button type="button" className="btn-secondary" onClick={() => void pruneSnapshots()} disabled={busy || !snapshotId}>清理旧 Snapshot</button>{plan?.status === "ready" ? <button type="button" className="btn-primary" onClick={() => void applyPlan()} disabled={busy}>确认恢复</button> : null}</div></div>{!manifest ? <div className="empty-state compact"><h2>选择 Snapshot 开始</h2><p>原位置恢复不会覆盖现有文件；指定目录会保留原目录结构。</p></div> : <div data-selectable-list><VirtualTable items={previewRows} rowKey={(operation) => operation.sourcePath} columnCount={4} ariaLabel="恢复计划预览" headers={<tr><th scope="col">选择</th><th scope="col">备份文件</th><th scope="col">恢复目标</th><th scope="col">状态</th></tr>} renderRow={(operation) => <tr key={operation.sourcePath}><td><input type="checkbox" aria-label={`选择恢复文件 ${operation.sourcePath}`} checked={plan ? true : selectedPaths.includes(operation.sourcePath)} disabled={Boolean(plan)} onChange={() => setSelectedPaths((paths) => paths.includes(operation.sourcePath) ? paths.filter((path) => path !== operation.sourcePath) : [...paths, operation.sourcePath])} /></td><td title={operation.sourcePath}>{operation.sourcePath}</td><td title={operation.destinationPath ?? ""}>{operation.destinationPath}</td><td><span className={`status-badge ${operation.status}`}>{"conflict" in operation && operation.conflict?.message ? operation.conflict.message : operation.status === "selected" ? "待恢复" : operation.status}</span></td></tr>} /></div>}</section></div>;
}
