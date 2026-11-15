# 模型目录示例

这个插件展示如何从包内的静态 `models.json` 搜索模型资料、分页返回。它只产生模型广场的说明文字；安装后不会创建可调用模型、渠道、价格或 Key。

## 运行前要知道

- Python 3.10 或更新版本用于运行本机打包器。
- 实际安装插件的每个 AnyFlows 节点都要具备 Manifest 指定的运行程序。本示例入口是 Linux 的 `/usr/bin/python3`。
- 示例只使用 Python 标准库，不连接网络、不读取数据库、不使用凭据。
- 若使用仓库内置打包命令，它会把 Python SDK 一起放进包。

## 打包、安装、运行

在 AnyFlows 仓库根目录执行：

```bash
python tools/plugins/package-example.py dist/catalog-example.afplugin
```

输出包必须尚不存在。随后管理员在“插件管理”上传并安装；示例能力属于简单安装器可直接授予的公共模型资料能力。登录后进入模型页的“插件模型资料”，选中 `Example model catalogue`，输入 `vision` 搜索示例结果。

## 文件说明

```text
catalog/
├── manifest.json        # 插件身份、固定能力和输入输出 Schema
├── models.json          # 示例显示数据
├── plugin.py            # 搜索、分页和 SDK 启动入口
└── README.md            # 本说明
```

打包脚本自动复制 `tools/plugins/sdk/python/anyflows_plugin.py`。若用 `package-plugin.py` 自行打包，必须先把该 SDK 文件复制到分发目录，因为通用打包器不会替用户自动安装或下载 SDK。

## 输入和输出

插件收到的输入形如：

```json
{"query":"vision","cursor":null,"limit":20}
```

- `query` 为搜索词或 `null`；插件按 ID、显示名和提供商名称进行不区分大小写的查找。
- `cursor` 在第一页为 `null`；本示例将返回的数字字符串当作下一页偏移量。
- `limit` 为 1–50 的页大小。

插件返回 `models` 数组和 `nextCursor`。模型最少要有 `id`、`displayName`、`provider`；其他允许字段及最大长度由宿主契约检查。宿主不接受价格、上游地址、密钥或额外字段。

## 修改示例

可以先在 `models.json` 增加、改写展示数据，再修改 `plugin.py` 的过滤规则。保持 Manifest 输入输出 Schema 与处理函数结构相符。将 `pluginId` 改为自己的命名空间，修改内容后递增 `pluginVersion`，然后打包成新制品；不要用相同 ID 覆盖已登记制品。

完整契约、对外 API、游标规则和新增模块接线说明见 [插件开发入门](../../../../docs/plugin-development.md#模型资料目录)。
