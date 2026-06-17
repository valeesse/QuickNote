# Markdown 解析与视图切换方案

## 方案比较

### 纯富文本编辑

优点是输入最快，和 TipTap/ProseMirror 的结构化文档模型完全一致，图片、任务列表和历史版本都容易维护。缺点是用户无法直接编辑 Markdown 源码，导入导出虽然可行，但对 Markdown 重度用户不够透明。

### 纯 Markdown 源码 + 预览

优点是 Markdown 语义最直接，和开发者习惯一致。缺点是图文混排、图片拖拽、任务列表状态、富文本快捷工具会变弱；每次预览都需要解析 Markdown，大文档下更容易出现输入与预览不同步。

### 三态视图：富文本 / 源码 / 预览

优点是保留便签应用的快速富文本输入，同时给 Markdown 用户源码编辑和只读预览；内部仍统一存储为 TipTap HTML，Markdown 解析和序列化由官方 `@tiptap/markdown` 承担，避免维护手写转换器。缺点是源码模式编辑时需要把 Markdown 解析回编辑器文档，超大文档会有额外成本。

## 采用方案

采用三态视图：

- 富文本：默认模式，适合高频快速记录。
- Markdown 源码：使用 `editor.getMarkdown()` 和 `setContent(markdown, { contentType: "markdown" })`。
- 预览：使用同一个 TipTap 文档，只读展示，避免预览与实际存储结果分叉。

这个方案最符合 QuickNote 的定位：本地优先、高性能、图文混排，同时完整支持 Markdown 导入、源码编辑、复制和导出。
