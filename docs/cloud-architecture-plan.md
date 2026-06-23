# 云端架构与实施计划

## 数据权威

系统只有两种运行模式，禁止三方多主同步：

| 模式 | 权威存储 | 附件存储 | WebDAV 行为 |
| --- | --- | --- | --- |
| local | SQLite | 本地目录 + WebDAV | 桌面双向增量同步 |
| cloud_primary | Postgres | SeaweedFS | 服务端单向镜像/备份 |

切换到 `cloud_primary` 后，桌面端不再直接运行 WebDAV 同步。桌面和 Web 都通过 Cloud API 修改 Postgres；所有写入在同一数据库事务内追加完整、不可变的 change envelope。

正文统一使用 `attachment://{sha256}`。本地模式从本地目录解析，云模式通过鉴权 API 从 SeaweedFS 解析。WebDAV 与 OSS 均使用内容哈希作为不可变对象 ID。

## 初始化状态机

```text
local -> import_preview -> importing -> cloud_primary
                         -> failed -> import_preview
cloud_primary -> export_snapshot -> cloud_primary
```

- 首次初始化先执行只读预检，返回便签、剪贴板、附件、冲突和预计空间。
- 导入作业使用 generation ID 和逐设备 cursor，允许安全重试。
- 只有空云账户可以执行自动导入；非空账户必须选择合并或清空后导入。
- 初始化完成后写入不可逆的 authority barrier。日常同步不得再次读取 WebDAV 作为输入。
- “重新从 WebDAV 导入”是显式管理操作，必须预览并确认，不属于普通同步。

## 当前完成

- 云端写入同时更新 Postgres 权威表和完整同步 envelope。
- 云上传使用用户、来源设备和来源序号幂等确认，客户端按明确序号清理 outbox。
- 服务端按实体持久化版本向量，合并桌面因果历史后生成规范化的 `cloud` envelope，避免跨端连续编辑被误判为并发。
- 桌面云认证区分密码与 JWT，并缓存短期 JWT。
- 云模式和桌面直连 WebDAV 模式互斥。
- SeaweedFS 上传下载接入，使用 SHA-256、大小和用户边界校验。
- Web 编辑器正文改用逻辑附件引用，不再持久化 Base64。
- Web 自动保存改为按便签隔离的串行队列，切换、删除和隐藏前 flush。
- TypeScript 领域模型收敛至 `packages/contracts`；Rust 实体、版本向量、冲突关系和 envelope 收敛至 `crates/quicknote-protocol`。

## 后续工作边界

服务端 WebDAV 镜像与一次性导入需要独立 worker。worker 持有加密后的 WebDAV 凭据，消费云 change log，把 envelope 和 SeaweedFS 对象写入现有不可变目录。该 worker 不得写回 Postgres，导入 worker 除外。

前端继续收敛为共享应用核心：`NoteRepository`、`AttachmentRepository` 和平台能力接口；桌面使用 Tauri adapter，Web 使用 HTTP adapter。TipTap 编辑器、草稿队列、列表和剪贴板 UI 应移动到共享 package，平台壳只负责认证、托盘和窗口生命周期。
