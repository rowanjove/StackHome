import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { formatBytes } from "../lib/backup-utils";
import type { ApplyPlanResult, DuplicateScanResult, FileRecord, PlanPreview, SimilarScanResult } from "../types/backup";

export function DuplicatesPage() {
  const [rootPath, setRootPath] = useState("");
  const [result, setResult] = useState<DuplicateScanResult | null>(null);
  const [similarResult, setSimilarResult] = useState<SimilarScanResult | null>(null);
  const [mode, setMode] = useState<"duplicate" | "similar">("duplicate");
  const [selected, setSelected] = useState<Record<string, boolean>>({});
  const [plan, setPlan] = useState<PlanPreview | null>(null);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);

  const selectedFiles = useMemo(
    () => (mode === "duplicate" ? result?.groups : similarResult?.groups)?.flatMap((group) => group.files.filter((file) => selected[file.path])) ?? [],
    [mode, result, selected, similarResult],
  );

  useEffect(() => {
    function selectAllVisibleFiles() {
      const groups = mode === "duplicate" ? result?.groups : similarResult?.groups;
      if (!groups) return;
      const next: Record<string, boolean> = {};
      groups.forEach((group) => group.files.forEach((file) => { next[file.path] = true; }));
      setSelected(next);
    }

    window.addEventListener("workspace-select-all", selectAllVisibleFiles);
    return () => window.removeEventListener("workspace-select-all", selectAllVisibleFiles);
  }, [mode, result, similarResult]);

  async function browse() {
    const value = await open({ directory: true, multiple: false, title: "选择重复项扫描目录" });
    if (typeof value === "string") setRootPath(value);
  }

  async function scan() {
    if (!rootPath.trim()) return;
    setBusy(true);
    setPlan(null);
    setMessage("");
    try {
      const next = await invoke<DuplicateScanResult>("duplicate_scan", {
        request: { rootPath: rootPath.trim(), includeHidden: false, includeSystemFiles: false, customExcludePatterns: [] },
      });
      setResult(next);
      setSimilarResult(null);
      const defaults: Record<string, boolean> = {};
      next.groups.forEach((group) => group.files.forEach((file, index) => { defaults[file.path] = index > 0; }));
      setSelected(defaults);
      setMessage(`扫描完成：发现 ${next.groups.length} 组重复文件。`);
    } catch (reason) {
      setMessage(String(reason));
    } finally {
      setBusy(false);
    }
  }

  async function scanSimilar() {
    if (!rootPath.trim()) return;
    setBusy(true);
    setPlan(null);
    setMessage("");
    try {
      const next = await invoke<SimilarScanResult>("similar_scan", { request: { rootPath: rootPath.trim(), maxDistance: 8, includeHidden: false } });
      setSimilarResult(next);
      const defaults: Record<string, boolean> = {};
      next.groups.forEach((group) => group.files.forEach((file, index) => { defaults[file.path] = index > 0; }));
      setSelected(defaults);
      setMessage(`相似图片分析完成：发现 ${next.groups.length} 组候选。`);
    } catch (reason) {
      setMessage(String(reason));
    } finally {
      setBusy(false);
    }
  }

  async function createPlan() {
    if (selectedFiles.length === 0) {
      setMessage("请至少选择一个要移至回收站的副本。");
      return;
    }
    setBusy(true);
    try {
      setPlan(await invoke<PlanPreview>(mode === "duplicate" ? "duplicate_create_plan" : "similar_create_plan", {
        request: { files: selectedFiles, reason: "重复项清理：用户确认后移至 Windows 回收站" },
      }));
      setMessage("计划已生成，请确认后执行。");
    } catch (reason) {
      setMessage(String(reason));
    } finally {
      setBusy(false);
    }
  }

  async function applyPlan() {
    if (!plan || plan.status !== "ready") return;
    if (!window.confirm(`将 ${plan.operations.length} 个文件移至 Windows 回收站，是否继续？`)) return;
    setBusy(true);
    try {
      const applied = await invoke<ApplyPlanResult>("organizer_apply_plan", { planId: plan.id });
      setPlan({ ...plan, status: applied.status, operations: applied.operations });
      setMessage(`执行完成：成功 ${applied.completed} 项，失败 ${applied.failed} 项。`);
    } catch (reason) {
      setMessage(String(reason));
    } finally {
      setBusy(false);
    }
  }

  function toggleFile(file: FileRecord) {
    setSelected((current) => ({ ...current, [file.path]: !current[file.path] }));
  }

  return (
    <div className="page-stack">
      <div className="page-toolbar"><div><h1>重复项</h1><p>{mode === "duplicate" ? "按大小预筛选，再用 BLAKE3 完整哈希确认。" : "用 8×8 感知哈希与汉明距离识别不同分辨率或重新编码的相似图片。"} 默认只移至 Windows 回收站。</p></div><div className="toolbar-summary">{mode === "duplicate" ? result ? `${result.groups.length} 组 · ${formatBytes(result.reclaimableSize)} 可回收` : "尚未扫描" : similarResult ? `${similarResult.groups.length} 组相似图片` : "尚未分析"}</div></div>
      <section className="toolbar-panel duplicate-toolbar"><div className="path-control"><label htmlFor="duplicate-root">扫描位置</label><input id="duplicate-root" value={rootPath} onChange={(event) => setRootPath(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void (mode === "duplicate" ? scan() : scanSimilar()); }} placeholder="例如 D:\\Downloads" /><button type="button" className="btn-secondary" onClick={() => void browse()}>浏览…</button><div className="segmented" role="group" aria-label="重复分析模式"><button type="button" className={mode === "duplicate" ? "selected" : ""} aria-pressed={mode === "duplicate"} onClick={() => setMode("duplicate")}>完全重复</button><button type="button" className={mode === "similar" ? "selected" : ""} aria-pressed={mode === "similar"} onClick={() => setMode("similar")}>相似图片</button></div><button type="button" className="btn-primary" onClick={() => void (mode === "duplicate" ? scan() : scanSimilar())} disabled={!rootPath.trim() || busy}>{busy ? "处理中…" : mode === "duplicate" ? "扫描重复项" : "分析相似图片"}</button></div></section>
      {message ? <div className={message.includes("失败") || message.includes("错误") ? "inline-error" : "inline-info"} role={message.includes("失败") || message.includes("错误") ? "alert" : "status"}>{message}</div> : null}
      {mode === "duplicate" && result && result.groups.length === 0 ? <div className="empty-state compact"><h2>没有确认的重复文件</h2><p>当前目录内没有大小与内容都相同的文件。</p></div> : null}
      {mode === "similar" && similarResult && similarResult.groups.length === 0 ? <div className="empty-state compact"><h2>没有相似图片候选</h2><p>当前目录内没有达到相似阈值的图片。</p></div> : null}
      {mode === "duplicate" && result && result.groups.length > 0 ? <section className="duplicate-groups" data-selectable-list>{result.groups.map((group) => <article className="table-panel duplicate-group" key={group.id}><div className="table-heading"><div><strong>{formatBytes(group.size)} · {group.files.length} 个副本</strong><span>Hash {group.hash.slice(0, 16)}…</span></div><span>可回收 {formatBytes(group.reclaimableSize)}</span></div><div className="duplicate-file-list">{group.files.map((file, index) => <label className="duplicate-file-row" key={file.path}><input type="checkbox" className="custom-checkbox" checked={Boolean(selected[file.path])} onChange={() => toggleFile(file)} /><span className="duplicate-keep">{index === 0 ? "建议保留" : "副本"}</span><span className="duplicate-path" title={file.path}>{file.path}</span></label>)}</div></article>)}</section> : null}
      {mode === "similar" && similarResult && similarResult.groups.length > 0 ? <section className="duplicate-groups" data-selectable-list>{similarResult.groups.map((group) => <article className="table-panel duplicate-group" key={group.id}><div className="table-heading"><div><strong>{group.files.length} 张相似图片</strong><span>汉明距离 {group.distance}</span></div><span>可回收 {formatBytes(group.reclaimableSize)}</span></div><div className="duplicate-file-list">{group.files.map((file, index) => <label className="duplicate-file-row" key={file.path}><input type="checkbox" className="custom-checkbox" checked={Boolean(selected[file.path])} onChange={() => toggleFile(file)} /><span className="duplicate-keep">{index === 0 ? "建议保留" : "候选"}</span><span className="duplicate-path" title={file.path}>{file.path}</span></label>)}</div></article>)}</section> : null}
      <section className="table-panel plan-panel"><div className="table-heading"><div><strong>清理计划</strong><span>{plan ? `${plan.operations.length} 项 · ${plan.status}` : `${selectedFiles.length} 个待处理副本`}</span></div><div className="panel-toolbar"><button type="button" className="btn-secondary" onClick={() => void createPlan()} disabled={busy || selectedFiles.length === 0}>生成预览</button>{plan?.status === "ready" ? <button type="button" className="btn-primary" onClick={() => void applyPlan()} disabled={busy}>移至回收站</button> : null}</div></div>{!plan ? <div className="empty-state compact"><h2>先扫描并选择副本</h2><p>计划生成前不会移动或删除任何文件。</p></div> : <div className="data-table-wrap"><table className="data-table"><thead><tr><th>文件</th><th>执行动作</th><th>状态</th></tr></thead><tbody>{plan.operations.map((operation) => <tr key={operation.id}><td title={operation.sourcePath}>{operation.sourcePath}</td><td>移至回收站</td><td><span className={`status-badge ${operation.status}`}>{operation.status}</span></td></tr>)}</tbody></table></div>}</section>
    </div>
  );
}
