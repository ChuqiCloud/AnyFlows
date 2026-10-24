# 公共 PR 同步

本流程供仓库维护者配置。双向同步均创建 PR，由维护者审查合并。

## 同步方向

| 来源 | 自动操作 | 维护者操作 |
| --- | --- | --- |
| GitHub PR 合并到 `main` | 下载该 PR 补丁，在 Gitea 当前 `develop` 上建分支和 PR | 审查并合并 Gitea PR |
| Gitea 推送到 `develop` | 检查公共边界，导出并更新 GitHub 同步 PR | 审查并合并 GitHub PR |

回传通过 GitHub API 获取已合并 PR 的补丁，支持 merge、squash 和 rebase。补丁无法应用时中止，不推送冲突分支；维护者需基于当前 Gitea 源码解决冲突并提交 PR。

## 标题、说明与评论

同步 PR 沿用来源标题和说明。来源信息不可用时使用默认标题和说明，已有同步 PR 也会更新。补丁获取失败、代码冲突或公共边界检查失败则中止同步。

普通讨论评论双向同步新增、编辑和删除，按来源评论 ID 去重，仅修改服务账号创建的镜像。镜像保留来源用户和评论链接。行内代码评审评论不复制。

Gitea 到 GitHub 的评论同步仅处理已导出、带唯一来源标记的公共核心 PR。导出时补齐已有评论，后续变更由事件工作流处理。来源不明确时跳过评论同步。

Gitea 来源仓库、PR 编号和提交通过隐藏标记关联，不附加到可见说明。更新同步 PR 时一并刷新标记，避免评论关联到旧提交。

Gitea 讨论可能含私有信息。启用评论公开同步前，应告知参与者公开范围并取得同意。

导出前进行三方内容检查；公共修复尚未回传时阻止导出。导出结果仅含来源元数据变化时，不创建同步 PR。

## Secrets

在 GitHub 仓库 `Settings → Secrets and variables → Actions` 配置：

| 名称 | 用途 |
| --- | --- |
| `GITEA_SYNC_USERNAME` | 同步用 Gitea 账号 |
| `GITEA_SYNC_TOKEN` | 读取目标仓库、推送分支和创建 PR |

在 Gitea 仓库的 Actions Secrets 中配置：

| 名称 | 用途 |
| --- | --- |
| `PUBLIC_REPO_SYNC_TOKEN` | 仅限 `ChuqiCloud/AnyFlows` 的 GitHub Token，具有 Contents、Issues 和 Pull requests 读写权限 |

两个平台分别使用表中的 Secret 名称，Gitea 不使用 `GITEA_` 前缀。同步账号应独立配置，令牌不得写入代码或日志。

Gitea 需启用 `pull_request` 和 `issue_comment` Actions 事件，分别更新合并后的标题、说明及评论。事件未触发时，可手动运行 `Mirror Gitea PR discussion`，填写 Gitea PR 编号。

Gitea 工作流令牌需要 Contents、Issues 和 Pull requests 只读权限，API 地址取 Runner 的 `github.server_url`。事件任务仅同步已合并到 `develop` 的 PR。工具从 `develop` 检出，GitHub 写入令牌仅提供给同步步骤。

## 分支和重复执行

公共导出复用 `sync/gitea-public-core`。存在开放 PR 时更新原 PR；合并后的下一次导出创建新 PR。回传工作流排除该分支前缀，防止循环。

公共 PR 回传分支为 `sync/github-pr-<number>-<public-head-sha>`，重跑复用已有分支。

`docs/public-core-source.json` 由导出工具生成，公共 PR 不得修改。调整同步流程时，应检查三方比较、分支复用和循环排除规则。导出细节见 [公共核心导出](public-core-export.md)。
