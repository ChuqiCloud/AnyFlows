# AnyFlows 插件安装与 Sidecar 协议

插件以独立进程运行，通过版本化 JSON Lines 与 AnyFlows 通信。常规安装只需上传插件包并点击“安装并启用”，无需填写摘要、签名、公钥或单独审批。

## 安装插件

在控制台的“插件管理”中选择 .afplugin 或 ZIP 文件，然后点击“安装并启用”。系统会读取插件信息、检查包内容、计算摘要、保存安装记录和共享归档，并启动本节点插件。其他运行节点自动获取同一部署。仅当插件需要公开配置时，才展开“可选配置”填写 JSON 对象；默认是 {}。配置不得包含密钥、口令、Cookie 或其他凭据。

节点本地缓存目录默认为运行目录下的 data/plugins，服务自动创建所需目录，不要求预先配置发布者公钥。共享归档保存在已有数据库中，无需额外部署对象存储或配置节点之间的传输密钥。需要改变本地缓存位置时，可设置相对于服务运行目录的路径：

    [server]
    plugin_artifact_directory = "data/plugins"

服务账号需要能创建、读写和执行该目录中的插件文件。系统不从任意 URL 下载插件，不提供在线插件市场；管理员应从可信来源取得插件包。

上传压缩包最大为 128 MiB，展开后的文件和元数据合计最大为 512 MiB，最多 100,000 个 ZIP 条目。系统拒绝链接、特殊文件、重复或仅大小写不同的路径、越界路径和不受支持的 ZIP 特性。入口文件必须在 payload/ 内。宿主还会校验版本、配置 Schema、能力声明、启动握手与健康检查。

安装接口为 POST /api/admin/plugins/install，使用 multipart/form-data：

| 字段 | 内容 |
| --- | --- |
| package | 必填，.afplugin 或 ZIP 插件包 |
| config | 可选，JSON 对象字符串，默认 {} |
| expected_generation | 可选；新装不传，升级可显式传当前部署的 generation |

常规安装不需读取 generation；如果插件 ID 已部署，服务端读取当前 generation 并用 compare-and-swap 更新，因此并发部署仍会返回冲突。高级管理保留显式 generation 字段供自定义部署使用。启用、停用和回滚保留版本历史与操作者审计。

安装成功响应表示本次部署已经完成本节点启动检查。集群状态需在“节点运行状态”中查看，不能用部署的 enabled 标志代替所有节点就绪确认。

## 插件包格式

.afplugin 是 ZIP 文件，根目录结构固定：

    example.afplugin
    ├── anyflows-plugin.json
    └── payload/
        ├── bin/
        │   └── example-plugin
        └── assets/
            └── catalog.json

anyflows-plugin.json 包含公开的 PluginManifest 和入口定位信息。executablePath 相对于 payload/，不要写 payload/ 前缀：

    {
      "manifest": {
        "pluginId": "org.example.catalog",
        "displayName": "Example catalog",
        "pluginVersion": "1.0.0",
        "contractRange": {
          "minimum": { "major": 1, "minor": 0 },
          "maximum": { "major": 1, "minor": 0 }
        },
        "capabilities": [
          {
            "id": "catalog.models.read",
            "version": 1,
            "access": "read",
            "sideEffect": "none",
            "inputSchema": { "type": "object" },
            "outputSchema": { "type": "object" }
          }
        ],
        "authenticationModes": ["none"],
        "limits": {
          "maxConcurrency": 1,
          "requestTimeoutMs": 1000,
          "maxResponseBytes": 1048576,
          "maxPageSize": 100
        },
        "configSchema": { "type": "object", "additionalProperties": false }
      },
      "executablePath": "bin/example-plugin"
    }

包内不得包含私钥、访问令牌、.env、用户数据或本地运行配置。为目标操作系统与 CPU 架构打包可执行文件；服务不会把其他架构的程序自动转换为当前架构。

### 打包工具

仓库提供仅使用 Python 标准库的工具 tools/plugins/package-plugin.py，需要 Python 3.10 或更新版本。准备一个仅含分发文件的目录，将上例中的 manifest 对象单独保存为 manifest.json，执行：

    python tools/plugins/package-plugin.py build/example-plugin manifest.json bin/example-plugin dist/example.afplugin

