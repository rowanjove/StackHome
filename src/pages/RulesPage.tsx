import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { RuleAction, RuleDefinition, RuleRecord, RuleSource } from "../types/backup";

type ConditionRow = { field: string; operator: string; value: string };
const fields = [["category", "类别"], ["extension", "扩展名"], ["filename", "文件名"], ["mime", "MIME"], ["sourceType", "来源"], ["size", "大小（字节）"], ["created", "创建时间"], ["modified", "修改时间"], ["parent", "父目录"], ["image.width", "图片宽度"], ["image.height", "图片高度"], ["exif.date", "EXIF 日期"], ["exif.camera", "相机"], ["audio.artist", "音频艺术家"], ["audio.album", "音频专辑"], ["audio.title", "音频标题"], ["audio.track", "音轨"]];
const operators = [["equals", "等于"], ["not_equals", "不等于"], ["contains", "包含"], ["starts_with", "开头是"], ["ends_with", "结尾是"], ["regex", "正则"], ["greater_than", "大于"], ["less_than", "小于"], ["greater_or_equals", "大于等于"], ["less_or_equals", "小于等于"]];

function blankRow(): ConditionRow { return { field: "category", operator: "equals", value: "image" }; }
function fromCondition(condition: Record<string, unknown>): ConditionRow[] {
  const values = Array.isArray(condition.all) ? condition.all : Array.isArray(condition.any) ? condition.any : [condition.not ?? condition];
  return values.filter((value): value is Record<string, unknown> => Boolean(value && typeof value === "object" && "field" in value)).map((value) => ({ field: String(value.field ?? "category"), operator: String(value.operator ?? "equals"), value: String(value.value ?? "") })) || [blankRow()];
}

