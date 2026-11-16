# AnyFlows 上传插件开发入门

这份指南面向第一次制作 AnyFlows 插件的人。若你要做的功能已有页面入口，只需按现有契约编写插件、打包并上传，不需要改 Rust 源码。若要把插件接入新模块，先让 AnyFlows 宿主增加一个固定入口；仅在 Manifest 中声明新能力不会自动增加 API、菜单或功能。

## 先看支持范围

| 目标 | 当前支持 | 插件能做什么 |
| --- | --- | --- |
| 在模型页提供资料、搜索和分页 | 已支持：catalog.models.read / catalog.models.search v1 | 返回展示用的模型资料，不会新增可调用模型、渠道或价卡 |
| 检查商户自己的服务资料 | 已支持：merchant.service.review.read v1 | 阅读商户服务说明和模型能力，返回文字建议；这些由商户填写的字段会发送给所选插件，安装前应确认信任该插件 |
| 总结管理员配置的渠道监控历史 | 已支持：channel.monitor.events.review.read v1 | 阅读最多 100 条历史事件，管理员在当前请求中查看摘要和建议 |
| 处理公开商户服务状态事件 | 已支持：events.merchant-service.changed.consume v1 | 接收服务 ID、已发布版本、active/paused 状态和时间；不包含商户身份、说明、模型、价格或上游地址 |
| 处理持久渠道健康事件 | 已支持：events.channel-health.changed.consume v1 | 消费 down / recovered 状态变化并返回有界结果；管理员可在插件管理 API 查询结果，插件建议不触发其他操作 |
| 静态检查管理员配置的渠道模型映射 | 已支持：channel.model_mapping.review.read v1 | 阅读所选渠道的模型名和映射值，返回摘要与建议；这些是管理员填写的数据，会发送给所选插件，不连接上游或修改渠道 |
| 给其他模块增加只读查询或计算 | 逐模块接入 | 宿主提供固定入口、数据投影、Schema、权限和结果展示 |
| 增加 HTTP API、Webhook、后台任务 | 只有渠道健康事件消费已接入 | 宿主通过固定事件生产者、持久队列和 Sidecar worker 处理事件；通用 Webhook、插件自建计划任务仍不支持 |
| 声明插件管理页静态信息卡 | 已支持：`plugin_management` 的 `info_card` | Manifest 最多声明 16 个中英文纯文本卡片，不支持菜单、链接、按钮或动态页面 |
| 增加其他控制台页面、菜单、翻译或登录表单 | 上传插件不支持 | 源码编译的 ConsoleExtension 随前端构建；上传包不能加载页面代码 |
| 替换模型协议、渠道适配器、实时路由策略 | 上传插件不支持 | 由 Rust 宿主实现；上传包不能替换实时请求路径 |
| 修改价格、计费、钱包、结算、权限或数据库结构 | 上传插件不支持 | 由宿主业务代码处理，插件不能绕过权限与账务规则 |
| 接入外部账号登录、账户资料或余额 | 不通过通用 Sidecar | 使用宿主提供的类型化连接器 |

上传安装的 `.afplugin`、编译进服务的 Rust `RuntimeExtensions`、编译进前端的 `ConsoleExtension` 是三种不同机制。`.afplugin` 可从插件管理上传安装；`RuntimeExtensions` 要改 Rust 并重建服务；`ConsoleExtension` 要改 Next 源码并重新构建前端。插件包可以在插件管理页声明受限静态信息卡，但不能注册菜单、接口或后台任务。

外部平台连接器和受控写操作仍处于设计阶段。已接入的渠道健康、商户服务事件处理及插件管理静态卡片见[插件能力扩展设计](plugin-capability-expansion.md)，可运行的事件插件示例位于 `tools/plugins/examples/channel-health-event/` 和 `tools/plugins/examples/merchant-service-event/`。

