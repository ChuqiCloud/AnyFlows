# 贡献指南

欢迎提交问题、文档修正和功能改进。请从 `main` 创建工作分支，并向 `main` 提交 Pull Request。公共仓库可以独立开发，不需要私有企业仓库权限。

## 提交问题

说明使用版本、部署方式、复现步骤和实际结果。日志保留 HTTP 状态码、请求 ID 和相关错误即可，提交前移除 API Key、会话、密码、认证材料和个人信息。

涉及页面问题时注明使用 Classic 还是 Next，以及浏览器和设备尺寸。

## 开发环境

- Rust 使用 [rust-toolchain.toml](rust-toolchain.toml) 固定的版本。
- 前端使用 Node.js，版本参考 [CI 配置](.github/workflows/ci.yml)，pnpm 版本以各前端的 `package.json` 为准。
- SQLite 可用于本地开发，PostgreSQL、MySQL 和 Redis 按改动范围准备。

完整构建会编译 Rust 和两套前端，先预留足够磁盘空间。运行前按 [配置参考](docs/configuration.md) 设置本地环境，不使用生产密钥。

## 按改动范围检查

Rust 改动通常需要：

```bash
cargo fmt --all -- --check
cargo check --workspace --all-features --locked
cargo test --workspace --all-features --locked
```

前端在相应目录安装锁定依赖并检查。修改两套前端共享行为时，两套都要检查：

```bash
corepack pnpm@10.33.0 --dir web install --frozen-lockfile
corepack pnpm@10.33.0 --dir web check
corepack pnpm@10.33.0 --dir web-next install --frozen-lockfile
corepack pnpm@10.33.0 --dir web-next check
```

只修改文档时，检查链接、命令与实际实现是否一致即可。CI 根据改动范围执行其余检查，涉及发布脚本、数据库或生成契约时还会有专门验证。

## 接口与双前端

修改接口时，先更新 Rust 定义，再生成 OpenAPI 和两套前端的客户端；不要手工编辑生成文件：

```bash
cargo xtask openapi export
corepack pnpm@10.33.0 --dir web api:generate
corepack pnpm@10.33.0 --dir web-next api:generate
cargo xtask openapi check
```

Classic 与 Next 共用接口、权限和业务规则，视觉组件可分别适配各自风格。新增交互需要同时考虑加载、空数据、失败和移动端状态。

## Pull Request

标题和描述直接说明问题及最终行为，必要时附复现步骤或页面截图。保持改动集中，新增测试覆盖实际风险，避免重复检查和无关格式改动。

不要提交密钥、生产配置、编译目录或私有企业实现。扩展契约见 [仓库与扩展边界](docs/repository-boundary.md)。

## 版本标签

版本标签使用 `v主版本.次版本.修订号`，预发布阶段使用 `-alpha.N`、`-beta.N` 或 `-rc.N`。版本发布前先完成发布分支合并和远程流水线检查，再由维护者创建不可变的附注标签。完整规则和三仓库对应关系见 [版本标签规范](docs/release-tagging.md)。
