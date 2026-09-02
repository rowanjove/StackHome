# 归栈

一个中文优先、Local First 的 Windows 本地文件工作台。名字取“让散落文件回到该在的位置”：它帮助你发现文件、建立索引、生成整理计划，并在确认预览后执行可恢复的文件变化；备份仍然保留为核心保护能力。

界面采用纸张、索引签与档案柜的视觉语言，以炭墨、暖纸和陶土橙构成品牌色，不使用常见的蓝紫渐变、玻璃卡片或“AI 魔法”意象。应用图标由叠放文件与归档槽组成，在小尺寸下仍保持清晰轮廓。

## 当前能力

- 文件 Catalog：扫描目录、读取基础元数据、按名称/路径/类型查询。
- 整理 Planner：规则支持 AND / OR / NOT、移动/复制/重命名/标签/忽略、命名模板、metadata 变量、预览和冲突策略。
- Executor + Journal：只有执行器修改文件，记录成功/失败操作，支持 rename / move Undo；重复文件默认移至 Windows 回收站。
- 重复与相似：按大小、前 64 KiB 预 Hash、BLAKE3 全 Hash 查找完全重复；用感知 Hash + 汉明距离查找相似图片。
- Metadata：读取扩展名与 magic 类型异常、图片尺寸/EXIF/GPS、音频标签与时长、MP4 时长/尺寸/创建时间。
- 备份 2.0：保留桌面、文档、下载、图片、视频、音乐、微信、QQ、Chrome、Edge、SSH、VS Code、自定义目录、排除、空间预检、取消、ZIP、7Z、报告和通知，并提供 Backup Job、Snapshot、Manifest、增量、快速/完整校验、恢复和安全保留清理。
- 自动化：Watch Folder 发现新文件后生成计划，可对指定规则显式自动应用；支持后台定时备份、Windows 托盘和关闭到托盘。
- 任务中心：统一 `task-progress`、`task-completed`、`task-error` 事件，扫描和备份不阻塞 WebView。
- 大目录体验：目录与计划预览使用窗口化表格，支持 10,000 条结果；Ctrl+F 聚焦搜索，Ctrl+A 全选可选列表，Esc 关闭对话框。
- 配置兼容：读取旧配置并迁移到当前格式；索引、任务和历史数据只保存在应用本地数据目录。

当前版本：v0.6.1，已完成“归栈”品牌、图标和桌面界面重构。PDF/DOCX 正文抽取、OCR 和智能分类尚未提供。

为保证原用户无感升级，应用标识与既有本地数据目录继续沿用；这些兼容信息不会作为对外产品名显示。

## 安全模型

扫描、索引、分析和生成计划都不会修改文件。批量操作必须经过 Preview；执行前重新检查源文件 size / mtime。跨盘移动遵循 Copy → Verify → Journal → Delete Source，Undo 遇到原位置冲突不会覆盖文件。应用不上传文件、不要求登录，也不默认永久删除。

## 从源码运行

```bash
npm install
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
npm run tauri dev
```

## 构建 Windows 版本

```bash
npm run build:windows:portable
npm run build:windows:installer
```