通用 Sidecar 目前只执行管理员明确授予的 access=read、sideEffect=none 能力，并要求每项能力都有输入和输出 JSON Schema。插件没有数据库句柄、用户会话、渠道密钥、托管 HTTP 客户端或通用数据库持久化接口；它只能处理宿主给出的本次输入和已配置的 publicConfig。宿主控制实际调用的插件、能力名、版本、用户权限及数据范围。没有“客户端任意选择 capability 并直接调用”的通用接口。

持久事件当前由两个业务事务内的固定生产者创建：渠道健康从 `down` 变为 `recovered` 或相反的转换，以及商户公开服务变为 `active` 或 `paused`。商户事件只投影服务 ID、已发布版本、状态和时间。处理结果保存在宿主队列中，有效期为 30 天；管理员可在“插件管理”的事件结果区域查看，也可调用 `GET /api/admin/plugins/events/results?before_id=<deliveryId>&limit=20` 分页查询。PAT 权限为 `admin:list_admin_plugin_event_results:execute`，普通用户不能读取。插件输出限制为 16 KiB JSON，不作为命令、通知或业务写操作执行。管理员上传的普通插件包会随安装流程完成制品登记；手工登记的制品需要先通过制品审批，再部署并授予精确能力。

## 从零制作第一个插件

开发者只需准备：

- Python 3.10 或更新版本，用于运行打包命令和 Python 示例。
- 一个 AnyFlows 源码副本，用于读取示例和调用打包工具。
- 一个可用于安装测试包的 AnyFlows 环境。要使用已有商户或监控入口，还需要对应权限的用户或管理员账号。

先选一个已经接入的业务入口。示例目录和打包命令如下：

如果仓库根目录下还没有 `dist` 文件夹，先创建它。Linux/macOS 使用 `mkdir -p dist`；Windows PowerShell 使用 `New-Item -ItemType Directory -Force dist`。

    python tools/plugins/package-example.py dist/catalog-example.afplugin
    python tools/plugins/package-example.py dist/channel-health-event.afplugin --example channel-health-event
    python tools/plugins/package-example.py dist/merchant-service-event.afplugin --example merchant-service-event
    python tools/plugins/package-example.py dist/merchant-review.afplugin --example merchant-service-review
    python tools/plugins/package-example.py dist/channel-monitor-review.afplugin --example channel-monitor-review
    python tools/plugins/package-example.py dist/channel-model-mapping-review.afplugin --example channel-model-mapping-review

这些命令会从仓库复制示例、自动加入 Python SDK，再生成 .afplugin。输出文件必须尚不存在。

要制作自己的插件，先把对应示例目录复制到一个新目录，例如 my-merchant-review。然后：

1. 在 manifest.json 中把 pluginId 改成自己的命名空间，例如 com.example.merchant-review；不要复用示例 ID。
2. 修改 displayName 和 pluginVersion。插件内容更新后增加版本号。
3. 修改 plugin.py 中的检查逻辑。现有接入的 capability ID、版本及 Schema 必须保持一致。
4. 从 tools/plugins/sdk/python/ 复制 anyflows_plugin.py 到自己的分发目录。
5. 打包并上传：

       python tools/plugins/package-plugin.py my-merchant-review my-merchant-review/manifest.json plugin.py dist/my-merchant-review.afplugin

6. 管理员在控制台打开“插件管理”并上传包。如果页面提示需要授权，在“高级管理”中为该部署明确授予示例要求的 capability，然后部署。
7. 等插件节点状态显示就绪，再到对应业务页面运行插件。
8. 更新代码时增加 pluginVersion，重新打包并升级，不要覆盖已登记制品。

自己打包时 SDK 文件必须位于分发目录中。package-plugin.py 只打包该目录，不会自动复制 SDK、安装 Python 或外部依赖。内置示例使用 package-example.py，因此不需要手工复制 SDK。

插件包使用 Linux 示例入口 /usr/bin/python3。每个运行节点都必须具备清单指定的解释器和依赖。当前不会按节点操作系统或 CPU 架构自动切换包。Windows 节点需要单独准备 Windows 可执行入口及所需文件。

