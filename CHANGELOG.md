## 0.3.0-beta.6 - 2026-10-30

本版本基于 `v0.3.0-beta.5`，完善分组授权、模型计费策略、调用观测和双前端控制台体验，用于新一轮社区测试与部署验收。

### 新增

- 支持用户创建个人 Key 时自主选择可用授权分组，并保留默认分组、固定分组和分组撤权边界。
- 增加分组模型访问策略、模型分时倍率和峰值时段配置，支持为特殊模型设置更高优先级的计费规则。
- 补充生图历史元数据和 URL 图片兼容处理，完善图片记录与 OpenAPI 契约。

### 改进

- web-next 控制台统一 HeroUI 组件与中英文文案，重排分组导航、账户认证、客户端档案、调用日志和生图广场页面。
- 调用日志支持失败记录过滤、时间范围、主体维度、RPM/TPM 汇总和更细的请求详情展示。
- 关于页面和在线更新面板优化双主题展示，更新说明支持展开与收起。

### 修复

- 修复不存在或已禁用模型仍进入调用日志和可通率统计的问题，并修正管理员模型目录可见性。
- 修正双前端一致性检查边界，页面和功能实现可按主题独立演进，共享接口契约仍保持同步。
- 修复 web-next 依赖审计中的 `decode-uri-component` 安全告警。

### 升级与兼容

- 本版本包含分组授权、模型策略和图片历史相关数据库及 OpenAPI 变更，升级前请备份数据库、配置和计费 WAL。
- 本版本为社区版 Beta 预发布，用于测试和部署验收。

