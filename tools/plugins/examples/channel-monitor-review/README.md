# 渠道监控事件检查示例

这个示例给渠道管理员查看**已有**监控事件的文字摘要和建议。插件只处理宿主传入的 `down`、`recovered` 事件快照，不会连接渠道、发起探测、改变监控设置、切换路由或修改渠道状态。

## 准备环境

- 安装 Python 3.10 或更新版本。插件逻辑只用 Python 标准库。
- 运行插件的每个 AnyFlows 节点都要能启动包声明的 Linux 入口 `/usr/bin/python3`。
- 本机开发时从仓库根目录运行打包命令。示例打包脚本会把 Python SDK 一起放入插件包；不需要手工复制依赖。

## 打包

在仓库根目录执行：

```bash
python tools/plugins/package-example.py dist/channel-monitor-review.afplugin --example channel-monitor-review
```

`dist/channel-monitor-review.afplugin` 必须尚不存在。命令成功后会打印新包的路径。若 `dist/` 目录还不存在，请先创建：

```bash
mkdir -p dist
```

Windows PowerShell 可用：

```powershell
New-Item -ItemType Directory -Force dist
```

## 安装和使用

1. 管理员打开 AnyFlows 的“插件管理”，上传生成的 `.afplugin` 包并安装。
2. 普通上传安装会完成制品登记。对手工登记的制品，先在插件管理中审批该制品。然后在部署的“高级管理”中明确授予 `channel.monitor.events.review.read` v1，再启用部署。插件的 `publicConfig` 必须保留为空对象 `{}`。
3. 管理员进入“渠道”页面，展开“主动渠道监控”。在目标的已记录事件区域中选择插件并运行检查。
4. 页面显示摘要和检查项。结果是参考意见，不会替管理员执行探测或更改设置。

使用管理员个人访问令牌调用管理接口时，列出插件需要 `admin:list_admin_channel_monitor_plugin_reviewers:execute`，运行检查需要 `admin:review_admin_channel_monitor_target_with_plugin:execute`。这两个权限分别只覆盖“列出检查插件”和“分析指定目标的事件”；会话和 PAT 都必须具有管理员角色。

## 输入数据

宿主只发送最近最多 100 条事件，不会把目标、渠道或凭据 ID、模型名、URL、响应正文或密钥交给本插件。字段固定如下：

```json
{
  "events": [
    {"kind": "down", "latency_ms": null, "occurred_epoch_seconds": 1794700000},
    {"kind": "recovered", "latency_ms": 850, "occurred_epoch_seconds": 1794700300}
  ]
}
```

| 字段 | 含义 |
| --- | --- |
| `kind` | `down` 表示监控记录到故障，`recovered` 表示监控记录到恢复 |
| `latency_ms` | 以毫秒表示的恢复探测延迟；没有采集到时为 `null` |
| `occurred_epoch_seconds` | Unix 时间戳，单位为秒 |

数据库按时间倒序返回事件。示例插件会先按时间排序再统计。输入为空时插件会说明“暂无历史”，不会将此情况解释为渠道健康。

## 输出数据

输出是符合固定格式的摘要和检查项：

```json
{
  "summary": "最近 2 条记录中有 1 次 down、1 次 recovered。",
  "checks": [
    {
      "code": "events.down-count",
      "title": "发现故障事件",
      "status": "warning",
      "message": "最近 2 条记录中包含 1 次 down。",
      "recommendation": "结合目标的监控周期和告警阈值检查上游状态。"
    }
  ]
}
```

`status` 只能是 `pass`、`info`、`warning` 或 `fail`。摘要、标题、消息和建议必须是简短文字；`recommendation` 没有建议时填 `null`。宿主最多接受 64 项检查，结果上限为 16 KiB。

## 修改检查规则

从 `plugin.py` 的 `review_events` 开始读。它按顺序做四件事：确认能力名与空配置、验证输入形状、分析事件并生成检查项、验证返回结果没有超过大小上限。

添加一条规则时，在 `# 检查规则` 区域增加一个 `if`，并用 `_check(...)` 创建检查项。例如：

```python
if down_count >= 5:
    checks.append(
        _check(
            "events.frequent-down",
            "故障事件较多",
            "warning",
            f"最近事件窗口里出现了 {down_count} 次 down。",
            "查看更长时间范围的监控记录并核实上游状态。",
        )
    )
```

`code` 是用于识别这条规则的稳定标识；修改文案时尽量保持 code 不变。请只根据 `events` 计算，不要添加网络请求或读取本地环境变量。修改 Manifest 时必须保持输入、输出 Schema 和处理函数一致；如需改变字段格式，应发布新的 capability 版本并由宿主增加相应支持，不能只改插件声明。

## 代码结构

```text
channel-monitor-review/
├── manifest.json  # 插件身份、能力名、Schema 和资源上限
├── plugin.py      # 业务检查规则
└── README.md      # 使用和修改说明
```

`package-example.py` 会临时复制这三个文件，再把标准库 SDK 加入目录后打包。SDK 只负责插件协议；检查规则都在 `plugin.py`，不要求了解 AnyFlows Rust 源码。
