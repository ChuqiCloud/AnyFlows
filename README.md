# AnyFlows

> 开源的大模型 API 中转 / 聚合网关系统（类 OpenRouter）。后端 **Rust**，前端 **React 19 + shadcn/ui**。

一把 API Key、一个统一的 OpenAI / Claude / Gemini 兼容端点，即可调用背后接入的任意上游大模型；管理员在一个控制台里管理上游渠道、订阅账号、计费、用户、限流与运营。

**当前状态**：里程碑 M1 开发中。后端已完成生产 Chat 动态选路，前端管理台进入骨架建设阶段。

## 技术栈

- **后端**：Rust · Axum · SeaORM · Tokio · Redis
- **前端**：React 19 · TypeScript · shadcn/ui · Tailwind CSS · TanStack Router/Query
- **数据库**：PostgreSQL（主）/ MySQL / SQLite
- **构建产物**：单二进制（前端内嵌）

## 仓库拓扑

- **产品主仓**（本仓库）：`https://cnb.cool/acmecloud/AnyFlows` — 后端 Rust 代码与前端。分支 `master` + `develop`；功能从 `develop` 拉 `feature/*` → PR 合回 `develop`；发布时 `develop` 合并到 `master` 打 tag。
- **设计文档仓**（独立）：`https://cnb.cool/acmecloud/AnyFlows-Design` — 架构、数据模型、协议、计费、调度、路线图、开发 TODO 与避坑清单。本仓库通过 `.gitignore` 排除 `design/`，两仓互不干扰。

> 设计文档不在本仓库。需要阅读设计方案时，单独克隆设计仓：
> `git clone https://cnb.cool/acmecloud/AnyFlows-Design design`

## 快速开始

前置：Rust 工具链由 `rust-toolchain.toml` 固定（当前 1.97.1，自动安装）；前端使用 Node.js 24 与 Corepack 管理的 pnpm。

```bash
# 构建
cargo build --workspace

# 质量门禁（与 CI 一致）
cargo fmt --all -- --check                                              # 格式
cargo run --locked --package xtask -- check-deps                        # crate 依赖边界
cargo run --locked --package xtask -- openapi check                     # OpenAPI 契约漂移
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --workspace --all-features --locked --no-fail-fast    # 测试
cargo test --workspace --all-features --doc --locked                    # 文档测试
cargo deny --all-features --locked check advisories bans licenses sources

# 前端开发
corepack pnpm@10.33.0 --dir web install --frozen-lockfile
corepack pnpm@10.33.0 --dir web dev

# 仅从 Rust 契约导出 OpenAPI
cargo run --locked --package xtask -- openapi export

# 同步导出 OpenAPI，并更新生成客户端
corepack pnpm@10.33.0 --dir web api:generate

# 校验 OpenAPI 与生成客户端均未漂移
cargo run --locked --package xtask -- openapi check
corepack pnpm@10.33.0 --dir web api:check

# 前端质量门禁
corepack pnpm@10.33.0 --dir web check
```

运行目录外部前端模板的目录结构、独立构建、API 契约、扫描切换和发布检查见
[`docs/frontend-template-development.md`](docs/frontend-template-development.md)。

### Linux Docker 部署

当前 Docker 首切片使用 Linux host 网络，以保持服务现有的 loopback 监听安全边界。应用监听宿主机 `127.0.0.1:8080`，PostgreSQL 和 Redis 分别只监听 `127.0.0.1:5432` 与 `127.0.0.1:6379`；公网访问应由宿主机上的 Nginx/Caddy 等反向代理转发，Compose 不直接发布公网端口。

```bash
./scripts/deploy/up.sh
curl http://127.0.0.1:8080/healthz
docker compose --env-file .env logs -f app
docker compose --env-file .env down
```