完整变更：[v0.3.0-beta.5...v0.3.0-beta.6](https://github.com/ChuqiCloud/AnyFlows/compare/v0.3.0-beta.5...v0.3.0-beta.6)。

## 0.3.0-beta.5 - 2026-10-29

本版本基于 `v0.3.0-beta.4`，增加 OpenAI Images 生图与修图能力和独立的生图广场工作台，默认采用 web-next 新主题，并完善公开首页与控制台导航。

### 新增

- 增加 OpenAI Images 文生图与图片编辑接口，支持模型、尺寸、质量、数量、原图和蒙版等参数。
- 增加生图广场工作台，支持文生图、图片编辑、模型筛选、结果查看、下载和继续编辑。([#78](https://github.com/ChuqiCloud/AnyFlows/pull/78))

### 改进

- 新安装实例默认使用 web-next 主题；已有实例保留当前前端选择。([#78](https://github.com/ChuqiCloud/AnyFlows/pull/78))
- 完善公开首页在移动端的布局，并将 web-next 的首页和控制台导航按分组结构统一。([#76](https://github.com/ChuqiCloud/AnyFlows/pull/76))

### 修复

- 修复已存在用户的数据库在启动时读取首次安装状态失败的问题，避免启动过程被错误的安装状态查询中断。
- 修复 deepmerge 原型污染风险，覆盖 CommonJS 与 UMD 发行入口。

### 升级与兼容

- 新安装会在完成初始化前写入 web-next 默认主题；已完成初始化的实例不会被改写。
- OpenAI Images 功能需要配置可用的图片模型和对应渠道权限。

本版本为社区版 Beta 预发布。

完整变更：[v0.3.0-beta.4...v0.3.0-beta.5](https://github.com/ChuqiCloud/AnyFlows/compare/v0.3.0-beta.4...v0.3.0-beta.5)。

## 0.3.0-beta.4 - 2026-10-25

本版本基于 `v0.3.0-beta.3`，增加启动器托管、在线更新、Anthropic 原生上下文压缩和关于页面，完善四种协议的摘要回放、失败调用日志与部署说明。发布包增加 Linux、Windows、macOS 的 amd64 与 arm64 原生包，包含 Classic、Next 两套内置前端、启动器和 SHA-256 校验文件。

### 新增

- 发布包默认包含独立启动器，负责拉起主程序、传递停止信号和更新后自动重启；支持服务管理器或宝塔等面板启动，并保留直接运行主程序的方式。([#65](https://github.com/ChuqiCloud/AnyFlows/pull/65))
- 增加当前节点的版本检测与在线安装，按操作系统、架构和 libc 匹配发行包，校验版本、文件大小与 SHA-256 后替换主程序。二进制部署默认启用在线安装，可通过配置关闭。([#65](https://github.com/ChuqiCloud/AnyFlows/pull/65))
- 增加 Anthropic Messages 原生压缩与上下文编辑支持，保留压缩块、摘要、签名、扩展字段和流式事件，合并传递客户端与渠道的 beta 参数。([#66](https://github.com/ChuqiCloud/AnyFlows/pull/66))
- 增加独立的“关于 AnyFlows”页面，展示公司信息、合作伙伴、当前系统版本与更新入口，站点设置不再放置更新面板。([#67](https://github.com/ChuqiCloud/AnyFlows/pull/67))

### 优化

- 完善 OpenAI Chat、OpenAI Responses、Anthropic Messages 与 Gemini GenerateContent 之间的普通文本摘要回放，覆盖四种入口到四种上游的 16 个方向；补齐 Responses 原生压缩参数与状态转发。([#66](https://github.com/ChuqiCloud/AnyFlows/pull/66))
- 启动器继承服务管理器或面板注入的环境变量和日志管道，支持工作目录 `.env` 与显式环境文件；同名外部环境变量优先。([#65](https://github.com/ChuqiCloud/AnyFlows/pull/65))
- 失败调用日志增加详情抽屉，支持查看完整错误、请求人及关联调试追踪，改善长错误内容的展示。([#62](https://github.com/ChuqiCloud/AnyFlows/pull/62))
- 整理二进制安装、Docker Compose、多节点部署、配置和升级文档，补充合作伙伴与社区赞助信息。([#61](https://github.com/ChuqiCloud/AnyFlows/pull/61)、[#63](https://github.com/ChuqiCloud/AnyFlows/pull/63)、[#64](https://github.com/ChuqiCloud/AnyFlows/pull/64))

### 修复

- 修复仅返回原生压缩状态时被误判为无有效输出而返回 503，以及流式结束后的纯 SSE 注释心跳触发协议错误的问题。([#66](https://github.com/ChuqiCloud/AnyFlows/pull/66))
- 修复流式响应返回 EOF 时计费回调尚未完成的竞态，避免紧接着停机导致用量记录丢失；压缩迭代计入内部结算，普通消息迭代不重复累计。([#66](https://github.com/ChuqiCloud/AnyFlows/pull/66))
- 修复构建环境中的 Cargo 变量干扰运行配置，以及环境文件变量名的跨平台兼容问题。([#65](https://github.com/ChuqiCloud/AnyFlows/pull/65))
- Linux 发行构建固定使用 Debian Bookworm，避免构建节点升级导致发布包要求更高版本的 glibc。

### 安全更新

- JWT 签名与验签改用 AWS-LC 后端，修复原加密依赖的安全审计问题。([#63](https://github.com/ChuqiCloud/AnyFlows/pull/63))

### 升级与兼容

- 升级前备份数据库、配置和计费 WAL。在线安装只更新当前节点，多节点部署应逐台处理；程序回退不会降级数据库。
- 启动器托管模式在更新后自动重启主程序；裸启动模式安装完成后需通过原有服务管理器重启。启动器本身需停止服务后手动更新，容器部署通过更换镜像标签升级。
- Responses 不透明压缩状态和 Anthropic 原生压缩块仅适用于对应协议，且上游需支持相应能力。跨协议继续对话时，由客户端明确使用普通文本摘要。
- Linux glibc 包要求 glibc 2.36 或更新版本及 OpenSSL 3；macOS 包要求 macOS 15 或更新版本。未提供对应平台安装包时，请按部署文档手动升级。
- 本版本为社区版 Beta 预发布，用于测试和部署验收。

完整变更：[v0.3.0-beta.3...v0.3.0-beta.4](https://github.com/ChuqiCloud/AnyFlows/compare/v0.3.0-beta.3...v0.3.0-beta.4)。

## 0.3.0-beta.3 - 2026-10-24

本版本基于 `v0.3.0-beta.2`，增加渠道级路由绑定、凭据批量管理和构建版本查询，完善四类对话协议与 Agent 连续对话兼容，修复渠道竞速、流式等待及智能路由问题。

### 新增

- 智能路由支持绑定整个渠道或指定 Key。渠道绑定自动纳入可用凭据，保留原有指定 Key 绑定、优先级、权重和调度统计。([#53](https://github.com/ChuqiCloud/AnyFlows/pull/53))
- 渠道 Key 调度支持按优先级和权重、同优先级轮询、固定顺序失败回退三种策略，Classic 与 Next 两套前端均可配置。([#53](https://github.com/ChuqiCloud/AnyFlows/pull/53))
- 凭据管理支持逐行 Key、JSON、JSONL 和文件批量导入，以及批量删除、状态筛选、健康降权和冷却状态查看。([#53](https://github.com/ChuqiCloud/AnyFlows/pull/53))
- 增加公开构建版本接口 `GET /api/version` 和 `/version` 别名，返回版本号、源码提交与 UTC 构建时间，便于核对实际部署版本。([#56](https://github.com/ChuqiCloud/AnyFlows/pull/56))

### 优化

- 统一 OpenAI Chat、OpenAI Responses、Anthropic Messages 与 Gemini GenerateContent 的请求转换和路由校验，完善四种入口与四种上游之间的 16 个转换方向，以及流式、非流式响应。([#53](https://github.com/ChuqiCloud/AnyFlows/pull/53)、[#58](https://github.com/ChuqiCloud/AnyFlows/pull/58))
- 兼容常见 Agent 和 SDK 的可选空值、字符串数值、字段别名、旧式 functions、结构化工具结果和完整助手历史回放；同协议转发保留上游扩展字段。([#53](https://github.com/ChuqiCloud/AnyFlows/pull/53)、[#55](https://github.com/ChuqiCloud/AnyFlows/pull/55)、[#58](https://github.com/ChuqiCloud/AnyFlows/pull/58))
- 渠道竞速以首个有效模型输出确定获胜请求，失败的 Key 立即补位；完善流式完成、失败和客户端取消时的健康反馈，避免误判 Key 状态。([#53](https://github.com/ChuqiCloud/AnyFlows/pull/53)、[#54](https://github.com/ChuqiCloud/AnyFlows/pull/54))
- 流式请求在调度、重试和等待上游输出期间定期发送 SSE 保活，减少长时间等待被入口代理中断的问题。([#54](https://github.com/ChuqiCloud/AnyFlows/pull/54))

### 修复

- 修复编辑已有智能路由返回 500，以及不兼容的首个分组或智能规则阻断后续可用路由的问题。([#54](https://github.com/ChuqiCloud/AnyFlows/pull/54)、[#57](https://github.com/ChuqiCloud/AnyFlows/pull/57))
- 修复 OpenAI Chat 完整和流式响应丢失 `logprobs` 明细，兼容工具调用可选索引、空值占位及新增的 `service_tier` 名称。([#51](https://github.com/ChuqiCloud/AnyFlows/pull/51))
- 修复 Responses 明文推理摘要、工具调用 item 历史和结果续传被错误拒绝，以及可选加密输出偏好影响跨协议路由的问题。([#55](https://github.com/ChuqiCloud/AnyFlows/pull/55)、[#56](https://github.com/ChuqiCloud/AnyFlows/pull/56)、[#57](https://github.com/ChuqiCloud/AnyFlows/pull/57))
- 修复 Anthropic 思考内容、签名和缓存提示在请求重建时丢失，以及 Gemini 缺少工具调用 ID 时无法关联后续结果的问题。([#55](https://github.com/ChuqiCloud/AnyFlows/pull/55))
- 修复连续对话中结构化工具结果、推理签名和工具调用签名的预扣估算与回退计量，保留签名来源协议，避免错误地跨协议回放。([#57](https://github.com/ChuqiCloud/AnyFlows/pull/57))
- 修复交错工具事件、Responses 工具参数完成事件和无输出终态事件的处理，避免工具续答丢失或流式结束后继续读取已释放的上游响应。([#53](https://github.com/ChuqiCloud/AnyFlows/pull/53)、[#58](https://github.com/ChuqiCloud/AnyFlows/pull/58))

### 安全更新

- Tokio 更新至 1.53.3，修复依赖安全公告涉及的问题。([#58](https://github.com/ChuqiCloud/AnyFlows/pull/58))
- 更新两套前端的 Vite、Zod 4.x、DOMPurify、proxy-addr、Undici 和 source-map-js，修复首批依赖安全告警涉及的版本；保留工具链需要的 Zod 3.x。([#59](https://github.com/ChuqiCloud/AnyFlows/pull/59))

### 升级与兼容

- 新增渠道级路由绑定迁移。升级前备份数据库、配置和计费 WAL，首次启动时执行数据库迁移；回退数据库迁移前须先移除渠道级路由绑定。
- Responses 沿用完整历史回放与 `store:false` 的无状态转发方式；已有服务器会话 ID 不能直接跨渠道续接。
- 同协议转发保留供应商签名；跨协议转换保留可表达的文本、推理和工具关联，不伪造目标供应商签名或工具执行结果。
- 本版本为社区版 Beta 预发布，用于测试和部署验收。

完整变更：[v0.3.0-beta.2...v0.3.0-beta.3](https://github.com/ChuqiCloud/AnyFlows/compare/v0.3.0-beta.2...v0.3.0-beta.3)。

## 0.3.0-beta.2 - 2026-10-21

本版本基于 `v0.3.0-beta.1`，增加代理池、NVIDIA NIM 和 OpenCode 接入，完善渠道执行策略与分时计费。发布包包含 Linux amd64 二进制、Classic 与 Next 两套内置前端、systemd 安装脚本和 SHA-256 校验文件。

### 新增

- 自定义代理池：支持粘贴文本、TXT/CSV/JSON 文件和 GET 订阅，识别 HTTP/HTTPS、SOCKS5/SOCKS5H、认证信息、IPv6 与混合备注；支持自定义同步间隔、节点测活、健康筛选、凭据/渠道/全局绑定，以及批量删除和清理。([#43](https://github.com/ChuqiCloud/AnyFlows/pull/43))
- NVIDIA NIM 内置渠道：支持官方托管接口与自建 NIM 地址、API Key、模型发现、凭据探活、流式与非流式 Chat Completions。([#44](https://github.com/ChuqiCloud/AnyFlows/pull/44))
- OpenCode 专用渠道：支持 Inference v1、Zen v1、公开访问凭据、流式响应聚合及工具兼容中继。([#35](https://github.com/ChuqiCloud/AnyFlows/pull/35)、[#37](https://github.com/ChuqiCloud/AnyFlows/pull/37)、[#40](https://github.com/ChuqiCloud/AnyFlows/pull/40))
- 客户端档案管理：支持不可变修订、渠道绑定、历史修订回滚与引用保护，两套前端均提供管理入口。([#34](https://github.com/ChuqiCloud/AnyFlows/pull/34)、[#36](https://github.com/ChuqiCloud/AnyFlows/pull/36))
- 渠道上游执行策略：可配置并发竞速、重试次数和退避间隔；渠道设置优先，未配置时继承全局默认值。([#46](https://github.com/ChuqiCloud/AnyFlows/pull/46))
- 分组分时计费：支持每周固定日期、多个时间段、固定 UTC 偏移时区、跨午夜和全天规则；可对分组全部模型或指定模型设置加价、折扣及零倍率，并校验重叠时段。([#49](https://github.com/ChuqiCloud/AnyFlows/pull/49))

### 改进

- 完善 OpenAI Chat、OpenAI Responses、Anthropic Messages 和 Gemini GenerateContent 之间的双向对话转换，保留工具调用关联、推理内容和用量信息，兼容第三方重复角色、结束标记及占位字段。([#34](https://github.com/ChuqiCloud/AnyFlows/pull/34)、[#42](https://github.com/ChuqiCloud/AnyFlows/pull/42))
- 工具兼容中继增加可逆工具名映射、双轮调用、隐藏调用隔离与预算约束，保留 OpenCode 工具定义的版本和许可信息。([#38](https://github.com/ChuqiCloud/AnyFlows/pull/38)、[#39](https://github.com/ChuqiCloud/AnyFlows/pull/39)、[#40](https://github.com/ChuqiCloud/AnyFlows/pull/40))
- 统一渠道能力目录与适配器装配；渠道配置、探活、模型发现、调度及两套前端共享能力定义，减少新增渠道时的重复维护。([#49](https://github.com/ChuqiCloud/AnyFlows/pull/49))

### 修复

- 修复 DeepSeek 工具对话中 `reasoning_content` / `reasoning` 历史内容丢失，以及渠道执行策略表单显示翻译键的问题。([#49](https://github.com/ChuqiCloud/AnyFlows/pull/49))
- 修复 OpenCode 非调试模式下错误摘要及响应记录缺失，补充第三方 Chat 响应的缓存、推理和用量兼容。([#42](https://github.com/ChuqiCloud/AnyFlows/pull/42))
- 修复新增迁移、峰值计费字段和代理池后台任务引起的回归基线问题；更新 TLS 证书校验依赖与根证书数据，移除已失效的公告忽略规则。([#43](https://github.com/ChuqiCloud/AnyFlows/pull/43)、[#49](https://github.com/ChuqiCloud/AnyFlows/pull/49))

### CI 与仓库同步

- 同步公共 PR 标题、说明和评论，移除可见来源尾注，首次同步提交沿用 PR 标题，并修复合并后重复触发讨论同步的问题。([#41](https://github.com/ChuqiCloud/AnyFlows/pull/41)、[#45](https://github.com/ChuqiCloud/AnyFlows/pull/45)、[#47](https://github.com/ChuqiCloud/AnyFlows/pull/47)、[#48](https://github.com/ChuqiCloud/AnyFlows/pull/48))
- 修复 GitHub Action 引用与 Playwright 安装兼容，精简历史专项自动检查，并完善数据库迁移检查。([#41](https://github.com/ChuqiCloud/AnyFlows/pull/41)、[#45](https://github.com/ChuqiCloud/AnyFlows/pull/45)、[#49](https://github.com/ChuqiCloud/AnyFlows/pull/49))

### 升级与兼容

- 新增客户端档案、OpenCode 公开档案、代理池及分组分时计费迁移。升级前备份数据库、配置和计费 WAL，首次启动时执行数据库迁移。
- 旧峰值计费配置继续兼容；显式保存空的分时规则列表会关闭旧峰值规则。
- 并发竞速会同时发起多个上游请求，可能增加上游用量；默认值与渠道覆盖值应按实际额度配置。
- OpenCode 免费模型仍受上游访问策略约束。NVIDIA 渠道的工具、多模态和推理能力以对应上游模型为准。
- 本版本为社区版 Beta 预发布，用于测试和部署验收。

完整变更：[v0.3.0-beta.1...v0.3.0-beta.2](https://github.com/ChuqiCloud/AnyFlows/compare/v0.3.0-beta.1...v0.3.0-beta.2)。

## 0.3.0-beta.1 - 2026-10-18

这是 AnyFlows 社区版的首个 Beta 版本，可独立部署。

### 发布内容

- 提供 Classic 与 Next 两套内置管理前端，以及 OpenAI 兼容的 Chat、Responses 和模型列表接口。
- 提供渠道、模型、密钥池、账号池、用量、额度、调用日志和运行状态管理。
- 支持 SQLite 单机部署，并提供 PostgreSQL、MySQL、Redis 和多节点部署配置。
- 发布包包含 Linux amd64 二进制、内嵌前端、systemd 安装脚本和 SHA-256 校验文件。

### 兼容说明

- 本版本沿用 `0.2.0` 版本线的数据库迁移历史，升级前请备份数据库、配置和计费 WAL。
- Beta 版本用于部署验收，生产环境请先完成备份和回滚演练。
