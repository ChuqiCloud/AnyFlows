# CI 维护

日常检查由 [`ci.yml`](../.github/workflows/ci.yml) 按目标分支和改动路径选择。手动运行该工作流会选择全部组件。

## 自动检查

| 改动范围 | 检查内容 |
| --- | --- |
| Rust | 格式、依赖审计与边界、渠道能力目录、OpenAPI；`develop` PR 执行工作区编译检查，`main` PR 和推送执行 Clippy、nextest、文档测试、MSRV 及 Linux amd64 发布包构建 |
| 前端 | Classic 和 Next 分别执行 lint、构建，共同检查接口及共享逻辑一致性 |
| 数据库 | SQLite、PostgreSQL、MySQL 迁移检查 |
| Redis、ClickHouse | `main` PR 和推送按对应路径执行集成检查 |
| Docker、systemd | `main` PR 和推送按部署文件路径执行检查 |

文档和专项测试脚本的改动不会单独触发整套 Rust 构建。修改 `ci.yml` 会选择 Rust 和双前端检查；数据库、部署等检查仍由各自路径规则决定。浏览器专项仅在手动运行时执行。

公共导出与同步检查、发布脚本检查有独立的路径规则。具体触发条件以各工作流的 `on` 和任务条件为准。

## 手动专项

在 Actions 中选择工作流和待检查分支后运行。下列工作流不随普通 PR 或分支推送触发：

| 工作流 | 用途 |
| --- | --- |
| `cache.yml` | 缓存契约与真实 Redis |
| `console-extensions.yml` | 双前端客户端档案、代理池浏览器检查 |
| `conversation-compatibility.yml` | 会话协议、OpenCode、调度与客户端档案 |
| `database-extension.yml` | 数据库扩展迁移、支付与共享配额 |
| `proxy-pools.yml` | 代理池导入、绑定、测活与后台任务 |
| `frontend-generated-assets.yml` | 生成 OpenAPI、前端 SDK 和补丁 |

## 修改工作流

新增检查时配置对应路径，避免仅修改文档时编译应用。扩充路径规则时同步检查 PR、分支推送和手动触发三种入口。

Rust 任务复用构建缓存；容器集成测试使用匹配的编译与运行环境，Redis 测试使用独立网络。发布、公共导出和讨论同步按既有并发组串行执行，防止重复发布或同步冲突。
