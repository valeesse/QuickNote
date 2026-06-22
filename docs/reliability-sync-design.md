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
- WebDAV 使用 `attachments/{sha256}`，相同内容天然去重。
- 清理时同时扫描当前便签和历史版本，避免删除仍被历史引用的文件。

## 同步协议

本地 `sync_changes` 是 provider-neutral outbox。远端 change envelope 包含：

- `schema_version`
- `device_id`
- 单调递增的设备序列号
- 实体类型、实体 ID、操作和修改时间
- `causal_version`：按设备计数的版本向量与确定性来源设备
- 当前便签、附件或剪贴板条目元数据

schema v2 使用版本向量判断 `dominates / dominated / concurrent`，`changed_at` 和
`updated_at` 仅用于展示与列表排序。并发版本以来源设备 ID 和规范化计数器做稳定
tie-break；失败一方生成内容寻址的冲突副本。旧 schema v1 change 会转换为
`{ device_id: sequence }` 的单设备向量，因此无需重写已有远端目录。

WebDAV provider 使用不可变路径：

```text
changes/{device-id}/{sequence}.json
attachments/{sha256}
```

同步先拉取再推送。这样本地未同步修改仍可参与冲突判断。远端较新且本地有未同步内容时，先生成“冲突副本”，再应用远端版本，禁止静默覆盖。

首次连接某个 provider endpoint 时执行 bootstrap，把已有便签、附件与剪贴板条目加入 outbox。每个远端设备使用独立 cursor，成功应用 change 后才推进 cursor。

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
| WebDAV 增量同步 | 已完成 | pull 后 push、endpoint 隔离 cursor、首次 bootstrap、不可变 change 路径与幂等重试。 |
| 冲突副本 | 已完成 | 远端更新/删除遇到本地未推送内容时保留本地冲突副本。 |
| 同步编辑边界与 UI 一致性 | 已完成 | 同步期间锁定编辑；完成后同时刷新列表与当前编辑器，远端删除会关闭当前便签。 |
| 协议与并发防护 | 已完成 | 校验 schema/device/sequence/entity，后端串行化同步任务。 |
| 崩溃级 draft 恢复 | 已完成 | 未落盘正文同步写入版本化 localStorage journal；启动后重建串行队列，成功落盘才清理。 |
| WebDAV 兼容与故障注入 | 已完成 | 覆盖真实 HTTP live smoke、207/命名空间/编码 href、幂等 PUT、确认丢失、超时、损坏 change 与附件校验。 |
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
