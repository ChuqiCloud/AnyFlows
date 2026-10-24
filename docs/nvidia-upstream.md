# NVIDIA 上游接入

在管理控制台创建渠道，通信适配器选择 **NVIDIA**，原生协议为 **OpenAI Chat Completions**。

1. 在 [NVIDIA API Catalog](https://build.nvidia.com/) 获取 API Key，在渠道凭据中选择 API Key 并保存。
2. 基础地址留空时使用官方接口 `https://integrate.api.nvidia.com/v1/chat/completions`。自建 NIM 或网关可填写自己的基础地址，支持带或不带 `/v1`，并保留路径前缀。
3. 使用渠道的模型发现功能读取 `/v1/models`，或手动填写上游支持的完整模型 ID，例如 `meta/llama-3.2-11b-vision-instruct`。模型可用性以当前目录和账号权限为准；保留模型 ID 的命名空间，需要对外使用其他名字时配置模型映射。
4. 将模型绑定到分组并发布，保存凭据后执行渠道测活，再通过 [API 目录](api-explorer.md) 验证实际请求。

适配器使用 `Authorization: Bearer <API Key>`，支持普通及流式对话，以及 [对话协议转换](conversation-compatibility.md)。输出上限转换为 NIM 的 `max_tokens` 参数；工具调用、推理、多模态和采样参数由所选模型决定。目录中可能包含非对话模型，此渠道仅适用于支持 Chat Completions 的模型。

此渠道不提供原生 Responses、嵌入、图像生成或语音端点。模型发现成功后，仍需验证 API Key 权限、配额和实际推理。自建 NIM 需提供兼容的 Chat Completions、模型目录和 Bearer 认证接口。