四个位置参数依次是分发目录、Manifest JSON 文件、目录内的 POSIX 入口路径和输出文件。输出必须放在分发目录外，并使用尚不存在的 .afplugin 或 .zip 路径。工具写入固定时间戳和普通文件/目录标识，入口标记为可执行；不保留原目录的所有者、扩展属性或 setuid 权限。目录中的普通文件会全部打包，发布者应先清理分发目录。工具不读取或生成签名私钥，也不能自动识别文件内容中的秘密。

已有发布者签名时，可以附加现有签名文件：

    python tools/plugins/package-plugin.py build/example-plugin manifest.json bin/example-plugin dist/example-signed.afplugin --signer-key-id publisher-example --signature-file signature.base64

signature.base64 只能包含对应此包、入口和 Manifest 的 Base64 Ed25519 签名。工具只是装入既有签名，不生成密钥、不签名，也不会因为填写了签名者 ID 就使插件成为已验证来源。

仓库还提供可复用的 Python SDK 和完整的本地模型资料示例：

    python tools/plugins/package-example.py dist/catalog-example.afplugin

这个未签名示例可以直接上传安装。Linux 目标节点需要 `/usr/bin/python3`；其他系统应提供适配其平台的可执行包。安装后，在登录后的模型广场进入“插件模型资料”，选择示例来源即可搜索、分页查看。示例中的模型仅用于开发，不提供模型推理、价卡或渠道。SDK、真实业务接口和开发方式见 [插件开发与运行隔离](plugin-development.md)。

## 来源标识与可选签名

管理员上传的未签名插件默认可以安装，界面标记为“管理员安装 · 发布者未验证”。这表示系统无法验证发布者身份，不表示插件代码已审核。

需要验证特定发布者时，运营方可选配其 Ed25519 公钥：

    [[server.plugin_trusted_signers]]
    key_id = "publisher-example"
    public_key_base64 = "<发布者的 Ed25519 公钥，Base64 编码的 32 字节>"

签名包在 anyflows-plugin.json 根对象中附加：

    {
      "signature": {
        "signerKeyId": "publisher-example",
        "signatureBase64": "<Base64 编码的 64 字节 Ed25519 签名>"
      }
    }

只有签名者 ID 命中宿主配置、且签名验证成功，才显示“发布者签名已验证”。这不等同于“官方插件”，也不自动授予额外能力。自建插件无需签名；未配置的签名者按未验证来源处理，匹配受信公钥但签名无效时安装失败。普通安装默认只授予 `catalog.models.read` 和 `catalog.models.search` 中由清单声明的只读、无副作用能力；声明了其他能力的插件仍可安装登记，但须在高级管理中逐项确认授权后部署。高级手动登记的签名字段可留空或成对填写。

官方发布者公钥使用单独的配置，不与普通受信发布者混用：

    [[server.plugin_official_signers]]
    key_id = "official-publisher"
    public_key_base64 = "<官方发布者提供的 Ed25519 公钥，Base64 编码的 32 字节>"

这两个列表默认均为空。只有插件元数据的完整签名通过已配置官方公钥验证，界面才显示“官方”；插件名、ID 前缀、签名者 ID 字符串或普通受信公钥都不能单独取得该标识。系统不生成或内置冒充发布者身份的密钥。配置官方公钥是可选的，不影响自有插件安装。多个节点应使用一致的发布者和官方公钥配置。

签名验证发布者身份及所签包内容，不自动授予业务能力。插件的实际操作系统权限取决于宿主选择的隔离策略；默认安装仍面向运营方信任的代码。

### V2 摘要与签名

V2 包摘要以 ANYFLOWS_PLUGIN_PACKAGE_V2\0 开头，对 payload/ 下所有条目按相对路径排序；摘要路径相对 payload 根目录，不包含 payload/ 前缀。每个条目依次写入类型字节（目录 0、普通文件 1）、UTF-8 路径长度（little-endian u64）及路径字节，文件条目再写入文件长度（little-endian u64）及文件内容，对完整序列计算 SHA-256。空目录也纳入摘要，根目录自身与 anyflows-plugin.json 不在该树摘要内。入口与 Manifest 由签名的其他字段绑定。

