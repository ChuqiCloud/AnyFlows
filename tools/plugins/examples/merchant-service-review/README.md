# 商户服务资料检查示例

这个示例实现 `merchant.service.review.read` v1。它只读取服务说明和公开模型能力，返回摘要与检查建议；不访问网络，不接收上游地址、API Key、价格或财务资料，也不修改商户数据。

从仓库根目录打包：

```bash
python tools/plugins/package-plugin.py tools/plugins/examples/merchant-service-review tools/plugins/examples/merchant-service-review/manifest.json plugin.py dist/merchant-service-review.afplugin
```

上传包后，在插件管理中安装并启用，并为部署明确授予 `merchant.service.review.read`。随后商户可在服务列表中选择该插件运行资料检查。

示例使用系统 Python 和标准库。实际部署的每个节点都需要兼容的运行环境；包格式不会替节点安装解释器。
