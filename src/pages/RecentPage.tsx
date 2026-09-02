import type { WorkspacePage } from "../types/workspace";
import { useHistory } from "../hooks/useHistory";

type RecentPageProps = { onNavigate: (page: WorkspacePage) => void };

function operationLabel(type: string) { return { move: "移动", rename: "重命名", copy: "复制", recycle: "移至回收站", restore: "恢复", tag: "添加标签" }[type] ?? type; }

export function RecentPage({ onNavigate }: RecentPageProps) {
  const history = useHistory(12);
  const actions: Array<{ index: string; title: string; description: string; page: WorkspacePage }> = [
    { index: "01", title: "收整下载目录", description: "扫描、拟定去向，确认后再移动", page: "organizer" },
    { index: "02", title: "找出重复文件", description: "按内容核对，不凭文件名猜测", page: "duplicates" },
    { index: "03", title: "留一份本地备份", description: "预检容量，生成可核验的 Snapshot", page: "backup" },
    { index: "04", title: "从备份中取回", description: "选择内容与位置，先看冲突再恢复", page: "restore" },
  ];
  return (
    <div className="page-stack">
      <section className="recent-intro">
        <div className="recent-kicker">STACKHOME · LOCAL FILE WORKSPACE</div>
        <div className="recent-intro-copy">
          <h1>把散落的文件，<br />安静地放回位置。</h1>
          <p>扫描只建立索引；整理、清理和恢复都先给你看一遍。</p>
        </div>
        <div className="recent-principles" aria-label="产品原则">
          <span><b>01</b> 本机处理</span>
          <span><b>02</b> 先看后改</span>
          <span><b>03</b> 留有退路</span>
        </div>
      </section>
      <section className="quick-actions" aria-label="快速操作">
        {actions.map((action) => (
          <button type="button" key={action.index} onClick={() => onNavigate(action.page)}>
            <span className="quick-index">{action.index}</span>
            <strong>{action.title}</strong>
            <span>{action.description}</span>
            <span className="quick-arrow" aria-hidden="true">↗</span>
          </button>
        ))}
      </section>
      <section className="table-panel recent-panel"><div className="table-heading"><div><strong>最近操作</strong><span>本地 Journal</span></div><button type="button" className="btn-link" onClick={() => onNavigate("history")}>查看全部</button></div>{history.entries.length === 0 ? <div className="empty-state compact"><h2>还没有操作记录</h2><p>整理或重命名文件后，这里会显示真实操作。</p></div> : <div className="recent-list">{history.entries.slice(0, 8).map((entry) => <div className="recent-row" key={entry.id}><div><strong>{operationLabel(entry.type)}</strong><span>{entry.sourcePath}</span></div><span className={`status-badge ${entry.status}`}>{entry.undoStatus === "undone" ? "已撤销" : entry.status}</span></div>)}</div>}</section>
    </div>
  );
}
