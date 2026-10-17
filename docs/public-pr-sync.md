# 公共 PR 同步

本文供仓库维护者配置同步使用。两个方向都通过 PR 审查，不直接写入目标主分支。

## 同步方向

| 来源 | 自动操作 | 仍需维护者完成 |
| --- | --- | --- |
| GitHub PR 合并到 `main` | 下载该 PR 补丁，在 Gitea 当前 `develop` 上建分支和 PR | 审查并合并 Gitea PR |
| Gitea 推送到 `develop` | 检查公共边界，导出并更新 GitHub 同步 PR | 审查并合并 GitHub PR |

回传通过已鉴权的 GitHub API 获取合并 PR 补丁，支持 merge、squash 和 rebase 合并。补丁无法干净应用时工作流失败，不推送冲突分支；维护者需基于当前 Gitea 源码解决冲突并按正常流程提交。

正向导出会先做三方内容检查。公共修复尚未回传时，导出被阻止，避免覆盖公共修改。只更新来源元数据的变更会跳过空回传 PR。

## Secrets

在 GitHub 仓库 `Settings → Secrets and variables → Actions` 配置：

| 名称 | 用途 |
| --- | --- |
| `GITEA_SYNC_USERNAME` | 同步用 Gitea 账号 |
| `GITEA_SYNC_TOKEN` | 读取目标仓库、推送分支和创建 PR |

在 Gitea 仓库的 Actions Secrets 中配置：

| 名称 | 用途 |
| --- | --- |
| `PUBLIC_REPO_SYNC_TOKEN` | 仅限 `ChuqiCloud/AnyFlows` 的 GitHub Token，具有 Contents 和 Pull requests 读写权限 |

两边的 Secret 名称属于各自平台，不要把 GitHub 一侧的 `GITEA_` 名称照搬到禁止该前缀的 Gitea 一侧。建议使用专用服务账号，令牌不写入代码或日志。

## 分支和重复执行

公共导出始终复用 `sync/gitea-public-core`，开放 PR 存在时更新同一 PR；合并后下次导出复用分支名创建新评审。该前缀被回传工作流排除，防止循环。

公共 PR 回传分支为 `sync/github-pr-<number>-<public-head-sha>`。重跑会复用已存在的分支，不为同一公共提交重复建分支。

`docs/public-core-source.json` 由导出工具生成，公共 PR 修改它会被拒绝。更改同步流程时同时检查防回退、分支复用及循环排除规则，导出实现见 [公共核心导出](public-core-export.md)。