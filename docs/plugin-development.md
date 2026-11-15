# 插件开发

AnyFlows 的 `.afplugin` 插件是一个独立程序。宿主通过标准输入、输出发送 JSON 请求，不限制开发语言。目前提供 Python SDK；Go、Rust 等语言可以按同一协议实现。

先确认要扩展的功能是否已有调用入口，再开始写插件。安装包中的 Manifest 声明能力，宿主模块负责调用；能力名称本身不会创建页面、注册接口或替换现有逻辑。

## 支持范围

| 要做的事 | 当前接入方式 |
| --- | --- |
| 在模型页提供模型资料、搜索和分页 | 已有入口。实现 `catalog.models.read` 或 `catalog.models.search` v1，上传后即可使用 |
| 给商户、渠道、运营等模块增加只读查询或计算 | 可以自定义能力；需要在宿主增加调用接口，有页面需求时还要接前端 |
| 增加 HTTP 接口、Webhook、定时任务 | 通过宿主源码扩展并重新构建；上传插件包不能注册这些入口 |
| 增加控制台页面、菜单、翻译或登录表单 | 使用前端 `ConsoleExtension` 并重新构建 Next；整站替换使用静态模板 |
| 增加模型推理协议、渠道适配器、路由策略 | 修改对应 Rust 模块；当前没有上传包替换这些模块的接口 |
| 接入外部账号认证、账户资料或余额 | 使用宿主的类型化连接器接口；当前通用进程插件不提供这些能力 |
| 修改价格、计费、钱包、结算、身份权限或数据库结构 | 由宿主业务代码和迁移处理，没有通用插件写入接口 |

前端扩展见[前端模板与扩展](frontend-template-development.md)。`RuntimeExtensions` 是编译进服务的 Rust 扩展接口，和上传安装的 `.afplugin` 是两套机制。

通用进程插件只接受 `access: "read"`、`sideEffect: "none"`，并且必须同时提供输入、输出 Schema。协议中虽有写能力和外部操作的类型定义，当前运行时不会因此开放这些操作。

