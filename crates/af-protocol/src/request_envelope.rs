use std::fmt;

use af_domain::Protocol;
use bytes::Bytes;

use crate::CanonicalRequest;

/// Canonical 请求及其可选的已验证源协议正文。
///
/// 协议解析器完成 JSON、结构预算和语义校验后才能附加源正文。调用方只能读取
/// Canonical 视图，不能在保留源正文时修改请求语义。
#[derive(Clone, PartialEq)]
pub struct CanonicalRequestEnvelope {
    canonical: CanonicalRequest,
    source: Option<ValidatedSourceRequest>,
    requested_model: Option<String>,
}

impl CanonicalRequestEnvelope {
    /// 包装程序内部构造的 Canonical 请求；这类请求不具备直通资格。
    #[must_use]
    pub const fn from_canonical(canonical: CanonicalRequest) -> Self {
        Self {
            canonical,
            source: None,
            requested_model: None,
        }
    }

    /// 保存模型别名归一前的客户端模型，并永久丢弃原始正文直通资格。
    pub(crate) fn from_normalized_model(
        canonical: CanonicalRequest,
        requested_model: String,
    ) -> Self {
        Self {
            canonical,
            source: None,
            requested_model: Some(requested_model),
        }
    }

    /// 由协议解析器附加已完成信任边界校验的原始正文。
    pub(crate) const fn from_validated_source(
        canonical: CanonicalRequest,
        protocol: Protocol,
        body: Bytes,
    ) -> Self {
        Self {
            canonical,
            source: Some(ValidatedSourceRequest { protocol, body }),
            requested_model: None,
        }
    }

    /// 只读访问 Canonical 语义；正文改写必须先消费信封并丢弃直通资格。
    #[must_use]
    pub const fn canonical(&self) -> &CanonicalRequest {
        &self.canonical
    }

    /// 返回已验证源正文所属协议；程序内部构造的请求返回 `None`。
    #[must_use]
    pub const fn source_protocol(&self) -> Option<Protocol> {
        match &self.source {
            Some(source) => Some(source.protocol),
            None => None,
        }
    }

    /// 返回客户端提交的原始模型别名；未发生归一时返回 Canonical 模型。
    ///
    /// 返回值属于敏感请求上下文，不得直接写入日志。
    #[must_use]
    pub fn requested_model(&self) -> &str {
        self.requested_model
            .as_deref()
            .unwrap_or(&self.canonical.model)
    }

    /// 消费信封并仅保留 Canonical 请求，同时永久丢弃原始正文。
    #[must_use]
    pub fn into_canonical(self) -> CanonicalRequest {
        self.canonical
    }

    /// 仅当目标协议与已验证源协议相同时，交付不可变的原始正文。
    ///
    /// 模型映射、参数覆盖或内部字段注入发生前，调用方必须改走 [`Self::into_canonical`]
    /// 并由目标协议构造器重新编码，不能继续消费这里返回的正文。
    pub fn into_same_protocol(self, target_protocol: Protocol) -> SameProtocolDecision {
        if self
            .source
            .as_ref()
            .is_none_or(|source| source.protocol != target_protocol)
        {
            return SameProtocolDecision::Rebuild(self);
        }

        let Self {
            canonical,
            source: Some(source),
            requested_model,
        } = self
        else {
            unreachable!("已检查的源协议正文必须存在");
        };
        SameProtocolDecision::Passthrough(SameProtocolRequest {
            protocol: source.protocol,
            canonical,
            body: source.body,
            requested_model,
        })
    }
}

impl From<CanonicalRequest> for CanonicalRequestEnvelope {
    fn from(canonical: CanonicalRequest) -> Self {
        Self::from_canonical(canonical)
    }
}

impl fmt::Debug for CanonicalRequestEnvelope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalRequestEnvelope")
            .field("canonical", &self.canonical)
            .field("source_protocol", &self.source_protocol())
            .field("has_requested_model_alias", &self.requested_model.is_some())
            .field(
                "source_body_bytes",
                &self.source.as_ref().map(|source| source.body.len()),
            )
            .finish()
    }
}

#[derive(Clone, PartialEq)]
struct ValidatedSourceRequest {
    protocol: Protocol,
    body: Bytes,
}

/// 同协议正文资格检查的闭合决策。
///
/// 调用方必须显式处理重构分支，不能在协议不匹配或缺少已验证正文时静默直通。
#[derive(Debug)]
pub enum SameProtocolDecision {
    /// 可以向同协议目标发送已验证的原始正文。
    Passthrough(SameProtocolRequest),
    /// 必须消费 Canonical 语义并由目标协议构造器重新编码。
    Rebuild(CanonicalRequestEnvelope),
}

/// 已证明源协议与目标协议一致、且请求语义未被改写的正文。
pub struct SameProtocolRequest {
    protocol: Protocol,
    canonical: CanonicalRequest,
    body: Bytes,
    requested_model: Option<String>,
}

impl SameProtocolRequest {
    /// 返回直通正文使用的协议。
    #[must_use]
    pub const fn protocol(&self) -> Protocol {
        self.protocol
    }

    /// 只读访问已经通过源协议信任边界校验的 Canonical 语义。
    #[must_use]
    pub const fn canonical(&self) -> &CanonicalRequest {
        &self.canonical
    }

    /// 返回保持客户端原始表示的正文。
    #[must_use]
    pub const fn body(&self) -> &Bytes {
        &self.body
    }

    /// 返回客户端提交的原始模型别名；未发生归一时返回 Canonical 模型。
    #[must_use]
    pub fn requested_model(&self) -> &str {
        self.requested_model
            .as_deref()
            .unwrap_or(&self.canonical.model)
    }

    /// 消费证明并拆分 Canonical 语义与原始正文。
    #[must_use]
    pub fn into_parts(self) -> (CanonicalRequest, Bytes) {
        (self.canonical, self.body)
    }
}

impl fmt::Debug for SameProtocolRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SameProtocolRequest")
            .field("protocol", &self.protocol)
            .field("canonical", &self.canonical)
            .field("body_bytes", &self.body.len())
            .field("has_requested_model_alias", &self.requested_model.is_some())
            .finish()
    }
}
