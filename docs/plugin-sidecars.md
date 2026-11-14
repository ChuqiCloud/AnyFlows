# AnyFlows 插件安装与 Sidecar 协议

插件以独立进程运行，通过版本化 JSON Lines 与 AnyFlows 通信。常规安装只需上传插件包并点击“安装并启用”，无需填写摘要、签名、公钥或单独审批。

## 安装插件

在控制台的“插件管理”中选择 .afplugin 或 ZIP 文件，然后点击“安装并启用”。系统会读取插件信息、检查包内容、计算摘要、保存安装记录并启动插件。仅当插件需要公开配置时，才展开“可选配置”填写 JSON 对象；默认是 {}。配置不得包含密钥、口令、Cookie 或其他凭据。

制品目录默认为运行目录下的 data/plugins，服务自动创建所需目录，不要求预先配置发布者公钥。需要改变位置时，可设置相对于服务运行目录的路径：

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

签名只验证发布者与包内容，不能证明插件行为安全。所有插件仍以 AnyFlows 服务账号运行，安装者应信任插件代码及其发布者。

### V2 摘要与签名

V2 包摘要以 ANYFLOWS_PLUGIN_PACKAGE_V2\0 开头，对 payload/ 下所有条目按相对路径排序；摘要路径相对 payload 根目录，不包含 payload/ 前缀。每个条目依次写入类型字节（目录 0、普通文件 1）、UTF-8 路径长度（little-endian u64）及路径字节，文件条目再写入文件长度（little-endian u64）及文件内容，对完整序列计算 SHA-256。空目录也纳入摘要，根目录自身与 anyflows-plugin.json 不在该树摘要内。入口与 Manifest 由签名的其他字段绑定。

Ed25519 签名按顺序覆盖以下字节：ANYFLOWS_PLUGIN_ARTIFACT_V2\0、plugin ID、换行、语义化版本、换行、64 位小写十六进制包摘要、换行、相对 payload/ 的 POSIX 可执行入口、换行、Manifest 的紧凑 UTF-8 JSON。Manifest 使用宿主契约类型序列化后的值；对象键按 UTF-8 字节字典序递归排序，数组顺序不变，不加空格，并使用 serde_json 的字符串转义方式。V1 签名不再接受。

安装后，宿主将已校验的包复制到制品目录 .runtime 中的私有随机 staging 目录，对副本再次计算摘要并从副本启动。Unix 上入口权限固定为 0500，其他文件固定为 0400，不继承 setuid 或组/其他用户写权限；.runtime 只应由服务账号访问，所在文件系统须允许执行。

## 多节点范围

当前上传只保存到处理该请求的节点，尚未实现统一制品存储、节点分发或集群安装确认。共享数据库中的启用状态不代表每台节点都有对应插件文件。其他节点每 30 秒协调期望部署，但缺少同一制品、签名配置或执行环境时仍无法启动插件。

因此，当前简易上传安装适用于单节点。已有多节点部署需由运维将同一制品放到每个运行节点的对应相对路径，使用一致的发布者配置，并逐节点确认插件就绪后再承接相关流量。现有 /readyz 不能作为插件集群就绪证明。没有完成统一分发前，不应把“上传成功”视为“全节点安装完成”。

## 调用与运行边界

标准输入每行一个 UTF-8 JSON PluginHostRequest，标准输出每行一个 JSON PluginHostResponse，标准错误不进入协议。请求包含协议版本、插件 ID/版本、制品摘要、单调递增部署 generation、request ID、Unix 毫秒 deadline 与操作。启动时宿主先读取 Manifest，再请求无副作用 health；身份、版本、摘要和 Manifest 必须一致，且 health 为 healthy 才可调用。

通用调用只接受 access=read、sideEffect=none 且具有输入/输出 Schema 的能力；外部账户资料和余额能力不在此通用路径内。简易安装仅自动授权固定的公开目录只读能力；其他能力需要管理员在高级部署中明确选择。输入、输出和公开配置仍分别按声明的 JSON Schema 校验。写能力、身份关联、钱包和其他资金行为保留在宿主业务工作流中；插件不能直接持有数据库句柄或钱包权限。

每个进程的请求串行执行，排队与处理共同受 deadline 约束；时间和输入/输出大小采用宿主、Manifest 与调用方限制的交集。握手、健康检查、协议格式、身份或输出校验失败时终止进程。取消、超时或异常会清理进程，避免未读响应污染后续请求；正常 capability 业务错误不会自动退出进程。宿主不把调用失败的请求自动重放到恢复进程。

宿主清空子进程环境，仅保留 Windows 运行所需的系统变量，工作目录固定为 staging 副本，直接执行批准的绝对文件路径。Unix 使用独立进程组，停用、撤销、超时和析构会清理该组；主动脱离进程组的后代仍需操作系统策略约束。Windows 当前只清理直接子进程，后代进程应通过服务管理器或 Job Object 约束。

后台每 30 秒恢复仍启用的故障进程；服务启动时同样协调启用部署。停用 generation 保留运行时水位，旧部署不能在完成握手后重新覆盖新状态。登记、批准、撤销和部署 revision 的 catalog_* 审计与相应目录变更处于同一数据库事务；运行时结果单独记录，启动失败可能追加补偿停用 revision。

当前运行时不提供操作系统沙箱、独立低权限执行身份、网络或资源隔离，也没有凭据代理。适用范围仍是运营方信任的本地插件。可信宿主业务扩展通过 RuntimeHttpContext.plugin_capability_runtime 调用获授的能力时，仍需自行执行用户、租户、资源授权与业务审计；该端口不是公共调用 API。