Ed25519 签名按顺序覆盖以下字节：ANYFLOWS_PLUGIN_ARTIFACT_V2\0、plugin ID、换行、语义化版本、换行、64 位小写十六进制包摘要、换行、相对 payload/ 的 POSIX 可执行入口、换行、Manifest 的紧凑 UTF-8 JSON。Manifest 使用宿主契约类型序列化后的值；对象键按 UTF-8 字节字典序递归排序，数组顺序不变，不加空格，并使用 serde_json 的字符串转义方式。V1 签名不再接受。

安装后，宿主将已校验的包复制到制品目录 .runtime 中的私有随机 staging 目录，对副本再次计算摘要并从副本启动。Unix 上入口权限固定为 0500，其他文件固定为 0400，不继承 setuid 或组/其他用户写权限；.runtime 只应由服务账号访问，所在文件系统须允许执行。

## 多节点安装与就绪状态

使用同一 AnyFlows 数据库的运行节点自动共享通过上传入口安装的插件。ZIP 归档按 `artifact_id` 与不可变制品记录关联，以 48 KiB 分块保存；归档摘要、总字节数、分块数量和全部内容在事务内提交。读取时重新核对分块顺序、长度和归档 SHA-256，避免节点使用不完整归档。

每个节点在启动和每 30 秒协调时读取期望部署。缺少本地包时，从数据库取得归档，经同一受限解包器恢复缓存，再核对插件 ID、版本、Manifest、包摘要和入口路径，最后按本节点信任根及隔离策略验证并启动。各节点的缓存根目录可以不同，无需手工复制插件目录。

旧版本安装以及高级手动登记的本地制品可能没有共享 ZIP 归档。需要自动分发时，管理员应通过上传入口重新提供对应安装包；系统不会把任意本地目录隐式上传到数据库。

统一分发不转换可执行文件的目标平台。运行同一个插件部署的节点应具备兼容的操作系统、CPU 架构、解释器、动态库和隔离工具；当前没有按节点架构自动挑选不同包的机制。不兼容或缺少依赖的节点启动失败，并在运行状态中保留固定错误码。

插件管理的“节点运行状态”读取 `GET /api/admin/plugins/runtime`。每个服务进程实例有自己的 node ID，并上报以下信息：

| 信息 | 用途 |
| --- | --- |
| `generation`、`digest` | 确认节点运行的代次和包与当前部署一致 |
| `status` | `pending`、`downloading`、`starting`、`ready`、`stopped`、`failed` 或 `stale` |
| `isolation` | 该插件进程实际采用的隔离级别，尚未启动时可能为 `unknown` |
| `errorCode` | 固定错误分类，不包含凭据、进程输出或本地路径 |
| `updatedAt`、`nodeLastSeenAt` | 区分插件观察时间和节点心跳时间 |

节点和插件观察的租约均为 120 秒。进程退出、节点失联或状态过期后不能继续算作就绪；服务重启会产生新实例，旧实例记录按租约显示为过期。只有处于有效租约、`generation` 和 `digest` 都匹配且状态为 `ready` 的节点，才计入该部署的就绪数量。

`enabled` 表示管理员希望插件启用。安装响应和核心 `/readyz` 都不能代替逐节点插件状态；插件尚未完成拉取或启动时，控制台显示待就绪、失败或过期状态。

## 停用、卸载与制品回收

停用会保留安装文件和历史，便于重新启用。卸载则创建新的停用代次，并在同一事务内保存逐节点清理意图和审计。收到卸载意图的节点终止对应进程并清理受控缓存；离线节点在恢复协调后继续执行。新代次不会被旧的清理请求覆盖。

卸载接口为 `POST /api/admin/plugins/{plugin_id}/uninstall`，JSON 请求包含当前 `expected_generation`。响应的 `pendingNodes` 是尚未确认该停用代次的在线节点数。卸载保留制品元数据、部署历史、审计和数据库归档；后续合法部署或回滚仍可按规则重新恢复包。

需要永久释放归档空间时，先撤销对应制品，再调用 `POST /api/admin/plugins/artifacts/{artifact_id}/cleanup`。只有已经撤销且不被任何启用部署引用的制品可以回收。该操作删除共享归档，并保存各节点本地缓存清理意图；不会删除制品元数据或历史审计。回收意图持久化后，不能通过重新上传同一制品恢复被清理的归档，应发布新的合法制品。

