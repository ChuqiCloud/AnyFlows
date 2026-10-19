# 公共核心导出

本文供仓库维护者使用。安装和社区开发无需运行导出工具。

Gitea `develop` 是公共核心的源码基线。GitHub 公共树由指定提交导出，不手工复制工作目录。GitHub 接收的公共贡献通过 [回传流程](public-pr-sync.md) 进入 Gitea。

## 手动导出

准备 Git 2.38 或更新版本。从干净的 Gitea checkout 执行，将提交占位符替换为待发布的完整提交 ID：

```sh
python3 tools/repository/export-public-core.py \
  --source . \
  --destination ../AnyFlows-public \
  --commit <gitea-commit> \
  --allow-replace
```

目标必须已经是无本地改动的 Git 仓库。工具检查企业边界及敏感文件，使用 `git archive` 导出精确提交，并生成 `docs/public-core-source.json`，记录来源仓库和完整提交。`export.preservePaths` 声明的法律文件可以保留公共仓库版本；工作目录、Git 凭据不会被复制。

替换前，工具在临时仓库中对比上次导出、当前公共分支与候选源码。尚未回传的公共修改、删除或冲突都会阻止导出。先将缺失贡献合并进 Gitea，再运行导出，不能用强制覆盖绕过。

审查差异后创建 GitHub 分支并提交 PR。来源元数据必须保留，不通过普通公共 PR 手改。

CI 用以下脚本验证导出行为，无需编译应用：

```sh
bash tools/ci/test-public-core-export.sh
```

## 自动导出

Gitea `develop` 每次推送触发导出工作流，使用 `PUBLIC_REPO_SYNC_TOKEN` 更新 GitHub 的 `sync/gitea-public-core` 分支，创建或复用一个 PR，仍由维护者合并。导出完成后会同步来源 Gitea PR 的普通讨论；后续评论事件由独立工作流处理。Secret 配置及双向流程见 [公共 PR 同步](public-pr-sync.md)。
