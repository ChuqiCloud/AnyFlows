# 版本与发布

公开版本遵循 [Semantic Versioning 2.0.0](https://semver.org/)，标签以 `v` 开头：

| 阶段 | 示例 | 用途 |
| --- | --- | --- |
| Alpha | `v0.3.0-alpha.1` | 早期预览，接口和配置可能调整 |
| Beta | `v0.3.0-beta.1` | 功能测试与部署验证 |
| RC | `v0.3.0-rc.1` | 发布候选版本，处理阻塞问题 |
| 正式版 | `v0.3.0` | 稳定发行 |

预发布序号从 `1` 开始，同阶段递增；进入下一阶段重新从 `1` 开始。正式版之后递增版本号，不复用原版本的预发布序列。

兼容功能递增次版本号，缺陷修复递增修订号，不兼容变更递增主版本号。项目处于 `0.y.z` 阶段时，不兼容变更递增次版本号，并在说明中列出兼容范围。

## 社区发布

1. 更新 workspace、内部依赖和 OpenAPI 版本，填写 `CHANGELOG.md`，说明迁移、配置及兼容性变化。版本字段不带 `v`。
2. 通过 Gitea PR 合入 `develop` 并完成远程检查，再将发布内容合入 `master`。
3. 等待自动生成的 GitHub 同步 PR 完成检查并合入 `main`，核对版本和 `docs/public-core-source.json` 来源记录。
4. 分别在两个仓库对应的提交创建同名附注标签。导出后的提交号可以不同，推送前分别核对远程地址和标签指向。
5. 向 GitHub 推送标签，确认 `Publish public release` 工作流成功，发布说明与版本记录一致，归档和 SHA-256 附件齐全。

公开工作流构建 Linux amd64 二进制及 Classic、Next 两套内置前端。Alpha、Beta 和 RC 标记为 Pre-release，正式版设为 Latest。合并同步 PR 不会自动创建标签或 Release。

标签和已发布产物不可覆盖；修复使用新的版本号。两个仓库分别推送指定标签，避免推送全部标签或私有历史。

标签检查由 [`verify-release-tag.sh`](../tools/ci/verify-release-tag.sh) 执行；仅允许 `alpha`、`beta`、`rc` 预发布阶段，不接受零序号、日期后缀或构建元数据。