本地文件清理只处理导入器管理的缓存目录；高级登记的任意路径不会成为递归删除目标。同一缓存仍被其他启用部署引用时，节点保留该目录。控制台分别展示归档删除和本节点文件删除结果，远端节点在后续协调中处理自己的缓存。

## 调用与运行边界

标准输入每行一个 UTF-8 JSON PluginHostRequest，标准输出每行一个 JSON PluginHostResponse，标准错误不进入协议。请求包含协议版本、插件 ID/版本、制品摘要、单调递增部署 generation、request ID、Unix 毫秒 deadline 与操作。启动时宿主先读取 Manifest，再请求无副作用 health；身份、版本、摘要和 Manifest 必须一致，且 health 为 healthy 才可调用。

通用调用只接受 access=read、sideEffect=none 且具有输入/输出 Schema 的能力；外部账户资料和余额能力不在此通用路径内。简易安装仅自动授权固定的公开目录只读能力；其他能力需要管理员在高级部署中明确选择。输入、输出和公开配置仍分别按声明的 JSON Schema 校验。写能力、身份关联、钱包和其他资金行为保留在宿主业务工作流中；插件不能直接持有数据库句柄或钱包权限。

每个进程的请求串行执行，排队与处理共同受 deadline 约束；时间和输入/输出大小采用宿主、Manifest 与调用方限制的交集。握手、健康检查、协议格式、身份或输出校验失败时终止进程。取消、超时或异常会清理进程，避免未读响应污染后续请求；正常 capability 业务错误不会自动退出进程。宿主不把调用失败的请求自动重放到恢复进程。

宿主清空子进程环境，仅保留 Windows 运行所需的系统变量，工作目录固定为 staging 副本。入口和隔离工具均使用受宿主控制的绝对路径，不运行插件提供的 shell 命令。Unix 使用独立进程组；Windows 在恢复子进程执行前将其加入 Job Object，停用、超时、异常或句柄释放时清理后代进程。

后台每 30 秒恢复仍启用的故障进程；服务启动时同样协调启用部署。停用 generation 保留运行时水位，旧部署不能在完成握手后重新覆盖新状态。登记、批准、撤销和部署 revision 的 catalog_* 审计与相应目录变更处于同一数据库事务；运行时结果单独记录，启动失败可能追加补偿停用 revision。

### 运行隔离

隔离策略通过 `server.plugin_isolation` 选择，默认 `auto`；不增加插件包签名或安装审批步骤。

| 策略 | 实际行为 |
| --- | --- |
| `auto` | Linux 有可信 `setpriv`、`prlimit` 时启用 no-new-privileges，并收紧 core dump、文件大小及打开文件数；缺少工具时保留进程管理。Windows 使用 Job Object 管理进程树 |
| `trusted` | 明确使用受信插件的宿主账户权限；仍保留进程组或 Job Object 清理 |
| `restricted` | Linux 必须具备上述工具，并将每个进程的地址空间限制为不超过 4 GiB、累计 CPU 时间不超过 300 秒；不支持的平台启动失败 |
| `require_sandbox` | Linux 额外要求 Bubblewrap 和内核命名空间支持：仅挂载系统运行文件与插件副本，根文件系统只读，独立 `/tmp`、网络及进程命名空间；缺少能力时启动失败 |

资源上限只会收紧已有宿主限制。`auto` 为兼容运行时的虚拟地址预留，不限制地址空间；需要固定内存和 CPU 边界时应选择显式限制策略。显式强隔离失败不会静默退回普通进程。

Windows Job Object 的进程树控制不等同于文件系统或网络沙箱。运行状态按实际情况返回 `process`、`resource_limited`、`sandboxed` 或 `job_object`，而概览顶层的 `isolation` 表示本节点配置的期望策略。详细平台要求见 [插件开发与运行隔离](plugin-development.md)。

### 业务权限边界

登录后的模型资料入口已经接入获授的目录能力，并由宿主处理用户认证、管理 Token 权限、来源标识、输入输出投影及分页游标。此入口提供资料扩展，不授予渠道、价格或资金操作权限。

当前协议没有可供通用 Sidecar 直接使用的凭据代理或上游资金写接口。身份、账本、钱包与其他资金行为仍由宿主业务流程控制，不能通过声明任意 capability 获得数据库或钱包权限。其他可信宿主业务扩展使用内部能力端口时，也必须自行执行用户、租户、资源授权和业务审计。
