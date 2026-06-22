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
| 非法 change JSON | provider 故障注入 | 同步失败，设备 cursor 不推进 |
| 附件缺失、路径穿越、大小或 SHA-256 不符 | provider/单元测试 | 拒绝落盘，cursor 不越过失败 change |
| schema v1 / v2 | 协议单元测试 | v1 映射单设备向量，v2 强制携带版本向量 |

## 外部服务验证入口

`sync::webdav::tests::live_webdav_smoke_test` 默认忽略。设置以下环境变量后可对
Nextcloud、标准 WebDAV 或厂商服务运行同一组 MKCOL/PROPFIND/PUT/GET 验证：

```text
QUICKNOTE_WEBDAV_TEST_URL
QUICKNOTE_WEBDAV_TEST_USERNAME
QUICKNOTE_WEBDAV_TEST_PASSWORD
```

仓库内 [webdav-server.mjs](../tests/fixtures/webdav-server.mjs) 是无外部依赖的 CI fixture。
厂商 endpoint 的认证、配额与限流策略仍应在发布前使用真实账号执行 smoke test；自动化
协议测试不等同于厂商认证。
