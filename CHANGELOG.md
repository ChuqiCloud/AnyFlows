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
- 本版本为社区版 Beta 预发布，用于测试和部署验收；企业扩展不包含在发布包中。

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
- 本版本为社区版 Beta 预发布，用于测试和部署验收；企业扩展不包含在发布包中。

完整变更：[v0.3.0-beta.1...v0.3.0-beta.2](https://github.com/ChuqiCloud/AnyFlows/compare/v0.3.0-beta.1...v0.3.0-beta.2)。

## 0.3.0-beta.1 - 2026-10-18

这是 AnyFlows 社区开放版的首个 Beta 版本。公共核心已与企业扩展拆分，社区版可以独立部署；企业空间、企业 SSO 等私有能力不包含在本版本中。

### 发布内容

- 提供 Classic 与 Next 两套内置管理前端，以及 OpenAI 兼容的 Chat、Responses 和模型列表接口。
- 提供渠道、模型、密钥池、账号池、用量、额度、调用日志和运行状态管理。
- 支持 SQLite 单机部署，并提供 PostgreSQL、MySQL、Redis 和多节点部署配置。
- 发布包包含 Linux amd64 二进制、内嵌前端、systemd 安装脚本和 SHA-256 校验文件。

### 兼容说明

- 本版本沿用 `0.2.0` 版本线的数据库迁移历史，升级前请备份数据库、配置和计费 WAL。
- 从企业版切换到社区版时，社区迁移器会保留企业数据并继续处理已知迁移；企业功能不会转换为社区功能。
- Beta 版本用于部署验收，生产环境请先完成备份和回滚演练。

## 0.2.0-beta.14 - 2026-09-25

这是基于 v0.2.0-beta.13 的测试版本，完善模型厂商目录、实名认证配置和 OpenAI 账号池。

### 新增

- 模型厂商目录独立成管理员页面，支持分页、搜索、自定义厂商和 Logo 配置。
- 实名认证方式可在后台启停；人工认证增加境内外证件类型与地区选择。
- OpenAI 渠道内置 Codex OAuth，并支持批量导入、导出 sub2api/Codex 令牌文件。
- 两套前端的账号池支持按需查看 Codex OAuth 用量窗口、重置时间及上游返回的余额。

### 改进

- 连续会话优先使用健康且有并发余量的同一账号，以提高上游缓存命中机会；拥塞或健康降级时回退到负载调度。

## 0.2.0-beta.13 - 2026-09-25

这是基于 v0.2.0-beta.12 的测试版本，包含新前端接口对接修复和支付宝实名认证后台配置。

### 修复

- 修复 API 调试页独立部署基址、API Key 请求头和特殊请求的会话失效处理。
- 修复企业认证材料上传的 multipart 请求序列化。
- 修复同源部署实时 WebSocket 地址生成。

## 0.2.0-beta.12 - 2026-09-24

这是基于 v0.2.0-beta.11 的测试版本，增加模型厂商目录管理和自定义提供商支持。

### 新增

- 增加模型厂商目录管理，支持配置显示名称、Logo、别名、排序和启用状态。
- 支持新增自定义模型厂商，并在经典主题和 Next 主题的模型、渠道表单中统一选择。
- 增加公开模型厂商目录接口和管理员目录维护接口。

### 改进

- 两套内置前端统一使用模型厂商目录，保留各自主题的交互和视觉风格。

## 0.2.0-beta.11 - 2026-09-23

这是基于 v0.2.0-beta.10 的测试版本，完善个人与企业认证、企业空间授权和经典主题使用体验。

### 新增

- 增加个人与企业认证记录，支持认证材料上传、预览、审核留档和状态查询。
- 支持认证用户申请企业空间，并提供企业管理员可配置的 SSO、域名等平台授权项。
- 双内置前端同步企业认证、材料预览、认证记录和企业授权管理页面。

### 改进

- 优化经典主题侧边栏分组、导航层级和常用入口文案，统一模型体验与健康检查等名称。
- 同步修正两套前端企业 SSO 回调接口的中文文案，提升接口文档可读性。

# 变更日志

## 0.2.0-beta.10 - 2026-09-23

这是基于 v0.2.0-beta.9 的测试版本，更新调用日志展示、OpenAI Chat 历史消息兼容和经典前端登录流程。

### 改进

- 两套内置前端将成功和失败调用合并到同一日志列表，覆盖个人、管理员与企业调用记录，沿用各角色的信息可见范围。
- 经典前端通过登录方式选项切换账号与企业 SSO 表单，合并 Passkey 入口，减少页面高度。

### 修复

- 修复 Next 前端 API 页面访问和顶栏 API 导航。
- 兼容 OpenAI Chat 助手历史消息中的 `reasoning_content` 与 `reasoning` 字符串或空值，避免客户端回传这些字段时续聊请求被拒绝。

## 0.2.0-beta.9 - 2026-09-23

这是基于 v0.2.0-beta.8 的测试版本，合入 API 目录、双内置前端和企业控制台后续改动。

### 新增

- 新增公开 API 目录和在线调试页面，按访问主体投影接口，支持搜索、标签筛选、分页及 OpenAPI 契约查看。
- 支持 AnyFlows Classic、AnyFlows Next 两套内置前端，以及外部模板的预览、分页目录和动态切换。
- 扩展模型厂商目录、Logo、规范化显示名称和自定义提供商配置。
- 增加按权限隔离的调用失败记录，普通用户、企业成员和平台管理员分别查看其授权范围内的信息。

### 修复

- 修复 API Key 模型发现入口和 OpenAI Chat 连续对话中的兼容性问题。

## 0.2.0-beta.8 - 2026-09-12

这是基于 v0.2.0-beta.7 的测试修复版，完善运行时前端模板、环境变量加载和企业控制台体验。

### 新增

- 支持从运行目录动态加载前端模板，并补充前端模板开发规范与 OpenAPI 地址说明。

### 修复

- 修复手工启动时环境变量及 `.env` 文件未正确加载的问题。
- 修复企业目录无法自动加载、管理员操作被无理由校验阻断，以及未配置支付服务时控制台出现 503 噪音的问题。

## 0.2.0-beta.7 - 2026-09-12

这是基于 v0.2.0-beta.6 的测试修复版，修正发布元数据版本与版本标签不一致导致的 CNB 发布失败。

### 修复

- 同步 workspace、内部 crate 依赖、Cargo.lock 与 OpenAPI 版本到 0.2.0-beta.7，确保版本校验和发布归档一致。

## 0.2.0-beta.6 - 2026-09-11

这是基于 v0.2.0-beta.5 的测试修复版，补齐企业初始化与资料编辑流程，并同步 OpenAPI 与前端生成客户端。

### 新增

- 企业 Owner 初始化向导改为服务端事实驱动，支持编辑企业资料、发送首批邀请并查看一次性邀请令牌。
- 新增企业资料更新接口，支持按版本 CAS 修改企业名称和 slug，并保持 slug 唯一性与审计记录。
- 企业认证入口明确为可选资质审核流程，不再与基础企业功能或初始化步骤混淆。

### 修复

- 同步企业概览和资料更新接口的 OpenAPI schema、生成客户端及固定契约测试，修复流水线 OpenAPI 检查失败。
- 补齐企业控制台相关中英文文案、权限入口和回归覆盖。

## 0.2.0-beta.5 - 2026-09-11

这是基于 v0.2.0-beta.4 的测试修复版，重点修复 DeepSeek 官方 OpenAI 与 Anthropic 协议兼容问题。

### 修复

- 兼容 DeepSeek OpenAI Responses 中的 `prompt_cache_hit_tokens` 和 `prompt_cache_miss_tokens` 用量字段，避免成功响应被误判为协议错误。
- 修复 DeepSeek Anthropic 渠道模型发现，改用官方 `/v1/models` 接口获取模型列表。
- 补充 OpenAI/Anthropic 非流式与流式 DeepSeek 响应回归测试，覆盖缓存用量映射和模型发现路径。

## 0.2.0-beta.4 - 2026-09-10

### 修复

- 修复 MySQL 下创建或更新渠道时，容量锁插入生成无效的 `ON DUPLICATE KEY IGNORE`，导致接口返回 500 和数据库冒烟失败的问题。改为主键自赋值的兼容写法，保留已有锁记录及事务串行化行为。
- 为三方言渠道回归补充创建、详情、列表、凭据和非法配置校验的阶段信息，便于定位失败步骤。
- 纳入 OpenAI Responses 上游工具与请求参数回显的兼容解析修复。

## 0.2.0-beta.3 - 2026-09-09

### 修复

- 修复 PostgreSQL 下新建渠道后，渠道列表和详情返回 500、凭据页面无法加载渠道信息的问题。账号池与 Responses WebSocket 状态的 SQL 投影显式使用 BIGINT，与 Rust 的 i64 解码保持一致。
- 将新建渠道后的列表、详情、空凭据与已添加凭据读取纳入数据库迁移 CI，覆盖默认配置、启用账号池/WebSocket、模型与分组关联及非法配置校验，避免仅靠 SQLite 测试漏掉 PostgreSQL 类型错误。

## 0.2.0-beta.2 - 2026-09-09

这是基于 v0.2.0-beta.1 的测试修复版，重点修复 NewAPI 上游兼容和渠道探活。

### 修复

- 兼容 NewAPI Chat 响应中的 request_id、reasoning_content、reasoning 扩展字段，避免 HTTP 200 被误判为协议错误。
- 兼容流式 Chat SSE chunk 的 request_id 字段。
- 修复渠道探活未应用普通模型映射的问题，使探活模型与实际转发一致。

## 0.2.0-beta.1 - 2026-09-08

这是用于部署验收的测试版本。生产升级前必须备份数据库、计费 WAL 和部署配置，并预留迁移失败后的快照恢复窗口。

### 新增

- 企业空间能力：成员、团队、企业钱包、分层预算、企业 API Key、调用日志、管理审计、SSO、SCIM、多级部门、合同价、授信账期和审批流。
- 订阅商业闭环：可售计划与价格快照、幂等订单、支付确认与订阅绑定、购买界面、退款请求、Stripe 原路退款、Epay 管理员审批后的人工退款和退款对账。
- 多协议与账号能力：OpenAI、Anthropic、Gemini、Responses、图像、音频、Embedding、Rerank，以及 OAuth 账号连接、刷新、客户端仿真和 Spark 影子账号等受控能力。
- 可选 ClickHouse 分析读取与异步事实投递；可选 Redis 请求限流键空间配置；Docker Compose 和 systemd 的 Linux amd64 部署、升级、回滚与健康检查。

### 改进

- 计费、额度、订阅周期、渠道选路、粘性路由、失败重试和多租户权限边界持续收紧，结果未知、并发冲突和失败关闭路径均保留可恢复事实。
- 管理台补齐企业工作空间、通知历史、认证安全、订阅购买、退款审批和分析看板；公开文档同步部署、认证、配置、迁移、备份与回滚边界。
- 工程质量门禁补齐上游错误分类、看板快照、计费生命周期、调度策略、遥测指标和 Redis 限流边界回归，流水线按变更路径选择性执行。

### 配置与升级提示

- `.env.example` 已同步 Redis、ClickHouse 和静态 OpenAI 上游兼容配置。ClickHouse 配置默认关闭，分析读取账号与异步写入账号必须分离；静态 OpenAI 上游仅保留兼容解析，不作为 M1 生产回退目标。
- 本版本包含 `v0.1.0` 之后的数据库迁移；应用启动时会自动执行待处理迁移，迁移不会自动回滚。
- 升级后应检查 `/healthz`、`/readyz`、迁移结果、管理鉴权和一条低额度网关请求，再决定是否扩大测试流量。
