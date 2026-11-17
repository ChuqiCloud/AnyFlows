# 外部账户余额快照分析示例

本插件只分析 AnyFlows 已保存的余额快照，不访问外部平台，也不修改账户数据。它按提供商返回的单位比较金额，并标记不可用的记录。

宿主会确认当前登录用户拥有该账户，最多读取 20 条快照，并移除账户和提供商标识。插件只收到 `amount`、`unit`、`status` 和 `observedAt`，不会收到账户 ID、外部主体、提供商 URL、访问令牌或 AnyFlows 用户 ID。运行分析不会刷新快照，也不会解密凭据。

结果是给账户所有者查看的文字建议，不是账务记录、提供商确认或财务建议。

## 打包

在仓库根目录运行：

```bash
python tools/plugins/package-example.py dist/account-balance-snapshot-review.afplugin --example account-balance-snapshot-review
```

输出文件必须尚不存在。

## 安装和运行

管理员在“插件管理”上传包，启用部署并授予 `account.balance.snapshot.review.read@1`。部署配置保持 `{}`。

用户打开“个人资料与安全”，在已绑定账户上选择“插件分析”，确认数据共享提示，选择本插件并运行。接口使用当前用户会话，或使用带 `external_accounts:read` 的系统 PAT：

```http
GET /api/account/external-accounts/plugin-reviewers
POST /api/account/external-accounts/{id}/plugin-review
```

## 插件输入

```json
{
  "snapshots": [
    {"amount": "12.50", "unit": "USD", "status": "available", "observedAt": 1730000000}
  ]
}
```

宿主会按 `manifest.json` 校验输入和输出，在调用前后检查账户归属和插件部署代次。插件输出只作为文字显示，不会修改账户、钱包或余额账本。

## 修改示例

保持输入只包含四个快照字段。不要添加 HTTP 请求、凭据、提供商 URL、账户查询或写入操作。需要分析其他账户数据时，先由宿主提供独立的固定 capability、字段投影、归属检查和授权流程。