# WebDAV 兼容与故障注入矩阵

## 自动化覆盖

| 场景 | 层级 | 预期 |
| --- | --- | --- |
| MKCOL / PROPFIND / PUT / GET | 本地真实 HTTP fixture | 完整 live smoke 通过 |
| 207 Multi-Status、DAV 命名空间、绝对 href | HTTP mock | 正确提取目录项 |
| 百分号编码 href | XML 解析 | UTF-8 名称正确解码 |
| `If-None-Match: *` 返回 412 | HTTP mock | 远端内容相同视为幂等成功，不同则报冲突 |
| PUT 已落远端但确认丢失 | provider 故障注入 | outbox 保留；重试同一路径后才标记 synced |
| WebDAV 响应超时 | HTTP mock | 30 秒生产超时；测试短超时可控失败 |
| 高延迟/间歇断网 | v4 head 最后提交 + 指数退避 | 未提交 root 不可见，outbox 保持可重试 |
| 10万次同实体编辑 | root/shard/object + 两阶段 GC | 回收后占用与当前状态相关，不随历史次数增长 |
| 上传完成但 head 丢确认 | 内容寻址幂等提交 | 重试识别已提交对象并安全确认 outbox |
| 新设备恢复 | 当前 heads/roots | 无需历史 generation 即可恢复 |
| 非法或缺失对象 | SHA-256 校验 | 同步失败，设备 cursor 不推进 |
| 附件缺失、路径穿越、大小或 SHA-256 不符 | provider/单元测试 | 拒绝落盘，cursor 不越过失败对象 |
| 两阶段 GC | 当前可达图 + 7天 marker | 上传中的孤儿不会被立即删除，重新可达会撤销 marker |
| 旧 v1-v3 目录 | 破坏性升级 | v4 客户端完全忽略；迁移时先备份再清空远端根目录 |

## 外部服务验证入口

`sync::tests::transfer::live_v4_end_to_end_and_gc` 默认忽略。设置以下环境变量后可对
Nextcloud、标准 WebDAV 或厂商服务运行同一组 MKCOL/PROPFIND/PUT/GET 验证：

```text
QUICKNOTE_WEBDAV_TEST_URL
QUICKNOTE_WEBDAV_TEST_USERNAME
QUICKNOTE_WEBDAV_TEST_PASSWORD
```

仓库内 [webdav-server.mjs](../tests/fixtures/webdav-server.mjs) 是无外部依赖的 CI fixture。
厂商 endpoint 的认证、配额与限流策略仍应在发布前使用真实账号执行 smoke test；自动化
协议测试不等同于厂商认证。
