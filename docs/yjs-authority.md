# Yjs 正文权威模型

## 权威边界

- `notes.yjs_state` 是正文的唯一权威状态。
- `notes.content`、`title` 和 `search_text` 是从 Yjs 编辑器派生的读模型。
- HTML 投影用于列表、搜索、历史、导出和旧客户端兼容，不用于覆盖已有 Yjs 文档。
- 标签、固定、排序、删除和附件元数据仍使用版本向量同步。

## 初始化

旧便签第一次进入协同编辑器时，服务端通过 30 秒 bootstrap lease 指定唯一客户端把
HTML 迁入空 Yjs 文档，避免多个客户端同时初始化导致内容重复。客户端必须先收到服务端
Yjs 源和 bootstrap 控制消息，之后才能渲染或迁移 HTML。

## 编辑与投影

Yjs update 通过 WebSocket 立即发送；断线 update 在内存中排队，重连并合并服务端完整状态
后补发。HTML 投影在客户端防抖生成，并通过同一 WebSocket 顺序发送。服务端只在对应便签
已有 Yjs 状态时接受投影，同时更新标题、搜索文本、版本历史和同步 outbox。

桌面本地模式在一次 SQLite 事务中保存完整 Yjs state、HTML 投影、版本和 outbox。WebDAV
或云同步遇到并发 Yjs state 时执行 CRDT 合并；仅旧的非 Yjs 正文继续使用冲突副本策略。

## 当前协同范围

协同面向同一账号下的多个 Web 或桌面实例，不包含团队权限、共享空间、计费和移动端。
Web 实例使用实时 WebSocket；桌面本地实例通过本地持久化和同步 provider 合并状态。
