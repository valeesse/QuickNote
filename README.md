# QuickNote

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.5-orange?logo=tauri)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-1.88+-000?logo=rust)](https://www.rust-lang.org)
[![React](https://img.shields.io/badge/React-18-61dafb?logo=react)](https://react.dev)

> **轻量级效率便签 · 本地优先 · 多端同步**
>
> 不是另一个笔记应用，而是你桌面角落里随时待命的效率搭档。

[🌐 官网](https://valeesse.github.io/QuickNote/) · [⬇️ 下载桌面端](#下载安装) · [📖 文档](#文档) · [🐛 反馈问题](https://github.com/valeesse/QuickNote/issues)

---

## ✨ 为什么选择 QuickNote？

市面上不缺笔记应用，但大多数都在往「知识库」的方向堆功能。QuickNote 反其道而行——**只做便签和剪贴板**，把它们做到极致。

| 你可能遇到的问题 | QuickNote 的解法 |
|---|---|
| 复制了一段文字，切换窗口后找不到了 | 📋 **剪贴板自动捕获**，历史内容随时回溯 |
| 想记个临时想法，打开笔记应用太重 | ⚡ **全局快捷键** `Ctrl+Alt+N` 一键呼出便签弹窗 |
| 便签窗口太碍眼，关了就没了 | 🪟 **失焦自动隐藏**，常驻系统托盘，用完即走 |
| 多台电脑之间同步很麻烦 | 🔄 **WebDAV / 云端双模式同步**，按需选择 |
| 担心数据隐私 | 🔒 **本地 SQLite 存储 + AES-256-GCM 加密**，数据完全自主 |

## 🚀 核心功能

### 📝 智能便签

- 基于 Tiptap 的富文本编辑器，**输入即渲染** Markdown
- 支持任务列表、代码块、图片粘贴、高亮标注
- 版本历史自动保存，支持回滚和置顶
- 回收站软删除，误删可恢复

### 📋 剪贴板管理

- 自动捕获系统剪贴板，支持文本和图片
- 全文搜索、置顶固定、智能去重
- 一键将剪贴板条目转为便签
- 跨设备剪贴板同步

### ⌨️ 全局快捷键

| 快捷键 | 操作 |
|---|---|
| `Ctrl+Alt+N` | 新建快速便签 |
| `Ctrl+Alt+C` | 打开剪贴板历史 |
| `Ctrl+Alt+Q` | 打开快速便签弹窗 |

> 关闭主窗口会隐藏到系统托盘；使用托盘菜单的"退出"才会结束进程。

### 🔄 双模式同步

- **WebDAV 模式** — 本地 SQLite 存储，通过 WebDAV 增量同步，兼容 NAS（如 fnOS）、坚果云等
- **云端模式** — PostgreSQL + SeaweedFS，Docker 一键自部署，支持 Web 端访问
- 两种模式互斥，避免多主冲突

### 🔐 安全与隐私

- 本地数据使用 SQLite 存储，离线完全可用
- 凭据 AES-256-GCM 加密，密码不入库
- 云端部署仅通过 HTTPS 通信
- 内容寻址附件，持久草稿，幂等同步

## 📸 截图

> 📌 截图待补充——添加应用主界面、剪贴板面板、设置页等截图到 `docs/screenshots/` 目录后更新此处。

<!-- 
<div align="center">
  <img src="docs/screenshots/main-window.png" alt="主界面" width="600" />
  <p><em>主界面：便签列表 + 富文本编辑器</em></p>
</div>

<div align="center">
  <img src="docs/screenshots/clipboard-panel.png" alt="剪贴板" width="600" />
  <p><em>剪贴板历史面板</em></p>
</div>

<div align="center">
  <img src="docs/screenshots/quick-popup.png" alt="快捷弹窗" width="400" />
  <p><em>全局快捷键呼出便签弹窗</em></p>
</div>
-->

## 📥 下载安装

> 当前项目处于活跃开发阶段，尚未发布正式安装包。你可以从源码构建，或关注 [Releases](https://github.com/valeesse/QuickNote/releases) 页面获取最新动态。

### 从源码构建

**环境要求：** Node.js 20+、Rust 1.88+

**Windows：** Microsoft Edge WebView2、Visual Studio C++ Build Tools

```powershell
# 克隆仓库
git clone https://github.com/valeesse/QuickNote.git
cd QuickNote

# 安装依赖并启动开发模式
npm --prefix apps/desktop install
npm run tauri -- dev
```

**构建安装包：**

```powershell
npm run tauri -- build
```

构建结果位于 `apps/desktop/src-tauri/target/release/bundle/`。

## 🏗️ 项目结构

```text
apps/
  desktop/                 桌面端 (Tauri + React + SQLite)
  server/                  云端 API (Axum + PostgreSQL + SeaweedFS)
  web/                     Web 前端 (React + Nginx)
crates/
  quicknote-protocol/      共享同步协议与版本向量
packages/
  contracts/               TypeScript 共享领域类型
  styles/                  共享样式
  tailwind-preset/         共享 Tailwind 预设
  ui/                      共享 UI 组件
docs/                      架构设计与技术文档
site/                      项目官网 (GitHub Pages)
tests/                     跨模块集成测试
docker-compose.yml         云端部署编排
```

## ☁️ 云端自部署

适合团队或多设备场景，一键部署 Web 版：

```powershell
# 1. 创建环境文件
Copy-Item .env.example .env

# 2. 编辑 .env，设置强密钥
#    POSTGRES_PASSWORD=<随机强密码>
#    JWT_SECRET=<至少32字节随机字符串>
#    PUBLIC_ORIGIN=https://notes.example.com

# 3. 构建并启动
docker compose up -d --build
```

默认 Web 入口为 `http://localhost:8081`。PostgreSQL、SeaweedFS 和 API 不直接暴露到宿主机，由 Web nginx 统一代理 `/api`。

```powershell
# 查看日志
docker compose logs -f api web

# 停止服务
docker compose down

# 彻底删除开发数据
docker compose down -v
```

## 🛠️ 开发指南

### 桌面端开发

```powershell
npm --prefix apps/desktop install
npm run tauri -- dev          # 完整开发（前端 + Rust）
npm run dev:desktop           # 仅启动前端
```

### Web 前端开发

先启动云端 API，再运行：

```powershell
npm --prefix apps/web install
npm run dev:web
```

默认 Vite 地址为 `http://localhost:5173`，`/api` 代理到 `http://localhost:3000`。

### 验证与测试

```powershell
npm run build:desktop         # 桌面端构建检查
npm run build:web             # Web 端构建检查
npm run test:desktop-rust     # Rust 单元测试
npm run test:server           # 服务端测试
npm run test:protocol         # 协议层测试
npm run test:e2e              # E2E 端到端测试
```

基础设施集成测试使用覆盖文件：

```powershell
docker compose -f docker-compose.yml -f tests/docker-compose.test.yml up -d
```

## 📊 技术栈一览

| 层级 | 技术 |
|---|---|
| 桌面框架 | Tauri 2.5 (Rust) |
| 前端 UI | React 18 + TypeScript 5.7 + Tailwind CSS v4 |
| 编辑器 | Tiptap + Yjs 协同编辑 |
| 本地存储 | SQLite (rusqlite) |
| 云端 API | Axum + PostgreSQL + SeaweedFS |
| Web 部署 | Nginx + Docker |
| 构建工具 | Vite 6 |
| CI/CD | GitHub Actions |

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！

- 🐛 [报告 Bug](https://github.com/valeesse/QuickNote/issues/new?labels=bug)
- 💡 [功能建议](https://github.com/valeesse/QuickNote/issues/new?labels=enhancement)
- 📖 [查看 Roadmap](https://github.com/valeesse/QuickNote/projects)

## 📄 许可证

[Apache License 2.0](LICENSE) — 可自由使用、修改和分发。

---

<div align="center">

**如果 QuickNote 对你有帮助，请给我们一个 ⭐ Star！**

[⭐ Star](https://github.com/valeesse/QuickNote) · [🐛 Issues](https://github.com/valeesse/QuickNote/issues) · [🌐 官网](https://valeesse.github.io/QuickNote/)

</div>
# QuickNote

[QuickNote](https://valeesse.github.io/QuickNote/) 是一个本地优先、支持云端协作的高性能便签应用。桌面端使用 Tauri + React + SQLite；云端由 Axum、PostgreSQL、SeaweedFS 和 React Web 组成。

## 功能

- 富文本、Markdown 快捷输入、任务列表、图片与版本历史
- 系统托盘、失焦隐藏弹窗和全局快捷键
- 本地模式：SQLite 与用户 WebDAV 双向增量同步
- 云端模式：桌面端和 Web 通过 Cloud API，以 PostgreSQL/SeaweedFS 为权威存储
- 内容寻址附件、持久草稿、同步幂等和因果冲突处理

## 项目结构

```text
apps/
  desktop/                 桌面 React、Tauri/Rust 与 E2E
  server/                  Axum API、迁移和服务端镜像
  web/                     Web React 与 nginx
crates/
  quicknote-protocol/      Rust 共享实体、版本向量和同步协议
packages/
  contracts/               TypeScript 共享领域类型
docs/                      架构、同步与兼容性设计
tests/                     跨模块 Docker 测试覆盖与共享 WebDAV fixture
docker-compose.yml         云端生产式编排
```

## 环境要求

- Node.js 20+
- Rust 1.88+，建议使用仓库当前锁文件对应的稳定版
- Windows 桌面开发：Microsoft Edge WebView2、Visual Studio C++ Build Tools
- Docker Desktop / Docker Engine 与 Compose v2

## 桌面端开发

```powershell
npm --prefix apps/desktop install
npm run tauri -- dev
```

只启动桌面前端：

```powershell
npm run dev:desktop
```

构建安装包：

```powershell
npm run tauri -- build
```

构建结果位于 `apps/desktop/src-tauri/target/release/bundle/`。

### 快捷键

| 快捷键 | 操作 |
| --- | --- |
| `Ctrl+Alt+N` | 新建快速便签 |
| `Ctrl+Alt+C` | 打开剪贴板历史 |
| `Ctrl+Alt+Q` | 打开快速便签 |

关闭主窗口会隐藏到系统托盘；使用托盘菜单的“退出”才会结束进程。

### 同步模式

- **本地/WebDAV 模式**：数据保存在本机 SQLite，附件位于本地目录并同步到 WebDAV。
- **云端模式**：桌面端使用 HTTPS Cloud API；PostgreSQL 和 SeaweedFS 是唯一权威来源。
- 两种模式互斥，避免 SQLite、WebDAV、云数据库形成三方多主。

## Web 前端开发

先启动云端 API，再运行：

```powershell
npm --prefix apps/web install
npm run dev:web
```

默认 Vite 地址为 `http://localhost:5173`，`/api` 会代理到 `http://localhost:3000`。

## 云端部署

1. 创建环境文件：

```powershell
Copy-Item .env.example .env
```

2. 修改 `.env`，必须使用随机强密钥：

```dotenv
POSTGRES_PASSWORD=replace-with-a-long-random-password
JWT_SECRET=replace-with-at-least-32-random-bytes
PUBLIC_ORIGIN=https://notes.example.com
```

3. 构建并启动：

```powershell
docker compose up -d --build
docker compose ps
```

默认 Web 入口为 `http://localhost:8081`。PostgreSQL、SeaweedFS 和 API 不直接暴露到宿主机，Web nginx 统一代理 `/api`。

查看日志与停止服务：

```powershell
docker compose logs -f api web
docker compose down
```

彻底删除开发数据：

```powershell
docker compose down -v
```

当前项目尚未正式发布，数据库迁移按新安装设计；数据结构调整后开发环境应删除旧卷重建。

## Docker 代理

当前 PowerShell 会话使用本地 HTTP 代理：

```powershell
$env:all_proxy = "http://127.0.0.1:7890"
$env:http_proxy = $env:all_proxy
$env:https_proxy = $env:all_proxy
$env:DOCKER_BUILD_PROXY = "http://host.docker.internal:7890"
$env:CARGO_MIRROR_URL = "sparse+https://rsproxy.cn/index/"
docker compose pull
docker compose up -d --build
```

清除会话代理：

```powershell
Remove-Item Env:all_proxy, Env:http_proxy, Env:https_proxy, Env:DOCKER_BUILD_PROXY, Env:CARGO_MIRROR_URL -ErrorAction SilentlyContinue
```

`all_proxy` 用于宿主机 Docker CLI 拉取基础镜像；构建容器必须通过 `host.docker.internal` 访问宿主机代理，因此单独使用 `DOCKER_BUILD_PROXY`。当前代理访问 crates.io 可能返回 TLS 502，API 镜像默认使用 rsproxy sparse index，可通过 `CARGO_MIRROR_URL` 覆盖。如果 Docker Desktop 后台仍不能拉取镜像，在 Docker Desktop 的 **Settings → Resources → Proxies** 设置同一地址并重启 Docker Desktop。通常无需修改系统文件。

## 验证

```powershell
npm run build:desktop
npm run build:web
npm run test:desktop-rust
npm run test:server
npm run test:protocol
npm run test:e2e
```

基础设施集成测试使用仅绑定回环地址的覆盖文件：

```powershell
docker compose -f docker-compose.yml -f tests/docker-compose.test.yml up -d
```

## 安全说明

- 云端地址必须使用 HTTPS。
- 不要提交 `.env`、WebDAV 密码、JWT 或数据库密钥。
- 生产环境应在 nginx 前配置 TLS、备份、监控与访问限流。
- SeaweedFS 和 PostgreSQL 只应在内部 Docker 网络中访问。

进一步设计说明见 [docs/cloud-architecture-plan.md](docs/cloud-architecture-plan.md) 和 [docs/reliability-sync-design.md](docs/reliability-sync-design.md)。
