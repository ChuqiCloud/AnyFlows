# 渠道适配器与能力目录

渠道的 `type` 选择通信适配器，`protocol` 选择原生上游协议，`provider` 记录厂商品牌。普通 OpenAI 兼容服务复用 OpenAI 适配器，通过服务地址和品牌区分；只有协议、认证或请求语义确有差异时才增加适配器。

## 能力声明

`crates/af-domain/src/channel_capability.rs` 集中声明渠道支持的生产协议、探活、模型发现以及凭据种类。管理写入、数据库校验、调度筛选和服务装配都读取同一份声明。

凭据保留两层契约：`supports_credential_kind` 兼容历史记录，`creatable_credential_kinds` 限定管理表单的新建选项。Codex OAuth、OpenCode 公开凭据和客户端档案仍有各自的联合校验，通用能力声明不能替代这些认证边界。

当前 Bedrock、Vertex 和 Custom 虽有底层配置类型，但尚未接入管理与生产装配，因此不会作为可用渠道出现在新建表单中。xAI 视频使用独立的异步任务接口，不提供通用探活或模型枚举。

## 适配器装配

`af-adapter::standard_adaptor_settings` 负责常规渠道的强类型配置构造，供生产调度、探活和模型发现共用。调用方提供渠道类型、协议、模型清单和可选的 OpenCode 固定入口，再由 `get_adaptor` 创建实例。

NVIDIA 保留专用适配器；OpenCode 必须提供明确的固定入口。Bedrock 的区域、Vertex 的项目与位置、Custom 的端点与认证配置继续使用各自的专用构造路径，不将这些参数折叠为任意 JSON 配置。

## 前端生成与一致性

在构建环境更新领域能力后执行：

```sh
cargo xtask channel-capabilities export
```

该命令同时生成 Classic 和 Next 的 `src/features/channels/channel-capabilities.gen.ts`。两套前端共用相同的表单辅助逻辑，从生成目录读取渠道类型、协议、凭据新建范围、探活和模型发现能力。Codex OAuth 是 OpenAI Responses 的表单预设，不增加通信适配器类型。

现有 CI 使用 `cargo xtask channel-capabilities check` 检查生成目录，并通过前端一致性门禁防止两套业务逻辑漂移。生成文件应随能力声明一起提交。

## 新增渠道

1. 先判断能否复用已有协议和适配器；仅增加品牌时维护品牌目录即可。
2. 确需新增适配器时，实现协议与认证行为，增加强类型配置，并接入适配器工厂。
3. 在领域能力目录声明已完成的运行阶段，常规装配接入 `standard_adaptor_settings`；有专用任务接口或额外参数时保留显式路径。
4. 导出两套前端能力目录，按需要增加展示名称、品牌与专有配置表单。
5. 补充协议、认证边界和请求响应回归，通过企业 Gitea CI 验证。新增渠道不再复制管理、调度、探活和前端的通用能力矩阵，但专有协议行为仍需单独实现。