插件接收公开配置和本次调用输入。宿主不向它提供数据库连接、用户登录会话、渠道密钥、托管 HTTP 客户端或持久存储接口。不要把密码、Token 放进 `publicConfig` 或安装包。普通进程仍受服务账号的操作系统权限影响，应安装可信代码；各隔离策略见[安装与运维](plugin-sidecars.md#选择隔离策略)。

## 先跑通模型目录示例

示例位于 [`tools/plugins/examples/catalog/`](../tools/plugins/examples/catalog/)，包含两个模型的资料，支持搜索和分页。

在仓库根目录执行：

```bash
python tools/plugins/package-example.py dist/catalog-example.afplugin
```

打包需要 Python 3.10 或更新版本。输出文件必须尚不存在。打包工具会自动加入 SDK，不需要配置签名。

1. 管理员打开「插件管理」，上传生成的包，点击「安装并启用」。示例不需要配置，保留 `{}`。
2. 登录后进入模型页的「插件模型资料」，选择 `Example model catalogue`。
3. 搜索 `vision`，应看到 `Example Vision`。

这个示例用于 Linux，入口是 `/usr/bin/python3`。每个运行节点都要有该解释器。Windows 需要单独打包 Windows 可执行文件及其依赖，不能直接运行这个 Linux 入口。宿主不会自动安装依赖或转换目标平台。

目录中的模型只用于展示资料。安装示例不会新增可调用模型、渠道或价卡。

## 改成自己的模型目录

先沿用示例结构，修改数据；跑通后再替换查询逻辑：

```text
my-catalog/
├── manifest.json       # 从示例复制，修改插件 ID、名称和版本
├── plugin.py           # 从示例复制，负责读取、搜索和分页
├── models.json         # 自己的模型资料
└── anyflows_plugin.py  # 从 sdk/python/ 复制
```

四个文件均放在同一目录。`manifest.json` 的 `pluginId` 建议使用自己的命名空间，例如 `com.example.catalog`；不要沿用示例 ID，以免覆盖示例部署。修改插件内容后也应更新 `pluginVersion`。

`models.json` 是数组。一个模型的最小记录是：

```json
[
  {
    "id": "example-text",
    "displayName": "Example Text",
    "provider": "example"
  }
]
```

打包时分别指定分发目录、Manifest、目录内入口和输出路径：

```bash
python tools/plugins/package-plugin.py my-catalog my-catalog/manifest.json plugin.py dist/my-catalog.afplugin
```

输出文件应在分发目录外。工具会把分发目录内的普通文件一并打包，先清理缓存、凭据和无关文件。示例 Manifest 中的 `configSchema` 只接受空对象；如果插件需要配置，同时修改 Schema 和处理函数。

### 处理函数负责什么

SDK 的入口是 `Plugin(manifest, handler).serve()`。`manifest` 从随包的 `manifest.json` 读取；`handler` 接收三个参数：

| 参数 | 内容 |
| --- | --- |
| `capability_id` | 本次调用的能力名，例如 `catalog.models.read` |
| `public_config` | 管理员为当前部署保存的公开配置 |
| `value` | 宿主传入的业务输入 |

处理函数返回业务 JSON，不需要自己拼协议外层。SDK 负责握手、健康响应、请求关联、截止时间和错误码。完整入口可直接参考 [`plugin.py`](../tools/plugins/examples/catalog/plugin.py)。

预期的业务失败使用固定错误码，例如：

```python
raise PluginError("unsupported")
```

错误码列表见 [SDK](../tools/plugins/sdk/python/anyflows_plugin.py)。不要将异常信息作为结果返回，也不要向标准输出打印日志；标准输出只能写协议帧，日志写标准错误。

## 模型目录的输入和输出

`catalog.models.read` 用于浏览，`catalog.models.search` 表示支持搜索。目录入口有搜索词时优先调用搜索能力，否则优先调用浏览能力；缺少对应能力时会使用另一个已授权能力。因此只实现浏览能力时，也要能接收 `query` 字段。

插件收到的输入：

```json
{"query":"vision","cursor":null,"limit":20}
```

| 字段 | 约定 |
| --- | --- |
| `query` | 搜索词或 `null`；宿主最多接受 128 字节，插件决定匹配规则 |
| `cursor` | 插件上一页返回的游标，第一页为 `null`；宿主封装、校验后再交回插件 |
| `limit` | 本页最多返回多少条，范围 1–50，同时受 Manifest 的 `maxPageSize` 限制 |

处理函数返回：

```json
{
  "models": [
    {
      "id": "example-vision",
      "displayName": "Example Vision",
      "provider": "example",
      "description": "支持文本和图片输入",
      "contextWindow": 32768,
      "inputModalities": ["text", "image"],
      "outputModalities": ["text"],
      "tags": ["vision"]
    }
  ],
  "nextCursor": null
}
```

必填模型字段为 `id`、`displayName`、`provider`。其余字段可省略；不要返回价格、渠道地址、密钥或额外字段。

| 字段 | 宿主限制 |
| --- | --- |
| `id` | 非空，最多 256 字节，同页不能重复 |
| `displayName` / `provider` | 非空，分别最多 128 / 64 字节 |
| `description` | 有值时非空，最多 4096 字节 |
| `contextWindow` | 正整数，最大 2147483647 |
| `inputModalities` / `outputModalities` | 仅 `text`、`image`、`audio`、`video`，各项不能重复 |
| `tags` | 最多 32 项，每项非空且最多 64 字节，不能重复 |
| `nextCursor` | 无下一页时为 `null`；否则为最多 512 字节的非空字符串，不能与当前游标相同 |

文本不能包含控制字符或首尾空白。插件自己的 Schema 可以更严格；上表是宿主的最终检查。每页条数不得超过输入的 `limit`，输出最多 256 KiB。

对外接口是：

```text
GET /api/account/plugin-models/sources
GET /api/account/plugin-models/{plugin_id}?q=vision&limit=20
```

需要登录会话，或具备 `models:read` 的系统 API Token；模型调用 API Key 不能用于这两个接口。翻页时把接口响应的 `nextCursor` 原样作为 `cursor` 参数传回。搜索词或部署版本改变后，从第一页开始，不能复用旧游标。

## 针对其他模块开发

例如，要给商户页增加“供给说明”，可以约定 `merchant.supply.notes.read` v1。这个名称是接入示例，当前没有现成接口。Manifest 没有 `targetModule`、路由或事件钩子字段，不能靠命名把插件挂到商户页。

需要完成以下工作：

1. **确定数据边界。** 商户模块定义输入、输出 Schema。只把当前用户有权读取的必要数据交给插件，不传登录会话或凭据。
2. **实现能力。** 插件按约定返回数据，声明 `read`、`none` 和两个 Schema。
3. **增加宿主接口。** 在商户模块校验用户、角色、资源归属和业务权限后，读取指定插件的当前部署及 `generation`。插件选择、能力名和版本由宿主控制，不直接采用客户端传来的任意值。
4. **调用插件。** 设置请求 ID、截止时间和输出大小上限，调用 `AdminPluginSidecarRuntime::invoke_capability`。
5. **处理结果。** 将返回 JSON 转成模块自己的类型；返回用户前复核部署代次、授权和撤销状态。定义超时、插件不可用时的页面行为。
6. **接入页面。** 在商户页调用这个宿主接口。需要新页面或菜单时，按前端扩展机制接入。
7. **授权并部署。** 上传包后，在高级管理中明确选择能力及配置，再部署。自定义能力不会走目录插件的自动启用流程。

宿主调用形状如下。此片段放在已经完成业务鉴权的 handler 或服务中，变量由该模块准备：

```rust
let result = runtime
    .invoke_capability(
        &repository,
        plugin_id,
        generation,
        &context,
        "merchant.supply.notes.read",
        1,
        input,
    )
    .await?;
```

`repository` 是 `PluginCatalogRepository`；`context` 是 `CapabilityExecutionContext`，包含本次请求 ID、截止时间、输出上限等信息。运行时会使用数据库中该部署的配置，不能由客户端替换。可通过 `DatabaseAdminPluginLifecycleService::capability_runtime()` 获取运行时；宿主 HTTP 扩展也可使用 `RuntimeHttpContext::plugin_capability_runtime`。

完整接入范例见 [`plugin_catalog_extension.rs`](../crates/af-admin/src/plugin_catalog_extension.rs)，调用实现见 [`plugin_runtime.rs`](../crates/af-admin/src/plugin_runtime.rs)。当前没有“客户端指定任意能力名并直接调用”的通用 HTTP 接口。

新增宿主接口或前端接线需要重新构建、部署 AnyFlows。入口完成后，同一契约内替换插件实现才只需上传新包。不要用插件返回的数据直接改写权限、价卡或资金状态。

## 发布前核对

- Manifest 中的 ID、版本和能力声明与程序握手响应一致，输入、输出、配置符合 Schema。
- 正常结果、空结果、分页、错误输入、超时和缺少权限都有明确处理。
- 包含目标平台所需依赖；每个节点具备相同运行条件。
- 包内文件只读，工作目录是启动副本。持久数据由宿主接口管理，不能依赖修改包内文件保存状态。
- 标准输出无调试信息；签名可选，更新内容后重新打包。

安装、升级和多节点排障见[插件安装与运维](plugin-sidecars.md)。Manifest 字段、协议帧、资源限制和签名算法见[协议参考](plugin-protocol-reference.md)。
