import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useCatalog } from "../hooks/useCatalog";
import { formatBytes } from "../lib/backup-utils";
import type { FileRecord } from "../types/backup";
import { VirtualTable } from "../components/VirtualTable";

const categories = [
  ["", "全部"],
  ["image", "图片"],
  ["video", "视频"],
  ["audio", "音频"],
  ["document", "文档"],
  ["archive", "压缩包"],
  ["installer", "安装程序"],
  ["code", "代码"],
] as const;

function localDate(timestamp?: number | null) {
  return timestamp ? new Date(timestamp).toLocaleString("zh-CN", { hour12: false }) : "—";
}

export function FilesPage() {
  const catalog = useCatalog();
  const [rootPath, setRootPath] = useState("");
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState("");
  const [selected, setSelected] = useState<FileRecord | null>(null);

  async function browse() {
    const selectedPath = await open({ directory: true, multiple: false, title: "选择要建立索引的目录" });
    if (typeof selectedPath === "string") setRootPath(selectedPath);
  }

  async function refresh(nextCategory = category, nextSearch = search) {
    await catalog.query({
      search: nextSearch,
      rootPath: rootPath || null,
      category: nextCategory || null,
      limit: 10_000,
      offset: 0,
    });
  }

  async function scan() {
    if (!rootPath.trim()) return;
    await catalog.scan({
      rootPath: rootPath.trim(),
      sourceType: "custom",
      includeHidden: false,
      includeSystemFiles: false,
      customExcludePatterns: [],
    });
  }

  const totalBytes = catalog.files.reduce((sum, file) => sum + file.size, 0);

  return (
    <div className="page-stack">
      <div className="page-toolbar">
        <div>
          <h1>文件</h1>
          <p>Catalog 只保存文件索引与元数据，不上传或复制文件内容。</p>
        </div>
        <div className="toolbar-summary">{catalog.files.length} 项 · {formatBytes(totalBytes)}</div>
      </div>

      <section className="toolbar-panel">
        <div className="path-control">
          <label htmlFor="catalog-root">索引位置</label>
          <input
            id="catalog-root"
            value={rootPath}
            onChange={(event) => setRootPath(event.target.value)}
            onKeyDown={(event) => { if (event.key === "Enter") void scan(); }}
            placeholder="例如 C:\\Users\\你\\Downloads"
          />
          <button type="button" className="btn-secondary" onClick={browse}>浏览…</button>
          <button type="button" className="btn-primary" onClick={() => void scan()} disabled={!rootPath.trim() || catalog.scanning}>
            {catalog.scanning ? "扫描中…" : "扫描并建立索引"}
          </button>
        </div>
        <div className="filter-row">
          <label htmlFor="catalog-search">搜索</label>
          <input
            id="catalog-search"
            className="filter-search"
            data-workspace-search
            aria-label="搜索文件名或路径"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            onKeyDown={(event) => { if (event.key === "Enter") void refresh(); }}
            placeholder="文件名或路径"
          />
          <div className="segmented" role="group" aria-label="文件类型">
            {categories.map(([value, label]) => (
              <button
                type="button"
                key={value}
                className={category === value ? "selected" : ""}
                onClick={() => {
                  setCategory(value);
                  void refresh(value, search);
                }}
              >
                {label}
              </button>
            ))}
          </div>
          <button type="button" className="btn-ghost" onClick={() => void refresh()}>刷新</button>
        </div>
      </section>

      {catalog.error ? <div className="inline-error" role="alert">{catalog.error}</div> : null}
      {catalog.scanResult?.warnings.map((warning) => <div className="inline-warning" key={warning}>{warning}</div>)}

      <section className="catalog-layout">
        <div className="table-panel">
          <div className="table-caption">文件列表 · 最近修改优先</div>
          {catalog.files.length === 0 ? (
            <div className="empty-state compact">
              <h2>还没有索引文件</h2>
              <p>选择一个目录开始扫描。扫描只读取元数据，原文件不会被修改。</p>
            </div>
          ) : (
            <VirtualTable
              items={catalog.files}
              rowKey={(file) => file.id}
              columnCount={4}
              rowHeight={58}
              ariaLabel="文件列表"
              headers={<tr><th scope="col">名称</th><th scope="col">类型</th><th scope="col">大小</th><th scope="col">修改时间</th></tr>}
              renderRow={(file) => (
                <tr
                  key={file.id}
                  className={selected?.id === file.id ? "selected" : ""}
                  tabIndex={0}
                  role="button"
                  aria-selected={selected?.id === file.id}
                  onClick={() => setSelected(file)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      setSelected(file);
                    }
                  }}
                >
                  <td><div className="file-name-cell">{file.filename}<small>{file.path}</small></div></td>
                  <td>{file.category}</td>
                  <td>{formatBytes(file.size)}</td>
                  <td>{localDate(file.modifiedAt)}</td>
                </tr>
              )}
            />
          )}
        </div>

        <aside className="inspector-panel" aria-label="文件详情">
          {selected ? (
            <>
              <div className="inspector-title">文件详情</div>
              <h2>{selected.filename}</h2>
              <dl className="detail-list">
                <dt>路径</dt><dd>{selected.path}</dd>
                <dt>类型</dt><dd>{selected.mime || selected.category}</dd>
                <dt>大小</dt><dd>{formatBytes(selected.size)}</dd>
                <dt>创建时间</dt><dd>{localDate(selected.createdAt)}</dd>
                <dt>修改时间</dt><dd>{localDate(selected.modifiedAt)}</dd>
                <dt>来源</dt><dd>{selected.sourceType || "未标记"}</dd>
                {selected.metadata?.width && selected.metadata.height ? <><dt>尺寸</dt><dd>{selected.metadata.width} × {selected.metadata.height}</dd></> : null}
                {selected.metadata?.creationTime ? <><dt>媒体创建</dt><dd>{selected.metadata.creationTime}</dd></> : null}
                {selected.metadata?.cameraModel ? <><dt>相机</dt><dd>{selected.metadata.cameraMake ? `${selected.metadata.cameraMake} ` : ""}{selected.metadata.cameraModel}</dd></> : null}
                {selected.metadata?.artist ? <><dt>艺术家</dt><dd>{selected.metadata.artist}</dd></> : null}
                {selected.metadata?.album ? <><dt>专辑</dt><dd>{selected.metadata.album}</dd></> : null}
                {selected.metadata?.durationSeconds != null ? <><dt>时长</dt><dd>{selected.metadata.durationSeconds} 秒</dd></> : null}
                {selected.tags.length > 0 ? <><dt>标签</dt><dd>{selected.tags.join("、")}</dd></> : null}
                {selected.metadata?.extensionMismatch ? <><dt>提示</dt><dd className="text-danger">扩展名与文件内容类型不一致</dd></> : null}
              </dl>
            </>
          ) : (
            <div className="empty-inspector">选择一行查看文件详情</div>
          )}
        </aside>
      </section>
    </div>
  );
}
