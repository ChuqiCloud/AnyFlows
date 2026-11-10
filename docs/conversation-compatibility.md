# 对话协议兼容

AnyFlows 支持 OpenAI Chat、OpenAI Responses、Anthropic Messages 和 Gemini GenerateContent。四种入口均可转发至四种协议的上游，覆盖 16 个方向，支持普通 JSON 响应和流式响应。具体功能取决于目标协议及模型能力。

| 协议 | 请求入口 |
| --- | --- |
| OpenAI Chat | `POST /v1/chat/completions` |
| OpenAI Responses | `POST /v1/responses` |
| Anthropic Messages | `POST /v1/messages` |
| Gemini | `POST /v1beta/models/{model}:generateContent`、`POST /v1beta/models/{model}:streamGenerateContent` |

## 请求与对话历史

网关兼容常见 SDK 的字段差异，包括可选字段的 `null`、字符串数值、字段别名、旧式 `functions`、结构化工具结果和完整的 assistant 输出回放。归一化仅作用于协议字段，保留工具参数、工具结果和用户 JSON Schema 中的业务值。重复键、正文大小和结构复杂度检查仍然生效。

同协议转发保留原生扩展，并应用渠道配置的模型映射、输出上限和采样参数。跨协议转发按目标格式转换消息、工具、结构化输出及可映射的采样参数。空停止序列按未提供处理。

工具历史按调用 ID 关联，并将结果放到对应调用之后。缺失 ID 但关联唯一时可恢复；无法确定关联的历史保留为带状态说明的普通文本，不生成虚假的工具结果。客户端应回传完整的工具调用和结果，尽量保留原始 ID。

Gemini 的 `VALIDATED` 工具模式在原生转发时保留，跨协议时映射为指定工具范围内的自动调用。尚未建模的内联 PDF、视频等内容可通过 Gemini 原生渠道转发；跨协议仍受目标媒体能力限制。

## Responses 会话

Responses 上游请求使用 `store:false`，对话历史由客户端完整回传。目前不支持通过 `previous_response_id` 或 `conversation` 引用上游保存的会话；此类请求会被拒绝，即使上下游同为 Responses。

接入客户端时，应使用完整历史回放，避免依赖仅由原上游识别的响应 ID。客户端档案的会话标识用于会话关联，不替代 Responses 的历史内容，详见 [客户端档案](channels.md#客户端档案与诊断)。

## 上下文压缩

客户端生成的普通文本摘要可作为 assistant 历史回放，在四种协议的 16 个转换方向中保留摘要内容、角色及后续消息。Chat 和 Gemini 可通过普通对话请求生成摘要，再由客户端回传摘要及需要保留的历史；网关按普通消息转换这类内容。

Responses 支持原生 `context_management` 压缩控制、`compaction_trigger` 扩展和不透明的 `compaction` item。同协议转发保留这些控制与状态。`compact_threshold` 可省略或设为 `null`，由上游决定阈值；显式整数按原值转发。独立的 `POST /v1/responses/compact` 使用专用压缩路径，需要上游支持该端点。

Anthropic Messages 支持 `compaction` 按需压缩和 `context_management` 上下文编辑，适配器按请求补充压缩所需的 beta，并合并客户端已有的 beta。原生压缩块及其摘要内容、签名、扩展字段、流式块生命周期和压缩结束原因均按 Messages 协议保留，客户端应完整回传压缩块以继续对话。

原生压缩控制与状态仅适用于对应协议。跨协议请求携带 Responses 不透明压缩状态或 Anthropic 原生压缩块时会返回兼容性错误。需要跨协议继续对话时，客户端应明确使用普通文本摘要，并保留必要的用户约束、工具结果和后续消息。

Anthropic 原生同协议响应保留顶层 `usage` 和 `usage.iterations` 元数据。内部结算额外累计压缩迭代的用量，已包含于顶层用量的普通消息迭代不重复累计。网关工具续答合成的响应使用累计用量，不再携带已经计入的迭代元数据。

## 推理、工具与流式响应

同协议响应保留供应商签名。跨协议响应保留可表达的推理文本和工具 ID，不将来源签名用于目标供应商。转为 Messages 时，非原生推理以可回放的文本表达；OpenAI 响应按目标格式组织文本、推理和工具块。

Messages 和 Responses 的跨协议流会将交错工具事件整理为完整的调用生命周期，保留参数片段和结束事件。Responses 的工具参数完成事件省略工具名时，沿用已确认的调用身份；显式冲突的名称或参数会返回协议错误。

终态后的 SSE 注释心跳不参与对话事件；后续数据事件按各协议的终态规则校验。

Chat 流式上游请求会补充 usage，以便统计和结算；下游是否返回 usage 仍遵循客户端选项。协议转换按目标格式表达归一后的计费用量；Anthropic 原生压缩的公开用量与内部累计计费遵循上述迭代规则。上游未提供有效 usage 时使用估算用量。

## 兼容边界与排查

不透明的服务器会话状态、供应商内置执行能力和无法映射的媒体语义需要原生协议支持。跨协议无法表达时返回错误，不丢弃载荷或伪造签名、执行结果。

调用失败时，保留请求 ID、入口协议、目标模型、HTTP 状态码和脱敏错误内容。管理员可在「调试追踪」中对照下游请求与实际上游请求，检查历史完整性、工具关联及目标模型能力。渠道探测成功仅说明对应探测请求可用，实际客户端请求仍需验证。
