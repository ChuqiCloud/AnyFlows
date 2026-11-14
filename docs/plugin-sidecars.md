# AnyFlows 插件 Sidecar 协议

本文记录 `af-plugin-runtime` 当前进程外连接器基础。协议采用版本化 JSON Lines：标准输入每行一个 UTF-8 JSON `PluginHostRequest`，标准输出每行一个 JSON `PluginHostResponse`；标准错误不进入协议。每帧大小、启动时间、调用期限与结果大小由宿主限制。

## 制品信任

插件包必须由管理员预先放置于本地受控目录，不由服务在线下载。目录树中不允许符号链接；总文件大小上限为 512 MiB，文件数上限为 100,000，文件相对路径须为 UTF-8。包摘要按相对路径排序，按 `ANYFLOWS_PLUGIN_PACKAGE_V1\0`、每个 UTF-8 相对路径长度（little-endian u64）、路径字节、文件长度（little-endian u64）、文件字节的顺序计算 SHA-256。可执行文件必须在包根目录内。

Ed25519 签名对以下字节签署：`ANYFLOWS_PLUGIN_ARTIFACT_V1\0`、plugin ID、换行、语义化版本、换行、64 位小写十六进制包摘要、换行、manifest 的紧凑 UTF-8 JSON 字节。签名前按对象键的 UTF-8 字节字典序递归排序，数组顺序保持不变；不添加空格，并使用 serde_json 的字符串转义方式。`signer_key_id` 必须命中宿主配置的可信公钥；签名本身不授予能力，部署记录还必须明确批准该插件版本、摘要和只读能力集合。manifest 在签名中绑定，启动握手仍会核对运行进程返回的完整 manifest。

## 帧与调用

每个请求包含协议版本、插件 ID/版本、制品摘要、单调递增部署 generation、request ID、Unix 毫秒 deadline 与操作。启动时宿主先请求 manifest，再请求无副作用 health；只有身份、版本、摘要和 manifest 完全一致且 health 为 healthy 才视为可用。

`invoke_capability` 只接受 `access=read` 且 `sideEffect=none`、并由宿主单独批准的能力，调用前后分别校验该能力的 JSON Schema。调用帧只含连接标识、公开配置、能力 ID/版本和输入 JSON；宿主还会递归拒绝键名包含密码、凭据、密钥、Cookie、Authorization 或访问令牌等标记的字段。该检查是防误传措施，不是秘密内容识别器；宿主调用方仍须确保公开配置和输入中不含秘密。当前协议没有凭据代理、钱包权限或数据库句柄。写能力由宿主业务工作流处理，不通过通用 sidecar 调用。

宿主清空子进程环境，除 Windows 运行所需的系统变量外不继承变量；工作目录固定在已校验的包根，直接执行批准的绝对文件路径，不经 shell。每个进程的请求串行执行。启动握手、健康检查失败，或协议格式、身份、输出 Schema 校验失败时会终止进程；正常 capability 业务错误不会自动退出进程。stderr 丢弃，协议错误不回显上游数据。

## 当前限制

这里实现的是签名制品校验与进程生命周期/消息边界，不是 OS 安全沙箱。子进程仍使用 AnyFlows 服务账号的操作系统权限；当前没有 seccomp、namespace、低权限账户或 cgroup 配置，也没有凭据/出站网络 broker。因此当前只适合无需私密凭据和外部网络访问的通用只读 capability。部署目录必须由管理员维护，不能允许不可信写入；宿主在启动前会再次计算摘要，但若进程或其他主体仍可改写制品目录，文件校验与执行之间仍存在竞态。部署前须使用受限服务账号和操作系统策略隔离 sidecar；在宿主 egress broker 与 OS 隔离策略接入并验证前，不应启用需访问外部平台/网络的第三方 sidecar。

## 管理与生命周期

默认不启用外部制品。管理员需在服务配置中同时设置受控制品目录和至少一个发布者公钥：

```toml
[server]
plugin_artifact_directory = "data/plugins"

[[server.plugin_trusted_signers]]
key_id = "publisher-example"
public_key_base64 = "<发布者的 Ed25519 公钥，Base64 编码的 32 字节>"
```

目录路径必须是运行目录下的相对路径；不配置目录和公钥时，服务不会接受制品登记或启用侧车。管理员把制品放入目录后，可通过管理 API 登记路径、清单、摘要、签名者 ID 和签名。请求路径必须是 POSIX 相对路径；服务端规范化路径，拒绝越过根目录或包目录的路径。数据库只保存规范化相对定位信息，不保存绝对服务器路径。

管理 API 提供制品列表/登记/审批/撤销，以及部署列表、历史、部署、停用和回滚。登记与审批分离；撤销是只追加的永久事实，同时会以新 generation 停用正在使用该制品的部署。部署以 compare-and-swap generation 写入不可变历史。所有状态变更记录操作者、结果和同一请求 ID。启用或回滚会在持久化后验签并启动新进程；启动失败时宿主保持进程停止并尝试写入补偿停用记录。服务启动时从数据库恢复启用的本地 sidecar，关闭时在关闭数据库前停止所有子进程。

可信宿主业务扩展可通过 `RuntimeHttpContext.plugin_capability_runtime` 调用获批的只读 capability。该端口不提供公共调用 API，也不会替扩展做业务授权；调用方仍需校验自己的用户、租户与资源权限。

生命周期审批、能力 grants、generation 与操作审计由宿主控制，不能由插件 manifest 自我授权。这里支持管理员预置的本地进程包，不提供在线下载、自动更新、WASM 或任意代码插件市场。当前代码也没有 OS 沙箱、受控 egress broker、秘密代理或进程崩溃后的自动重启；不应将启用状态解释为低权限或网络隔离保证。
