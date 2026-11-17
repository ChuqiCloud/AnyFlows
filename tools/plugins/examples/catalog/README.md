# 模型目录示例

这个插件展示如何从包内的静态 `models.json` 搜索模型资料、分页返回。它只产生模型广场的说明文字；安装后不会创建可调用模型、渠道、价格或 Key。

## 运行前要知道

- Python 3.10 或更新版本用于运行本机打包器。
- 实际安装插件的每个 AnyFlows 节点都要具备 Manifest 指定的运行程序。本示例入口是 Linux 的 `/usr/bin/python3`。
- 示例只使用 Python 标准库，不连接网络、不读取数据库、不使用凭据。
- 若使用仓库内置打包命令，它会把 Python SDK 一起放进包。

## 打包、安装、运行

在 AnyFlows 仓库根目录打开终端。若还没有 `dist` 文件夹，先创建：

```bash
mkdir -p dist
```

Windows PowerShell 使用：

```powershell
New-Item -ItemType Directory -Force dist
```

再运行打包命令：

```bash
python tools/plugins/package-example.py dist/catalog-example.afplugin
```

输出包必须尚不存在。命令会复制示例文件和 Python SDK，再生成 `.afplugin` 文件；它只负责打包，不会安装服务器依赖或启动 AnyFlows。

随后管理员在“插件管理”上传并安装。模型资料目录属于公开只读能力，使用简单安装流程即可部署。登录后进入模型页的“插件模型资料”，选中 `Example model catalogue`，输入 `vision` 搜索示例结果。清空搜索词可以查看全部目录；继续翻页时，宿主会把插件上次返回的 `nextCursor` 传回来。

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

宿主每次调用都会传入 `limit`。`query` 和 `cursor` 是可选字段；第一页通常没有 `cursor`，不搜索时可以省略 `query`。

插件收到的输入形如：

```json
{"query":"vision","cursor":null,"limit":20}
```

- `query` 是搜索词或 `null`；插件按模型 ID、显示名和提供商名称进行不区分大小写的查找。不传搜索词时返回未过滤的目录。宿主限制搜索词为 128 个 UTF-8 字节；本例 Manifest 限制为 128 个字符，插件还会检查实际字节数。
- `cursor` 在第一页省略或为 `null`；本示例把传给插件的数字字符串当作下一页偏移量。模型页返回给浏览器的是宿主编码后的游标，宿主会在下一页请求前验证它属于当前插件版本和搜索词，再还原成插件自己的游标。不要自行解码或构造页面使用的游标。
- `limit` 是必填页大小，范围为 1–50；实际调用还会受插件 Manifest 中 `maxPageSize` 上限约束。

插件返回 `models` 数组和 `nextCursor`。模型最少要有 `id`、`displayName`、`provider`；其他允许字段及最大长度由宿主契约检查。宿主不接受价格、上游地址、密钥或额外字段。

返回值必须包含 `models` 和 `nextCursor` 两个字段。没有下一页时使用 `null`；有下一页时返回一个符合插件 Manifest 限制的游标字符串。宿主会把它编码为页面游标，并检查每页数量、重复 ID、字段长度、支持的输入/输出模态、标签数量和游标是否有效。字段不符合契约时，宿主会拒绝整页结果。

## 跟着代码走一遍

打开 `plugin.py`，可以按这个顺序阅读：

1. 文件顶部导入 Python 标准库及 AnyFlows Python SDK；SDK 负责插件通信。
2. `ROOT` 定位示例目录。这样无论 AnyFlows 从哪个当前工作目录启动，代码都能找到 `models.json` 和 `manifest.json`。
3. `read_catalog()` 是本例的业务函数。它先确认宿主调用的是已声明能力，再校验配置和输入字段。
4. 函数读取随包附带的 `models.json`，根据可选搜索词过滤，再用 `cursor` 和 `limit` 截取当前页。
5. 最后返回 `models` 与 `nextCursor`。宿主还会按自己的固定契约重新检查结果；插件不能靠 Manifest 放宽宿主上限。
6. `main()` 读取 Manifest 并调用 `Plugin(...).serve()`。不要自己读取标准输入或向标准输出写日志，因为标准输出只用于 JSON Lines 协议帧。

代码中的中文注释解释了每一步为什么存在。你可以先只修改 `models.json` 看资料变化，再改搜索条件，最后再尝试新增一个模型展示字段。每次新增或改变字段时，都要确认宿主的输出契约允许它；插件端 Schema 不能自行扩展宿主接口。

## 修改示例

可以先在 `models.json` 增加、改写展示数据，再修改 `plugin.py` 的过滤规则。保持 Manifest 输入输出 Schema 与处理函数结构相符。将 `pluginId` 改为自己的命名空间，修改内容后递增 `pluginVersion`，然后打包成新制品；不要用相同 ID 覆盖已登记制品。

常见修改顺序：先添加一条带有 `id`、`displayName` 和 `provider` 的模型资料；再用搜索词确认它能被找到；最后多次翻页确认 `nextCursor` 能继续前进并在末页变成 `null`。如果需要新的展示字段，先确认输出 Schema 已支持该字段，再同时修改 Manifest、代码和数据。

完整字段约束、对外 API 和游标规则见[模型资料目录契约](../../../../docs/plugin-development.md#模型资料目录)。要接入其他业务模块，先让宿主提供该模块的输入输出 Schema、权限和调用入口。
