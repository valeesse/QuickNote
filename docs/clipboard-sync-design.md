# 跨平台剪贴板同步设计

## 产品范围

首版参考 PasteNow 的“历史卡片、搜索、固定和一键回填”交互，但继续使用 QuickNote
现有的 React、Tauri、SQLite outbox、WebDAV 和版本向量，不引入第二套云端或账户系统。

支持文本、URL 和代码片段，单条上限 1 MB。图片、文件和富文本暂不采集；这些类型在
iOS、Android 与桌面平台的权限和数据表示差异较大，后续应复用现有内容寻址附件协议单独设计。

## 平台策略

| 平台 | 读取 | 写入 | 采集策略 |
| --- | --- | --- | --- |
| Windows | Tauri clipboard-manager | Tauri clipboard-manager | 窗口前台每 1.5 秒检查，可暂停 |
| macOS | Tauri clipboard-manager | Tauri clipboard-manager | 窗口前台每 1.5 秒检查，可暂停 |
| Android | Tauri Android clipboard binding | Tauri Android clipboard binding | 仅用户点击“读取当前剪贴板” |
| iOS | Tauri iOS clipboard binding | Tauri iOS clipboard binding | 仅用户点击读取，遵循系统粘贴提示 |

移动端不尝试后台监听，避免违反系统隐私限制和产生重复授权提示。全平台均使用同一组
Rust commands，前端不直接依赖平台 API。

## 数据与同步

- `clipboard_items.id` 是规范化文本的 SHA-256，跨设备天然去重。
- 类型仅用于展示：`text`、`link`、`code`。
- 固定、软删除、来源设备和采集次数随条目同步。
- `sync_changes.entity_type = clipboard`，envelope 继续使用 schema v2 版本向量。
- 首次连接 endpoint 时，现有剪贴板条目随 notes 和 attachments 一起 bootstrap。
- 并发固定/删除使用同一确定性因果裁决，不使用设备墙钟决定胜负。

## 隐私与可靠性

- 自动采集只在桌面窗口可见且聚焦时运行，并可在界面中暂停。
- 移动端始终要求显式用户动作。
- 平台命令保存当前内容指纹，应用自身复制不会形成采集反馈循环。
- 空内容被忽略；超出 1 MB 的文本拒绝入库。
- WebDAV 仅允许 HTTPS 配置，但远端 change 仍是服务端可读 JSON，不是端到端加密。
  对高敏感剪贴板数据，发布前应继续增加端到端加密或可配置排除规则。

## 验证边界

Windows 已完成 Rust 编译、浏览器端功能测试和响应式移动视口测试。官方
`tauri-plugin-clipboard-manager` 2.3.2 源码包含 Windows/macOS desktop backend 以及
Android/iOS mobile binding。当前 Windows 环境缺少 Android SDK/NDK，且不能运行 iOS
工具链，因此 Android/iOS/macOS 仍需各平台 CI 或真机完成最终打包和系统权限验证。
