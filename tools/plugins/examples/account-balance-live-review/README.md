# 外部账户实时余额分析示例

本示例比较用户当前外部账户余额与 AnyFlows 已保存的余额快照。每次运行前，用户都必须在页面勾选一次性授权。宿主使用已有连接器读取当前账户并保存新快照，然后只把允许的余额字段传给插件。

插件不会收到用户 ID、账户 ID、提供商 URL、凭据、请求头，也没有任意 HTTP 能力。插件不能读取其他账户、修改账户绑定、充值钱包或修改账本。结果仅用于提示，不代表提供商账单或财务建议。

## 打包

在仓库根目录运行：

```bash
python tools/plugins/package-example.py dist/account-balance-live-review.afplugin --example account-balance-live-review
```

输出文件必须尚不存在。

## 安装和授权

管理员在“插件管理”上传包，启用部署并授予：

- `account.balance.snapshot.live-review.read@1`：允许运行余额分析；
- `external.account.balance.read@1`：允许插件请求宿主固定的余额回调。

部署配置必须是空对象 `{}`。这两个授权互相独立，都不会把账户凭据或通用网络客户端交给插件。

用户在“个人资料与安全”页面选择已绑定账户和插件。实时读取复选框默认关闭，只对本次分析有效：

- 不勾选：只分析本地快照，不刷新上游账户；
- 勾选：宿主读取当前选中的账户、保存新快照，再通过 Sidecar 回调返回有限字段。

使用系统 PAT 时，查看插件需要 `external_accounts:read`。如果请求把 `allow_external_reads` 设为 `true`，PAT 还必须有 `external_accounts:live_read`。PAT scope 不能替代页面上的逐次同意。

## Python 处理函数

只有处理函数声明第四个参数时，SDK 才会传入 `context`：

```python
def review_live_balance(capability_id, public_config, value, context):
    current = context.proxy_read("account.balance")
    return make_report(value["snapshots"], current)
```

回调只能使用宿主登记的 endpoint ID。SDK 和宿主会校验同一请求的制品摘要、部署代次、能力授权和截止时间。一次调用最多四次回调；协议帧异常或关联信息不一致时，宿主会终止插件。

## 修改示例

需要新增提供商字段时，先由宿主增加固定投影和 Schema。不要向插件请求中增加 URL、自由路由参数或凭据。新的远程读取必须拥有独立 endpoint、独立 capability、逐次同意、账户归属检查、输出上限和回归测试。
## 本地测试

输入样例在 `fixtures/input.json`。实时余额插件还需要模拟宿主回调：

```bash
python tools/plugins/test-plugin.py --root tools/plugins/examples/account-balance-live-review --manifest tools/plugins/examples/account-balance-live-review/manifest.json --capability account.balance.snapshot.live-review.read --input tools/plugins/examples/account-balance-live-review/fixtures/input.json --proxy-response tools/plugins/examples/account-balance-live-review/fixtures/proxy-account-balance.json
```

`--proxy-response` 是宿主返回的固定余额投影，不是上游接口地址或凭据。测试工具会验证插件发出的 continuation 请求和最终输出。