export function RulesPage() {
  const [rules, setRules] = useState<RuleRecord[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [name, setName] = useState("");
  const [priority, setPriority] = useState(100);
  const [sourceType, setSourceType] = useState("");
  const [pathContains, setPathContains] = useState("");
  const [combine, setCombine] = useState<"all" | "any">("all");
  const [negate, setNegate] = useState(false);
  const [conditions, setConditions] = useState<ConditionRow[]>([blankRow()]);
  const [actionType, setActionType] = useState<RuleAction["type"]>("move");
  const [destination, setDestination] = useState("");
  const [renameTemplate, setRenameTemplate] = useState("");
  const [tags, setTags] = useState("");
  const [message, setMessage] = useState("");

  async function load() {
    try { setRules(await invoke<RuleRecord[]>("rules_list")); } catch (reason) { setMessage(String(reason)); }
  }
  useEffect(() => { void load(); }, []);

  function reset() {
    setSelectedId(""); setName(""); setPriority(100); setSourceType(""); setPathContains(""); setCombine("all"); setNegate(false); setConditions([blankRow()]); setActionType("move"); setDestination(""); setRenameTemplate(""); setTags("");
  }

  function selectRule(rule: RuleRecord) {
    setSelectedId(rule.id); setName(rule.name); setPriority(rule.priority); setSourceType(rule.definition.source?.sourceType ?? ""); setPathContains(rule.definition.source?.pathContains ?? ""); setConditions(fromCondition(rule.definition.condition)); setCombine(Array.isArray(rule.definition.condition.any) ? "any" : "all"); setNegate(Boolean(rule.definition.condition.not)); setActionType(rule.definition.action.type); setDestination(rule.definition.action.destinationTemplate ?? ""); setRenameTemplate(rule.definition.action.renameTemplate ?? ""); setTags(rule.definition.action.tags.join(", "));
  }

  async function save() {
    if (!name.trim()) { setMessage("请输入规则名称。"); return; }
    const leaves = conditions.map((condition) => ({ field: condition.field, operator: condition.operator, value: ["size", "image.width", "image.height"].includes(condition.field) ? Number(condition.value) : condition.value }));
    const base = { [combine]: leaves };
    const condition = negate ? { not: base } : base;
    const current = rules.find((rule) => rule.id === selectedId);
    const source: RuleSource | null = sourceType.trim() || pathContains.trim() ? { sourceType: sourceType.trim() || null, pathContains: pathContains.trim() || null } : null;
    const definition: RuleDefinition = { source, condition, action: { type: actionType, destinationTemplate: destination.trim() || null, renameTemplate: renameTemplate.trim() || null, tags: tags.split(",").map((value) => value.trim()).filter(Boolean) } };
    try { const saved = await invoke<RuleRecord>("rules_save", { rule: { id: selectedId || `rule-${Date.now()}`, name: name.trim(), enabled: current?.enabled ?? true, priority, ruleType: "organize", definition, createdAt: current?.createdAt ?? 0, updatedAt: current?.updatedAt ?? 0 } }); setRules((items) => items.some((item) => item.id === saved.id) ? items.map((item) => item.id === saved.id ? saved : item) : [...items, saved]); setSelectedId(saved.id); setMessage("规则已保存。"); } catch (reason) { setMessage(String(reason)); }
  }

  async function remove(rule: RuleRecord) {
    if (rule.id.startsWith("builtin-")) { setMessage("内置规则不能删除，请关闭它。"); return; }
    try { await invoke("rules_delete", { ruleId: rule.id }); setRules((items) => items.filter((item) => item.id !== rule.id)); if (selectedId === rule.id) reset(); setMessage("规则已删除。"); } catch (reason) { setMessage(String(reason)); }
  }

  async function toggle(rule: RuleRecord) {
    try { const saved = await invoke<RuleRecord>("rules_save", { rule: { ...rule, enabled: !rule.enabled } }); setRules((items) => items.map((item) => item.id === saved.id ? saved : item)); } catch (reason) { setMessage(String(reason)); }
  }

  return <div className="page-stack"><div className="page-toolbar"><div><h1>规则</h1><p>用 AND / OR / NOT 组合条件，再通过 Planner 预览移动、复制、重命名、标签或忽略动作。</p></div><button type="button" className="btn-primary" onClick={reset}>新建规则</button></div>{message ? <div className={message.includes("失败") || message.includes("错误") ? "inline-error" : "inline-info"}>{message}</div> : null}<section className="rules-layout"><div className="table-panel"><div className="table-caption">规则列表 · 优先级低者先执行</div><div className="rule-list">{rules.map((rule) => <div className={`rule-row ${selectedId === rule.id ? "selected" : ""}`} key={rule.id} onClick={() => selectRule(rule)}><div><strong>{rule.name}</strong><span>{rule.id.startsWith("builtin-") ? "内置" : "自定义"} · priority {rule.priority}</span></div><div className="rule-actions"><span className={`status-badge ${rule.enabled ? "enabled" : "skipped"}`}>{rule.enabled ? "启用" : "关闭"}</span><button type="button" className="btn-ghost" onClick={(event) => { event.stopPropagation(); void toggle(rule); }}>{rule.enabled ? "关闭" : "启用"}</button>{!rule.id.startsWith("builtin-") ? <button type="button" className="btn-ghost btn-danger-ghost" onClick={(event) => { event.stopPropagation(); void remove(rule); }}>删除</button> : null}</div></div>)}</div></div><section className="panel rule-editor"><div className="panel-header"><div><h2>{selectedId ? "编辑规则" : "新建规则"}</h2><p className="panel-desc">规则只在生成计划时生效，不会绕过预览阶段。</p></div></div><div className="rule-form"><div className="condition-header"><strong>来源</strong><span className="panel-desc">可选限制条件</span></div><div className="condition-row"><input value={sourceType} onChange={(event) => setSourceType(event.target.value)} placeholder="来源类型，例如 downloads" /><input value={pathContains} onChange={(event) => setPathContains(event.target.value)} placeholder="路径包含，例如 Screenshots" /></div><div className="condition-header"><strong>规则</strong><label className="compact-field">优先级 <input type="number" min={-9999} max={9999} value={priority} onChange={(event) => setPriority(Number(event.target.value) || 0)} /></label><select value={combine} onChange={(event) => setCombine(event.target.value as "all" | "any")}><option value="all">全部满足（AND）</option><option value="any">任一满足（OR）</option></select><label className="check-inline"><input type="checkbox" checked={negate} onChange={(event) => setNegate(event.target.checked)} />NOT 取反</label></div>{conditions.map((condition, index) => <div className="condition-row" key={index}><select value={condition.field} onChange={(event) => setConditions((items) => items.map((item, i) => i === index ? { ...item, field: event.target.value } : item))}>{fields.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select><select value={condition.operator} onChange={(event) => setConditions((items) => items.map((item, i) => i === index ? { ...item, operator: event.target.value } : item))}>{operators.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select><input value={condition.value} onChange={(event) => setConditions((items) => items.map((item, i) => i === index ? { ...item, value: event.target.value } : item))} placeholder="条件值" />{conditions.length > 1 ? <button type="button" className="btn-ghost" onClick={() => setConditions((items) => items.filter((_, i) => i !== index))}>移除</button> : null}</div>)}<button type="button" className="btn-ghost align-left" onClick={() => setConditions((items) => [...items, blankRow()])}>+ 添加条件</button><div className="condition-header"><strong>动作</strong><select value={actionType} onChange={(event) => setActionType(event.target.value as RuleAction["type"])}><option value="move">移动</option><option value="copy">复制</option><option value="rename">重命名</option><option value="tag">添加标签</option><option value="ignore">忽略</option></select></div>{actionType === "move" || actionType === "copy" ? <label className="control-field"><span>目标模板</span><input value={destination} onChange={(event) => setDestination(event.target.value)} placeholder="Pictures\\{{year}}" /></label> : null}{actionType === "rename" ? <label className="control-field"><span>命名模板</span><input value={renameTemplate} onChange={(event) => setRenameTemplate(event.target.value)} placeholder="{{year}}-{{seq:03}}" /></label> : null}{actionType === "tag" ? <label className="control-field"><span>标签（逗号分隔）</span><input value={tags} onChange={(event) => setTags(event.target.value)} placeholder="待整理, 图片" /></label> : null}<div className="rule-form-actions"><button type="button" className="btn-secondary" onClick={reset}>清空</button><button type="button" className="btn-primary" onClick={() => void save()}>保存规则</button></div></div></section></section></div>;
}
