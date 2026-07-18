# AnyFlows

> 开源的大模型 API 中转 / 聚合网关系统（类 OpenRouter）。后端 **Rust**，前端 **React 19 + shadcn/ui**。

一把 API Key、一个统一的 OpenAI / Claude / Gemini 兼容端点，即可调用背后接入的任意上游大模型；管理员在一个控制台里管理上游渠道、订阅账号、计费、用户、限流与运营。

**当前状态**：早期开发（里程碑 M0 · 项目骨架）。以下架构已落地为 Cargo workspace 骨架，业务逻辑正在按里程碑推进。

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

前置：Rust 工具链由 `rust-toolchain.toml` 固定（当前 1.97.1，自动安装）。

```bash
# 构建
cargo build --workspace

# 质量门禁（与 CI 一致）
cargo fmt --all -- --check                                              # 格式
cargo run --locked --package xtask -- check-deps                        # crate 依赖边界
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --workspace --all-features --locked                   # 测试
cargo test --workspace --all-features --doc --locked                    # 文档测试
cargo deny --all-features --locked check advisories bans licenses sources
```

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
| 工具 | `tools/xtask` | 依赖边界校验等仓库质量任务 |

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
