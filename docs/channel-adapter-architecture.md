# 渠道适配器与能力目录

渠道的 `type` 指定通信适配器，`protocol` 指定上游原生协议，`provider` 记录厂商品牌。OpenAI 兼容服务通常复用 OpenAI 适配器，通过服务地址和品牌区分。协议、认证或请求语义有差异时再增加适配器。

## 能力声明

[`channel_capability.rs`](../crates/af-domain/src/channel_capability.rs) 声明渠道支持的协议、生产调度、探活、模型发现和凭据种类，供管理写入、数据库校验、调度筛选及服务装配共用。

凭据分为两层契约：`supports_credential_kind` 定义历史记录的兼容范围，`creatable_credential_kinds` 定义管理表单的新建选项。Codex OAuth、OpenCode 公开凭据和客户端档案还需通过各自的联合校验。

Bedrock、Vertex 和 Custom 已有底层配置类型，尚未接入管理与生产装配，不在新建表单中提供。xAI 视频使用独立的异步任务接口，不提供通用探活或模型发现。

## 适配器装配

`af-adapter::standard_adaptor_settings` 构造常规渠道的强类型配置，供生产调度、探活和模型发现共用。调用方提供渠道类型、协议、模型清单及可选的 OpenCode 固定入口，由 `get_adaptor` 创建实例。

NVIDIA 使用专用适配器；OpenCode 必须指定固定入口。Bedrock 区域、Vertex 项目与位置、Custom 端点与认证配置由专用配置类型表达。

## 前端生成与一致性

修改领域能力后导出前端目录：

```sh
cargo xtask channel-capabilities export
```

该命令生成两套前端的 `src/features/channels/channel-capabilities.gen.ts`。表单共用辅助逻辑，从生成目录读取渠道类型、协议、凭据新建范围、探活和模型发现能力。Codex OAuth 是 OpenAI Responses 的表单预设，不是独立适配器类型。

用 `cargo xtask channel-capabilities check` 检查生成目录，用 `node tools/ci/check-frontend-parity.mjs` 检查两套前端的共享逻辑。生成文件随能力声明一起提交。

## 新增渠道

1. 判断能否复用已有协议和适配器；仅增加品牌时维护品牌目录。
2. 确需新增适配器时，实现协议与认证行为，增加强类型配置，并接入适配器工厂。
3. 在领域能力目录声明已接入的运行阶段。常规装配接入 `standard_adaptor_settings`；专用任务接口和额外参数使用专用构造路径。
4. 导出两套前端能力目录，按需要增加展示名称、品牌与专有配置表单。
5. 补充协议、认证和请求响应回归，并通过对应的远程检查。通用能力矩阵集中维护，供应商特有行为在适配器中实现。
