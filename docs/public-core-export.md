# 公共核心导出

本流程用于维护源码镜像。社区部署与开发不需要运行导出工具。

公共核心从 Gitea `develop` 的指定提交导出到 GitHub。GitHub 社区贡献通过 [公共 PR 同步](public-pr-sync.md) 回传 Gitea。

## 手动导出

使用 Git 2.38 或更新版本，将 Gitea 源码检出到待导出的提交。源仓库和目标仓库均须无本地改动，目标须为源目录之外的独立 Git 仓库。

在源仓库执行以下命令，将占位符替换为当前 `HEAD` 的完整提交 ID：

```sh
python3 tools/repository/export-public-core.py \
  --source . \
  --destination ../AnyFlows-public \
  --commit <gitea-commit> \
  --allow-replace
```

工具根据 [`public-core-extraction.json`](public-core-extraction.json) 检查企业边界与敏感文件，再用 `git archive` 导出指定提交。生成的 `docs/public-core-source.json` 记录来源仓库和完整提交 ID；`export.preservePaths` 中的法律文件保留公共仓库版本。未跟踪文件和 Git 凭据不参与导出。

替换目标文件前，工具对比上次导出、当前公共分支和候选源码。尚未回传的公共修改、删除或冲突会阻止导出，需先将对应贡献合并进 Gitea。

导出后审查差异，在 GitHub 创建分支并提交 PR。来源元数据由工具维护，普通公共 PR 不应修改。

导出行为检查：

```sh
bash tools/ci/test-public-core-export.sh
```

## 自动导出

Gitea `develop` 推送触发 [`export-public-core.yml`](../.github/workflows/export-public-core.yml)，使用 `PUBLIC_REPO_SYNC_TOKEN` 更新 GitHub 的 `sync/gitea-public-core` 分支，创建或复用 PR，由维护者审查合并。

导出后同步来源 Gitea PR 的普通讨论，后续评论由独立事件工作流处理。令牌配置和双向流程见 [公共 PR 同步](public-pr-sync.md)。
