# NVIDIA 上游接入

在管理控制台创建渠道，通信适配器选择 **NVIDIA**，原生协议为 **OpenAI Chat Completions**。Classic 和 Next 控制台均支持此配置。

1. 在 [NVIDIA API Catalog](https://build.nvidia.com/) 获取 API Key，在渠道凭据中选择 API Key 并保存。
2. 基础地址留空即可使用官方托管接口 `https://integrate.api.nvidia.com/v1`。自建 NIM 或网关可填写自己的基础地址，支持带或不带 `/v1`，并保留网关路径前缀。
3. 使用渠道的模型发现功能读取 `/v1/models`，或手动填写上游支持的完整模型 ID，例如 `meta/llama-3.2-11b-vision-instruct`。模型可用性以当前目录和账号权限为准；保留模型 ID 的命名空间，需要对外使用其他名字时配置模型映射。
4. 将模型绑定到分组并发布，保存凭据后执行渠道测活，再使用在线调试发送请求。

适配器使用 `Authorization: Bearer <API Key>`，支持普通及流式对话，复用现有工具调用与响应转换。输出预算会转换为 NIM 的 `max_tokens` 参数；工具调用、推理、多模态和采样参数的可用性取决于选定模型。模型目录中可能包含非对话模型，只有支持 Chat Completions 的模型适用于此渠道。

此渠道不提供原生 Responses、嵌入、图像或语音端点。模型发现成功只表示能读取目录，实际推理仍受 API Key 权限、模型能力、配额和上游服务状态限制。自建 NIM 也需提供兼容的 Chat Completions、模型目录和 Bearer 认证接口。
