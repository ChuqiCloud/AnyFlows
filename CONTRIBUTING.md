# 贡献指南

问题反馈、文档修正和功能改进均通过公共仓库提交。开发分支从 `main` 创建，Pull Request 的目标分支为 `main`。

## 提交问题

说明版本、部署方式、复现步骤、预期行为和实际结果。日志保留 HTTP 状态码、请求 ID 和相关错误，移除 API Key、会话、密码、认证材料和个人信息。

页面问题还需注明前端模板、浏览器和视口尺寸。

## 开发环境

- Rust 使用 [rust-toolchain.toml](rust-toolchain.toml) 固定的版本。
- 前端使用 Node.js 和 Corepack，pnpm 版本以各前端的 `package.json` 为准。
- SQLite 可用于本地开发，PostgreSQL、MySQL 和 Redis 按改动范围准备。

运行前按 [配置参考](docs/configuration.md) 设置开发环境，使用独立的数据库和密钥。服务构建包含 Rust 和 `web-next`；`web/` Classic 前端源码保留供查阅，不参与服务构建和前端 CI。

## 按改动范围检查

Rust 改动通常需要：

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo test --workspace --all-features --locked
```

Next 前端使用 `web-next` 目录中的 pnpm 版本安装锁定依赖并检查：

```bash
cd web-next
corepack pnpm install --frozen-lockfile
corepack pnpm check
```

Classic 前端源码仍保留在 `web/`，但不再随服务发布。新前端功能只需维护和检查 `web-next`。

文档改动检查链接、命令和实现是否一致。CI 按路径选择前端、Rust、数据库和部署检查；手动运行可执行完整检查。触发条件见 [CI 配置](.github/workflows/ci.yml)。

## 接口与前端

接口以 Rust 定义为源。在 `web-next` 中生成客户端和 OpenAPI 定义：

```bash
corepack pnpm api:generate
```

该命令导出 OpenAPI 并生成 Next 前端的客户端。生成文件会同步到保留的 `web/` 源码目录；完成后，在根目录执行 `cargo xtask openapi check`。生成文件随接口改动提交，不手工编辑。

新增交互应覆盖加载、空数据、失败和移动端状态。

## 新增渠道

渠道的 `type` 指定适配器，`protocol` 指定上游原生协议，`provider` 记录厂商品牌。OpenAI 兼容服务通常复用已有适配器；只有协议或认证语义不同时才增加适配器。

1. 仅增加品牌时维护品牌目录。新增适配器需实现协议、认证和强类型配置，并接入 `af-adapter` 工厂。
2. 在 [`channel_capability.rs`](crates/af-domain/src/channel_capability.rs) 声明已接入的协议、调度、探活、模型发现与凭据类型。`supports_credential_kind` 保留历史兼容范围，`creatable_credential_kinds` 定义新建选项。
3. 常规渠道通过 `standard_adaptor_settings` 装配；固定入口、异步任务或额外参数使用专用配置。能力声明不替代认证校验。
4. 导出前端能力目录，补充表单与协议回归，生成文件随源码提交：

```bash
cargo xtask channel-capabilities export
cargo xtask channel-capabilities check
```

## Pull Request

标题和描述说明问题与修改后的行为，必要时附复现步骤或截图。改动保持集中，测试覆盖实际风险，避免重复测试和无关格式改动。

不要提交密钥、生产配置或编译目录。

## 版本标签

版本标签使用 `v主版本.次版本.修订号`，预发布使用 `-alpha.N`、`-beta.N` 或 `-rc.N`。发布分支合并且远程检查通过后，由维护者创建不可变的附注标签。规则见 [版本标签规范](docs/release-tagging.md)。
