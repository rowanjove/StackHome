import { useHistory } from "../hooks/useHistory";
import { invoke } from "@tauri-apps/api/core";
import type { OperationHistoryItem } from "../types/backup";

function operationLabel(type: string) {
  return { move: "移动", rename: "重命名", copy: "复制" }[type] ?? type;
}

function dateText(timestamp?: number | null) {
  return timestamp ? new Date(timestamp).toLocaleString("zh-CN", { hour12: false }) : "计划中";
}

export function HistoryPage() {
  const history = useHistory();

  async function undo(entry: OperationHistoryItem) {
    if (!window.confirm("撤销后会把文件移回原位置，是否继续？")) return;
    try {
      await invoke("operation_undo", { operationId: entry.id });
      await history.refresh();
    } catch (reason) {
      window.alert(String(reason));
    }
  }

  return (
    <div className="page-stack">
      <div className="page-toolbar"><div><h1>历史</h1><p>Operation Journal 记录已执行的文件变化；撤销遇到冲突时不会覆盖原文件。</p></div><button type="button" className="btn-secondary" onClick={() => void history.refresh()}>刷新</button></div>
      {history.error ? <div className="inline-error" role="alert">{history.error}</div> : null}
      <section className="table-panel">
        {history.loading ? <div className="loading-state">正在读取本地 Journal…</div> : history.entries.length === 0 ? <div className="empty-state"><h2>还没有操作记录</h2><p>执行一次整理或重命名后，操作记录会显示在这里。</p></div> : <div className="data-table-wrap"><table className="data-table history-table"><thead><tr><th>时间</th><th>操作</th><th>来源</th><th>目标</th><th>状态</th><th>操作</th></tr></thead><tbody>{history.entries.map((entry) => <tr key={entry.id}><td>{dateText(entry.executedAt)}</td><td>{operationLabel(entry.type)}</td><td title={entry.sourcePath}>{entry.sourcePath}</td><td title={entry.destinationPath ?? ""}>{entry.destinationPath || "—"}</td><td><span className={`status-badge ${entry.undoStatus === "undone" ? "undone" : entry.status}`}>{entry.undoStatus === "undone" ? "已撤销" : entry.error || entry.status}</span></td><td>{entry.status === "completed" && entry.undoStatus === "available" && ["move", "rename"].includes(entry.type) ? <button type="button" className="btn-link" onClick={() => void undo(entry)}>撤销</button> : <span className="muted-text">—</span>}</td></tr>)}</tbody></table></div>}
      </section>
    </div>
  );
}
