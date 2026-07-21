# 编辑可靠性与同步设计

## 实施顺序

1. 本地编辑先进入按便签隔离的 draft 队列。
2. 每个便签的保存请求严格串行，旧 revision 完成后不得覆盖新 revision 的 UI。
3. 切换、删除、窗口关闭和同步前必须 flush；失败时保留 draft 并阻止关闭。
4. SQLite 在同一事务中更新正文、版本快照和 `sync_changes` outbox。
5. 同步只消费已落盘数据，不直接读取 TipTap 临时状态。

## 附件协议

- 数据库正文只保存 `attachment://{sha256}`。
- TipTap 显示时把逻辑 ID 解析为当前设备的 asset URL。
- 附件表保存 SHA-256 ID、相对路径、MIME、大小和创建时间。
- WebDAV v4 对小附件使用 `v4/attachments/{prefix}/{sha256}.bin`；超过 4 MiB 时写入 `v4/attachment-manifests/` 和 1 MiB 内容寻址块 `v4/attachment-chunks/`，支持幂等重试并对相同块去重。
- 清理时同时扫描当前便签和历史版本，避免删除仍被历史引用的文件。

## 同步协议

本地 `sync_changes` 是 provider-neutral outbox，成功提交后立即裁剪已确认行。远端实体对象包含：

- `schema_version`
- `device_id`
- 单调递增的设备序列号
- 实体类型、实体 ID、操作和修改时间
- `causal_version`：按设备计数的版本向量与确定性来源设备
- 当前便签、附件或剪贴板条目元数据

schema v2 envelope 使用版本向量判断 `dominates / dominated / concurrent`，`changed_at` 和
`updated_at` 仅用于展示与列表排序。并发版本以来源设备 ID 和规范化计数器做稳定
tie-break；失败一方生成内容寻址的冲突副本。并发 Yjs 合并结果会作为新的本地变更再次
发布，确保所有设备最终观察到同一个 joined causal frontier。

WebDAV 生产路径为破坏性 v4，不读取或写入旧 `state/changes/device-heads`。v4 使用内容寻址
Merkle 状态：

- `v4/workspace.json`：固定协议、workspace ID 和 epoch。
- `v4/heads/{device_id}.json`：每个设备独占写入的提交指针。
- `v4/roots/{prefix}/{hash}.json.gz`：指向256个实体 shard 的不可变 root。
- `v4/shards/{prefix}/{hash}.json.gz`：实体 key 到当前对象 hash 的索引。
- `v4/objects/{prefix}/{hash}.json.gz`：完整当前实体；Yjs 二进制使用 Base64 而非 JSON 数字数组。
- `v4/attachments/{prefix}/{sha256}.bin`：小型内容寻址附件。
- `v4/attachment-manifests/` 与 `v4/attachment-chunks/`：大附件清单和 1 MiB 内容寻址块。
- `v4/gc/candidates/`：两阶段垃圾回收标记。

```text
head -> root -> shard -> entity object -> attachment
```

同步按“推送本地、拉取远端、发布合并结果”执行。本地未同步修改参与因果冲突判断；
远端较新且本地有并发内容时保留确定性冲突副本，禁止静默覆盖。

首次连接 v4 workspace 时执行 bootstrap。新设备只需读取当前 heads/roots，不需要任何历史
generation。未被当前 head 引用的对象先标记，至少7天后再次确认仍不可达才删除；上传中断
产生的孤儿不会被当场误删。自动 GC 每24小时最多运行一次，设置页也可手动触发安全扫描。

## CouchDB 扩展

同步核心只依赖 `SyncProvider` 的 `prepare/list/get/put` 能力。CouchDB provider 可继续复用 change envelope：

- change envelope 映射为 CouchDB 文档。
- 附件可映射为 CouchDB attachment 或独立内容寻址文档。
- `_changes` 序列替代 WebDAV 目录扫描和本地设备 cursor。
- CouchDB revision 冲突最终仍交给现有冲突副本策略处理。

编辑器、SQLite outbox 和附件协议无需因 provider 更换而修改。

## 实现程度（2026-06-22）

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| 按便签串行保存、revision 防回滚 | 已完成 | 新输入不会被较早完成的保存覆盖；失败保留内存 draft 并定时重试。 |
| 切换、删除、隐藏、关闭、同步前 flush | 已完成 | 保存失败会中止切换/删除/同步；Tauri 关闭请求会被阻止。 |
| 正文、版本快照、outbox 原子提交 | 已完成 | SQLite 事务覆盖创建、编辑、删除、恢复、固定和版本恢复。 |
| 内容寻址附件 | 已完成 | 使用 SHA-256 ID；同步拉取会校验路径、大小和内容哈希后原子落盘。 |
| WebDAV 有界同步 | 已完成 | v4 内容寻址 root/shard、endpoint/workspace/epoch 隔离 cursor、首次 bootstrap、gzip、幂等提交。 |
| 冲突副本 | 已完成 | 远端更新/删除遇到本地未推送内容时保留本地冲突副本。 |
| 同步编辑边界与 UI 一致性 | 已完成 | 同步期间锁定编辑；完成后同时刷新列表与当前编辑器，远端删除会关闭当前便签。 |
| 协议与并发防护 | 已完成 | 校验 schema/device/sequence/entity，后端串行化同步任务。 |
| 崩溃级 draft 恢复 | 已完成 | 未落盘正文同步写入版本化 localStorage journal；启动后重建串行队列，成功落盘才清理。 |
| WebDAV 兼容与故障注入 | 已完成 | 覆盖真实 HTTP v4 E2E、207/命名空间/编码 href、head 确认丢失、超时、对象 hash 与附件校验。 |
| 无墙钟冲突排序 | 已完成 | schema v2 使用版本向量和确定性 tie-break；±百年时间戳及编辑/删除并发测试均收敛。 |
| 跨平台剪贴板同步 | 已完成 | 文本、链接和代码片段使用内容寻址去重，复用 outbox/版本向量；桌面前台采集，移动端主动读取。 |

## 后续开发计划

### 阶段一：崩溃恢复（已完成）

- 编辑时同步更新 `quicknote-draft-journal-v1`，只记录逻辑正文，不复制附件二进制。
- 启动时恢复已有便签草稿；原便签已不存在时创建恢复副本。
- 保存失败继续保留 journal，成功保存、版本恢复或永久删除后清理。
- 验收：debounce 前 reload 可恢复；连续保存失败不丢草稿；附件 hydration 不产生临时空草稿。

### 阶段二：WebDAV 兼容与故障注入（已完成）

- 内置最小标准 WebDAV live fixture，并提供连接外部 WebDAV/Nextcloud 的 ignored smoke test。
- 注入 PUT 成功但本地未确认、207/编码差异、超时和损坏 change/附件。
- 验收结果：重试不重复、不跳 cursor、不静默覆盖；本地真实 HTTP 流程及 Nextcloud 风格响应测试通过。

### 阶段三：无墙钟冲突排序（已完成）

- change envelope 已升级为向后兼容的版本向量；v1 自动映射为单设备向量。
- 墙钟仅用于展示，冲突胜负由因果关系和确定性 tie-break 决定。
- 验收结果：模拟时间戳前后漂移约百年，编辑/编辑与编辑/删除均收敛并保留冲突副本。

CouchDB provider 仍只是协议扩展点，待上述可靠性基线稳定后再实现。
