# QuickNote

QuickNote 是一个本地优先、支持云端协作的高性能便签应用。桌面端使用 Tauri + React + SQLite；云端由 Axum、PostgreSQL、SeaweedFS 和 React Web 组成。

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
