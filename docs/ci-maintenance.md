# 流水线资源维护

功能分支通过目标为 `develop` 或 `main` 的 PR 执行对应模块检查。校验工作流不再为历史功能分支保留专用 push 触发器；主分支合并后的 push 检查仍保留，手动入口用于必要的复测。

同一工作流、同一 PR 只保留最新一轮检查，后续提交会通过 `concurrency.cancel-in-progress` 取消旧轮次的排队和运行任务。PR 关闭也触发同一个并发组，但所有校验 job 跳过，不分配构建 Runner。不同 PR 的检查互不取消。发布、公共同步和讨论同步具有外部写入，继续串行完成。

数据库扩展检查复用 Rust 构建缓存，失败时保留已完成的依赖产物。公共导出检查直接覆盖任意来源分支的 PR，避免 `feature/`、`feat/`、`fix/` 或其他分支命名造成漏检。

## 定期回收

Gitea 1.27 的定期回收由服务器执行，无需另建占用 Runner 的定时流水线：

| 任务 | 默认检查周期 | 处理对象 |
| --- | --- | --- |
| `stop_zombie_tasks` | 每 5 分钟 | 长时间没有更新的运行或取消中任务 |
| `stop_endless_tasks` | 每 30 分钟 | 持续更新但运行时间超过上限的任务 |
| `cancel_abandoned_jobs` | 每 6 小时 | 长期没有 Runner 领取的排队、阻塞任务 |

检查周期与判定失效的时长不同。实际超时阈值由服务器的 `ZOMBIE_TASK_TIMEOUT`、`ENDLESS_TASK_TIMEOUT` 和 `ABANDONED_JOB_TIMEOUT` 决定；仓库中的 `timeout-minutes` 单独限制每个 job 的执行时间。管理员可通过 Gitea 管理后台或 `GET /api/v1/admin/cron` 核对启用状态、下次执行时间和执行次数，调整周期需要修改服务器配置，不能仅修改仓库 YAML。

该策略不删除历史运行记录、日志、缓存或发布产物。`cleanup_actions`、历史记录保留期和产物保留期属于另一类存储维护，不通过本次变更调整。

工作流改动由公共导出检查中的 `check-workflow-policy.py` 校验：防止重复功能分支触发、关闭事件分配构建任务、漏检同仓 PR、取消具有外部写入的发布流程及遗漏数据库构建缓存。