首次运行会调用 `scripts/deploy/init-env.sh` 生成权限为 `0600` 的 `.env`，其中包含随机 PostgreSQL 密码、管理会话签名密钥和凭据加密密钥；已有 `.env` 时脚本拒绝覆盖。此部署形态要求 Docker Engine、Compose 插件和 OpenSSL，并仅支持 Linux 主机；数据库、Redis 和计费 WAL 使用 Docker 命名卷，删除容器不会删除数据。

### Linux systemd 部署

Linux amd64 Release 归档同时包含 `anyflows`、`anyflows.conf`、`install.sh` 和 `anyflows.service`。手工启动时使用归档内的 TOML 配置并注入两项密钥：

```bash
export AF_CONFIG_FILE="$PWD/anyflows.conf"
export AF_AUTH__SESSION_SIGNING_KEY="$(openssl rand -base64 32 | tr '+/' '-_' | tr -d '=')"
export AF_CREDENTIAL_ENCRYPTION__KEY_ID=primary
export AF_CREDENTIAL_ENCRYPTION__KEY="$(openssl rand -base64 32 | tr '+/' '-_' | tr -d '=')"
./anyflows
```

首次使用 systemd 安装时执行以下命令，安装脚本会在 `/etc/anyflows/anyflows.env` 生成 SQLite 与随机密钥配置；升级会保留现有配置并在 `/readyz` 未恢复时自动回滚上一份二进制：

```bash
sudo ./install.sh install
sudo ./install.sh upgrade
sudo ./install.sh rollback
sudo ./install.sh status
```

服务使用专用 `anyflows` 用户，数据和计费 WAL 位于 `/var/lib/anyflows`，配置位于 `/etc/anyflows`；应用仍只监听 `127.0.0.1:8080`，公网访问需由宿主机反向代理提供。安装脚本不下载网络内容，也不会覆盖已有配置文件。

## 架构：crate 分层

采用模块化单体，按 crate 强制依赖方向（下层不得依赖上层）。依赖边界由 `cargo xtask check-deps` 在 CI 中**强制校验**（fail-closed，见 `tools/xtask`）。

| 层 | crate | 职责 |
|---|---|---|
| 入口 | `af-server` | Bootstrap、路由挂载、优雅关闭 |
| HTTP | `af-http` | Axum router、中间件、错误映射（**唯一可依赖 axum 的 crate**） |
| 应用 | `af-relay` `af-billing` `af-scheduler` `af-account` `af-admin` `af-analytics` | 转发编排 / 计费 / 调度 / 账号 / 管理 / 用量 |
| 协议 · 适配 | `af-protocol` `af-adapter` | Canonical 协议中间层 / 上游供应商适配器（**协议纯净，禁止反向依赖应用层**） |
| 基础设施 | `af-db` `af-cache` `af-httpclient` `af-config` `af-telemetry` | 存储 / 缓存 / HTTP 客户端 / 配置 / 可观测性 |
| 领域 | `af-domain` | 纯领域类型、值对象、错误（**无 IO**） |
| 工具 | `tools/xtask` | 依赖边界校验、OpenAPI 导出与漂移检查等仓库质量任务 |

### 依赖策略要点（`tools/xtask`）

- `af-domain` / `af-protocol` / `af-adapter` **默认拒绝外部依赖**，每新增一项都必须显式更新 `xtask` 的允许列表（force-review 门禁）。
- 只有 `af-http` 可以依赖 `axum`。
- 内部 crate 依赖必须在允许边表中显式声明，否则 CI 失败。

## 版本与 MSRV

- 工具链固定 `rust-toolchain.toml`（当前 1.97.1）。
- MSRV = `Cargo.toml` 的 `rust-version`（当前 1.95），CI 有独立 job 用 MSRV 校验。
- 提升 MSRV 需同步 `Cargo.toml`、`clippy.toml`、CI 与本 README。

## 贡献

分支模型、提交规范与发布流程见设计仓 `design/12-git-workflow.md`。简述：从 `develop` 拉 `feature/*` → PR 合回 `develop`；发布时 `develop` → `master` 打 tag。
