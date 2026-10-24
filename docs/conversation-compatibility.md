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

接入客户端时，应使用完整历史回放，避免依赖上游私有响应 ID。客户端档案的会话标识用于会话关联，不替代 Responses 的历史内容，详见 [客户端档案](channels.md#客户端档案与诊断)。

## 推理、工具与流式响应

同协议响应保留供应商签名。跨协议响应保留可表达的推理文本和工具 ID，不将来源签名用于目标供应商。转为 Messages 时，非原生推理以可回放的文本表达；OpenAI 响应按目标格式组织文本、推理和工具块。

Messages 和 Responses 的跨协议流会将交错工具事件整理为完整的调用生命周期，保留参数片段和结束事件。Responses 的工具参数完成事件省略工具名时，沿用已确认的调用身份；显式冲突的名称或参数会返回协议错误。

Chat 流式上游请求会补充 usage，以便统计和结算；下游是否返回 usage 仍遵循客户端选项。协议转换仅调整用量明细的表达方式，总输入和总输出量保持一致。上游未提供有效 usage 时使用估算用量。

## 兼容边界与排查

不透明的服务器会话状态、供应商内置执行能力和无法映射的媒体语义需要原生协议支持。跨协议无法表达时返回错误，不丢弃载荷或伪造签名、执行结果。

调用失败时，保留请求 ID、入口协议、目标模型、HTTP 状态码和脱敏错误内容。管理员可在「调试追踪」中对照下游请求与实际上游请求，检查历史完整性、工具关联及目标模型能力。渠道探测成功仅说明对应探测请求可用，实际客户端请求仍需验证。