### Manifest 字段

Manifest 是插件与宿主之间的约定。修改已有业务插件时，按对应示例保留原有 Schema；不要为了减少字段随意放宽 Schema。

| 字段 | 作用 |
| --- | --- |
| pluginId | 插件身份。使用自己的命名空间，升级时保持不变 |
| displayName | 页面中显示的名称 |
| pluginVersion | 插件版本。发布不同内容时更新 |
| contractRange | 声明兼容的插件协议版本 |
| capabilities | capability ID、版本、read/write、sideEffect 和输入输出 Schema |
| authenticationModes | 当前通用 Sidecar 使用 ["none"]，不代表插件获得 AnyFlows 登录会话 |
| limits | 并发、请求超时、JSON 值大小和分页上限；宿主可以继续收紧 |
| configSchema | 管理员可以设置的公开配置格式；不要把密码、API Key、Token、Cookie 放入配置 |

完整字段定义见[协议参考](plugin-protocol-reference.md#manifest-字段)。一般插件不需要签名；签名只用于核对发布者身份，不替代能力授权。

### Python 处理函数

Python SDK 的入口是 Plugin(manifest, handler).serve()。handler 接收 capability_id、public_config 和 value：

    from anyflows_plugin import Plugin, PluginError

    def handle(capability_id, public_config, value):
        if capability_id != "my.fixed.capability":
            raise PluginError("unsupported")
        if public_config:
            raise PluginError("invalid_configuration")
        # value 是宿主传入的 JSON 对象。
        # 校验必填字段，计算结果后返回普通 Python dict。
        return {"summary": "检查完成", "checks": []}

SDK 负责 JSON Lines 握手、健康响应、请求关联、截止时间和错误帧；handler 只返回业务 JSON。预期的业务错误使用 SDK 中固定的 PluginError 错误码。不要把异常、文件路径或密钥写进输出。

一个完整入口还需要从同目录读取 manifest.json，并启动 SDK：

    import json
    from pathlib import Path
    from anyflows_plugin import Plugin, PluginError

    def handle(capability_id, public_config, value):
        if capability_id != "my.fixed.capability":
            raise PluginError("unsupported")
        # 在这里校验 value，并返回符合 outputSchema 的字典。
        return {"summary": "检查完成", "checks": []}

    if __name__ == "__main__":
        manifest = json.loads(Path("manifest.json").read_text(encoding="utf-8"))
        Plugin(manifest, handle).serve()

协议帧走标准输入和标准输出。不要向标准输出打印日志；调试日志写标准错误。各语言都可以实现插件，但必须与[协议参考](plugin-protocol-reference.md)中的 JSON Lines 协议、字段类型和错误码一致。

## 当前已接入的四个业务入口

### 模型资料目录

模型目录示例位于 [tools/plugins/examples/catalog/](../tools/plugins/examples/catalog/)。输入格式如下：

    {"query":"vision","cursor":null,"limit":20}

Manifest 的输入 Schema 必须声明 `query`、`cursor`、`limit` 三个属性，其中 `limit` 必填，`query` 和 `cursor` 不要放进 `required`。模型页会始终发送这三个字段；未填写搜索词或第一页游标时，值为 `null`。`limit` 范围是 1–50，并受 Manifest `maxPageSize` 限制。没有搜索词时返回未过滤的目录。`catalog.models.search` 表示该插件同时声明搜索能力；它和 `catalog.models.read` 使用相同的输入输出格式。

宿主对搜索词最多接受 128 字节、对请求游标最多接受 2048 字节。示例 Manifest 可以设置更严格的长度，但不能放宽宿主限制；JSON Schema 的 `maxLength` 按字符计算，宿主列出的上限按 UTF-8 字节计算。

插件返回一个模型列表和 nextCursor：

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

id、displayName、provider 必填。宿主最多返回 50 项，并校验字段长度、重复 ID、模态、标签和游标。不得返回价格、渠道地址、密钥或 Schema 外字段。

| 字段 | 宿主限制 |
| --- | --- |
| id | 非空，最多 256 字节，同页不能重复 |
| displayName / provider | 非空，分别最多 128 / 64 字节 |
| description | 省略或非空，最多 4096 字节 |
| contextWindow | 正整数，最大 2147483647 |
| inputModalities / outputModalities | 仅 text、image、audio、video，每项不能重复 |
| tags | 最多 32 项，每项非空且最多 64 字节，不能重复 |
| nextCursor | 无下一页为 null；有下一页时最多 512 字节，不能与当前游标相同 |

文本不能包含控制字符或首尾空白。插件自己的 Schema 可以更严格；宿主仍会做最后校验。宿主最多接受 256 KiB 输出。示例 Manifest 把模型字段和下一页游标设得更短，例如 `nextCursor` 最多 8 个字符；若数据源需要更长游标，应在宿主允许的上限内同步调整 Manifest 和处理函数的校验。

对外接口：

    GET /api/account/plugin-models/sources
    GET /api/account/plugin-models/{plugin_id}?q=vision&limit=20

这些接口需要登录会话或 models:read 系统 API Token；模型调用 API Key 不能用于管理接口。翻页时把 nextCursor 原样传回，搜索词或插件部署版本改变后从第一页开始。

四个现有入口的单次调用预算最多 5 秒；目录输出最多 256 KiB，商户和监控检查的输入最多 64 KiB、宿主接受的输出最多 16 KiB，模型映射检查最多接收 512 KiB、输出最多 16 KiB。对应 Manifest 的 `maxResponseBytes` 必须足够接收输入，但不能提高宿主的输出限制。商户、监控和模型映射检查要求空 `publicConfig`；模型目录按各插件的 `configSchema` 处理公开配置。

### 商户服务资料检查

完整新手示例和逐字段说明见 [merchant-service-review/README.md](../tools/plugins/examples/merchant-service-review/README.md)。固定能力为 merchant.service.review.read v1，部署 publicConfig 必须为空对象 {}。

宿主验证当前商户对服务的归属，然后只提供服务名称、说明、协议、公开模型 ID 和模型能力。不会传商户或用户 ID、上游地址、上游模型映射、API Key、价格、倍率或财务信息。插件只返回摘要和建议，不会改变服务资料、审核状态、价格或资金。

接口和 PAT 权限：

    GET  /api/merchant/plugin-reviewers
    POST /api/merchant/services/{id}/plugin-review

商户 PAT 读取插件列表需要 merchant_plugins:read；运行检查需要 merchant_plugins:execute。插件只收到请求体中的 plugin_id，能力名和版本由宿主固定。普通用户只能检查自己的服务。

### 渠道监控事件分析

完整新手示例见 [channel-monitor-review/README.md](../tools/plugins/examples/channel-monitor-review/README.md)。固定能力为 channel.monitor.events.review.read v1。该入口只供管理员总结已经记录的监控事件，不执行探测、不管理监控目标，也不改变渠道状态、健康分或实时路由。

输入最多含 100 条 down / recovered 事件及延迟和 Unix 时间；不包含目标 ID、渠道 ID、凭据 ID、模型名、接口地址、响应正文或 API Key。空历史也是有效输入，插件应说明没有可分析事件，不要把“没有记录”解释为健康。

插件选择与执行接口：

    GET  /api/admin/channel-monitor/plugin-reviewers
    POST /api/admin/channel-monitor/targets/{id}/plugin-review

这是管理员接口。PAT 权限也仅授予管理员，分别为 admin:list_admin_channel_monitor_plugin_reviewers:execute 和 admin:review_admin_channel_monitor_target_with_plugin:execute。普通用户 PAT 不能调用。

### 渠道模型映射静态检查

完整逐步示例见 [channel-model-mapping-review/README.md](../tools/plugins/examples/channel-model-mapping-review/README.md)。固定能力为 `channel.model_mapping.review.read` v1，只供管理员对指定渠道执行静态分析。

输入只包含渠道公开模型名列表和精确模型映射，例如：

    {"models":["gpt-4o"],"mappings":[{"model":"gpt-4o","upstreamModel":"vendor-chat-large"}]}

模型列表和映射列表各最多 512 项；每个模型名最多 256 个 UTF-8 字节。宿主会先确认渠道存在并且调用者是管理员，再构造不超过 512 KiB 的最小数据投影。渠道 ID、名称、上游地址、凭据、请求头、参数覆盖、余额和运行日志都不会传给插件。插件只返回摘要与建议，不会连接上游、修改模型清单或映射。

接口为：

    GET  /api/admin/channel-model-mapping/plugin-reviewers
    POST /api/admin/channels/{channel_id}/model-mapping/plugin-review

部署必须明确授予 `channel.model_mapping.review.read` v1，且 `publicConfig` 必须为空对象。管理员 PAT 分别需要 `admin:list_admin_channel_model_mapping_plugin_reviewers:execute` 和 `admin:review_admin_channel_model_mapping_with_plugin:execute`。完整输入输出 Schema、安装步骤和可修改规则见示例 README。

该入口最多允许 5 秒调用、512 KiB 输入和 16 KiB 输出。Manifest 的 `maxResponseBytes` 至少要为 512 KiB；插件处理过程仍应快速返回，不应执行外部网络或长时间任务。

## 想给新模块加插件入口

若当前没有合适入口，插件作者不需要先读 Rust 代码。先整理需求，交给 AnyFlows 维护者评估宿主集成：

1. **使用场景**：谁会在什么页面、什么操作时运行插件？
2. **输入数据**：提供一份最小 JSON 样例；逐字段说明来源、用户能否查看、是否属于凭据或财务数据。
3. **输出结果**：提供 JSON 样例，说明它只展示建议，还是会影响宿主行为。
4. **权限和归属**：哪些用户可以运行，如何验证他们对记录的访问权？管理员功能是否只能管理员使用？
5. **失败行为**：插件超时、未启用、无权限或返回无效 JSON 时，宿主如何安全失败？
6. **页面位置**：已有页面上加一个按钮，还是需要独立页面？

维护者据此定义固定 capability ID/version 和 Schema，在宿主加入认证、PAT 权限、资源归属检查、限时调用、输出校验及前端入口。开发完成并部署后，插件作者才需要按固定输入输出实现 handler。不要只在 Manifest 中自创 ID，页面不会自动发现它。

一个新功能的提案可以先写成：

    使用者：AnyFlows 管理员
    入口：渠道模型映射页面的“检查映射”
    插件输入：平台公开模型名、上游模型名和映射选项，不含密钥或接口地址
    插件输出：摘要、问题列表和建议
    副作用：无；不自动改写模型映射
    失败策略：超时或结果不合 Schema 时显示不可用，不影响当前映射

### 插件接入地图

表中“候选”表示可以由 AnyFlows 维护者评估的新宿主入口，不代表上传插件已经能直接使用。每个入口都需要宿主明确数据投影、固定 capability、输入输出 Schema、权限和界面；Manifest 不能自行开启这些能力。

| 模块 | 已接入能力 | 适合评估的下一步 | 接入时的边界 |
| --- | --- | --- | --- |
| 模型广场 | 读取、搜索插件提供的公开模型资料 | 提供商类别、能力标签或兼容性资料筛选 | 插件只给资料；模型是否可调用、提供商 Logo 和来源可信度由宿主管理 |
| 商户工作台 | 检查本人服务的公开说明和模型能力 | 服务发布资料、公开模型 ID 与能力声明检查 | 只投影当前商户有权查看的字段；建议由商户确认，不自动提交或改变审核状态 |
| 渠道管理 | 分析已记录的监控事件；静态检查模型映射 | 静态配置诊断、脱敏健康历史说明 | 不提供 API Key、渠道凭据或上游地址；不执行探测、不参与实时路由、不改健康分 |
| 管理与运营 | 暂无通用插件入口 | 对人工填写内容或脱敏汇总数据做检查和报告 | 由宿主决定可见数据、管理员权限和审计范围；原始日志需先去除密钥及个人资料 |
| 账号、钱包和结算 | 不由通用 Sidecar 接入 | 如需第三方平台连接，使用类型化的宿主连接器并单独设计授权 | 插件不得读取用户会话、账户余额或账本，也不得发起扣款、退款、提现或结算 |
| 定时任务和通知 | 上传插件没有通用后台任务钩子 | 由宿主提供有范围的事件/任务契约，再评估插件处理 | 需要宿主管理触发条件、幂等、重试、撤销和审计；不能靠插件进程常驻或自行访问网络 |

新增只读入口时，宿主先整理本模块允许提供的最小数据快照，插件返回符合 Schema 的摘要或检查建议，宿主再检查资源归属和数据版本。若插件需要修改业务数据，宿主必须提供单独的操作入口，让用户确认并按现有权限执行；分析结果本身不能触发写操作。

### 哪些改动由谁完成

| 想增加的内容 | 需要做什么 |
| --- | --- |
| 修改现有插件检查规则或静态资料 | 按已发布 Schema 修改插件代码，更新版本并重新打包 |
| 让插件可在一个新业务页面运行 | AnyFlows 宿主增加固定 capability、数据投影、鉴权/PAT 权限、结果校验、API 文档和页面入口 |
| 增加新的输入输出字段 | 维护者更新宿主 Schema 和兼容版本；作者按发布后的契约更新插件 |
| 展示独立菜单或插件自定义页面 | 修改并重新构建 Console 前端；上传包自身不能注册菜单或插入页面 |
| 运行计划任务、Webhook 或数据写操作 | 先在宿主设计固定触发器、权限、幂等和审计，再添加受限扩展接口；当前上传插件没有这些通用接口 |
| 访问外部账号、余额或其他系统 | 通过宿主类型化连接器实现，单独处理凭据保管、用户授权、资金一致性和审计 |

通用 Sidecar 不提供任意 HTTP 代理、数据库连接、环境变量读取或宿主凭据。候选场景只有在宿主明确发布契约后才能开发；插件作者不必先读 AnyFlows 源码，只需跟随该入口的示例和字段说明。

## 调试和发布前检查

插件包可通过 package-example.py 或 package-plugin.py 生成；这只是打包，不会执行 handler。最可靠的验收方式是在隔离的 AnyFlows 测试环境上传并运行示例，然后再部署你自己的版本。

如果问题来自插件本身，可在 handler 中临时对一个已知 JSON 输入调用纯函数并检查返回值；不要在标准输出打印诊断信息。示例说明了空输入、未知字段、未知 capability 和结果过大的处理方式。

| 现象 | 先检查 |
| --- | --- |
| 插件没有出现在选择器 | 宿主是否已接入此 capability；插件是否启用、获批、未撤销并授予精确版本 |
| 安装后不能启动 | 入口路径、操作系统/架构、Python 解释器及依赖是否匹配 |
| No module named anyflows_plugin | 自己打包时是否将 anyflows_plugin.py 放入分发目录；内置示例请用 package-example.py |
| 调用返回无效输入 | 请求字段、类型、required 字段和插件 Manifest Schema 是否一致 |
| 调用超时或结果过大 | 缩减处理量并返回简短摘要；Manifest 上限不能提高宿主限制 |
| 插件返回错误 | 使用固定 PluginError 错误码；检查日志时不要记录输入中的隐私信息 |

AnyFlows 源码构建和测试由仓库流水线验证。安装、多节点、升级和隔离策略见[插件安装与运维](plugin-sidecars.md)；Manifest、包格式、完整协议帧、签名及资源限制见[协议参考](plugin-protocol-reference.md)。
