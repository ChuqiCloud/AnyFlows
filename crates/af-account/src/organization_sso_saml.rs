//! 企业 SSO SAML XMLDSig 与已签名断言语义的受限纯验证边界。
//!
//! 本模块只消费已由受控回调层取得的 SAML Response 与 Provider 锁定的证书，
//! 不发起网络请求、不写入数据库；语义验证只输出受控身份和重放摘要投影。

use std::{
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use quick_xml::{
    XmlVersion,
    events::{BytesStart, Event},
    name::ResolveResult,
    reader::NsReader,
};
use saml_rs::{
    AcsEndpoint, AuthnRequestSigningPolicy, BrowserInput, ClockSkew, EntityId, FormField,
    IdpDescriptor, MessageId, MetadataTrustPolicy, NameIdFormat, PendingAuthnRequest,
    RelayStateParam, ReplayCache, ReplayKey, ReplayPolicy, Saml, SamlError, SamlValidationContext,
    SpConfig, SpValidationPolicy, SsoResponse, SsoResponseBinding, XmlPolicy,
    constants::{Binding, digest_algorithm, namespace, signature_algorithm, transform_algorithm},
    crypto::verify_signature_with_limits,
    metadata::{
        Endpoint as SamlMetadataEndpoint, IdpMetadataConfig as SamlIdpMetadataConfig,
        generate_idp_metadata,
    },
    xml::XmlLimits,
};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use url::Url;

const MAX_SAML_RESPONSE_BYTES: usize = 128 * 1024;
const MAX_SAML_CERTIFICATE_BYTES: usize = 16 * 1024;
const MAX_SAML_CERTIFICATES: usize = 8;
const MAX_SAML_XML_ATTRIBUTES: usize = 32;
const MAX_SAML_XML_DEPTH: usize = 32;
const MAX_SAML_XML_NODES: usize = 2_048;
const MAX_SAML_XML_TEXT_BYTES: usize = 16 * 1024;
const MAX_SAML_SIGNATURES: usize = 2;
const MAX_SAML_ENTITY_ID_BYTES: usize = 2_048;
const MAX_SAML_ENDPOINT_BYTES: usize = 2_048;
const MAX_SAML_REQUEST_ID_BYTES: usize = 255;
const MAX_SAML_SUBJECT_BYTES: usize = 1_024;
const SAML_CLOCK_SKEW_MILLIS: i64 = 60_000;

const SAML_XML_LIMITS: XmlLimits = XmlLimits {
    max_bytes: MAX_SAML_RESPONSE_BYTES,
    max_depth: MAX_SAML_XML_DEPTH,
    max_nodes: MAX_SAML_XML_NODES,
    max_attributes_per_element: MAX_SAML_XML_ATTRIBUTES,
    max_attribute_value_bytes: 4 * 1024,
    max_text_bytes: MAX_SAML_XML_TEXT_BYTES,
};

/// 企业 SAML 验签所需的 Provider 配置快照。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationSsoSamlSignatureVerificationConfig {
    trusted_certificates: Vec<String>,
}

impl OrganizationSsoSamlSignatureVerificationConfig {
    /// 固定当前 Provider 配置版本允许的受信签名证书，支持有限数量的证书轮换。
    pub fn new(certificates: Vec<String>) -> Result<Self, OrganizationSsoSamlSignatureError> {
        if certificates.is_empty() || certificates.len() > MAX_SAML_CERTIFICATES {
            return Err(OrganizationSsoSamlSignatureError::InvalidConfiguration);
        }

        let trusted_certificates = certificates
            .into_iter()
            .map(|certificate| {
                let certificate = certificate.trim().to_owned();
                if !valid_certificate(&certificate) {
                    return Err(OrganizationSsoSamlSignatureError::InvalidConfiguration);
                }
                Ok(certificate)
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            trusted_certificates,
        })
    }
}

impl fmt::Debug for OrganizationSsoSamlSignatureVerificationConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationSsoSamlSignatureVerificationConfig(<redacted>)")
    }
}

/// 已通过受信 XMLDSig 验证且没有签名包装歧义的 SAML Response 标记。
#[derive(Clone, Eq, PartialEq)]
pub struct VerifiedOrganizationSsoSamlResponse {
    response_destination: Option<String>,
}

impl fmt::Debug for VerifiedOrganizationSsoSamlResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VerifiedOrganizationSsoSamlResponse")
    }
}

/// 企业 SAML XML 输入、证书或 XMLDSig 验证失败时使用的稳定错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoSamlSignatureError {
    /// Provider 快照没有受控的受信证书集合。
    #[error("企业 SAML Provider 配置无效")]
    InvalidConfiguration,
    /// Response 不满足受限 XML 与 SHA-256 XMLDSig 验证规则。
    #[error("企业 SAML Response 验证失败")]
    Rejected,
}

/// 企业 SAML XMLDSig 纯验证器；回调、事务和身份绑定必须由后续受控服务组合。
#[derive(Clone)]
pub struct OrganizationSsoSamlSignatureVerifier {
    config: OrganizationSsoSamlSignatureVerificationConfig,
}

impl OrganizationSsoSamlSignatureVerifier {
    /// 绑定创建登录事务时锁定的 Provider 配置版本中的受信证书。
    #[must_use]
    pub const fn new(config: OrganizationSsoSamlSignatureVerificationConfig) -> Self {
        Self { config }
    }

    /// 验证严格受限的单 Assertion Response 与受信 SHA-256 XMLDSig。
    pub fn verify(
        &self,
        saml_response: &str,
    ) -> Result<VerifiedOrganizationSsoSamlResponse, OrganizationSsoSamlSignatureError> {
        let inspection = inspect_saml_response(saml_response)?;

        // 上游库只以配置证书作为信任锚，并复验引用覆盖、重复 ID 与签名包装。
        let (verified, signed_content) = verify_signature_with_limits(
            saml_response,
            &self.config.trusted_certificates,
            SAML_XML_LIMITS,
        )
        .map_err(|_| OrganizationSsoSamlSignatureError::Rejected)?;
        if !verified || signed_content.is_none() {
            return Err(OrganizationSsoSamlSignatureError::Rejected);
        }

        Ok(VerifiedOrganizationSsoSamlResponse {
            response_destination: inspection.response_destination,
        })
    }
}

impl fmt::Debug for OrganizationSsoSamlSignatureVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationSsoSamlSignatureVerifier(<configuration>)")
    }
}

/// 企业 SAML 允许的受控 NameID 格式；不接受管理员提供任意 XPath。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationSsoSamlNameIdFormat {
    /// IdP 为同一主体维持稳定标识。
    Persistent,
    /// IdP 为当前会话签发临时标识。
    Transient,
    /// IdP 明确声明邮箱格式；该值仍只作为 subject，不自动关联邮箱。
    EmailAddress,
}

impl OrganizationSsoSamlNameIdFormat {
    fn as_saml_format(self) -> NameIdFormat {
        match self {
            Self::Persistent => NameIdFormat::Persistent,
            Self::Transient => NameIdFormat::Transient,
            Self::EmailAddress => NameIdFormat::EmailAddress,
        }
    }

    const fn digest_label(self) -> &'static [u8] {
        match self {
            Self::Persistent => b"persistent",
            Self::Transient => b"transient",
            Self::EmailAddress => b"email_address",
        }
    }
}

/// 企业 SAML Provider 的版本化闭合配置投影；不包含原始 metadata/XML。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationSsoSamlProviderConfig {
    provider_version: i64,
    idp_entity_id: String,
    idp_sso_url: String,
    sp_entity_id: String,
    acs_url: String,
    name_id_format: OrganizationSsoSamlNameIdFormat,
    trusted_certificates: Vec<String>,
}

impl OrganizationSsoSamlProviderConfig {
    /// 构造并校验当前 Provider 版本的 SAML 配置快照。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与持久化 Provider 配置投影一一对应"
    )]
    pub fn new(
        provider_version: i64,
        idp_entity_id: String,
        idp_sso_url: String,
        sp_entity_id: String,
        acs_url: String,
        name_id_format: OrganizationSsoSamlNameIdFormat,
        trusted_certificates: Vec<String>,
    ) -> Result<Self, OrganizationSsoSamlSemanticError> {
        if provider_version <= 0
            || !valid_entity_id(&idp_entity_id)
            || !valid_https_endpoint(&idp_sso_url)
            || !valid_entity_id(&sp_entity_id)
            || !valid_https_endpoint(&acs_url)
        {
            return Err(OrganizationSsoSamlSemanticError::InvalidConfiguration);
        }
        let signature = OrganizationSsoSamlSignatureVerificationConfig::new(trusted_certificates)
            .map_err(|_| OrganizationSsoSamlSemanticError::InvalidConfiguration)?;
        Ok(Self {
            provider_version,
            idp_entity_id,
            idp_sso_url,
            sp_entity_id,
            acs_url,
            name_id_format,
            trusted_certificates: signature.trusted_certificates,
        })
    }

    /// 从持久化 JSON 投影恢复严格配置；协议标记和字段集合必须完全匹配。
    pub fn from_metadata_projection(
        provider_version: i64,
        projection: &Value,
    ) -> Result<Self, OrganizationSsoSamlSemanticError> {
        let object = projection
            .as_object()
            .ok_or(OrganizationSsoSamlSemanticError::InvalidConfiguration)?;
        const KEYS: [&str; 7] = [
            "idp_entity_id",
            "sso_url",
            "sp_entity_id",
            "acs_url",
            "name_id_format",
            "trusted_certificates",
            "protocol",
        ];
        if object.len() != KEYS.len() || object.keys().any(|key| !KEYS.contains(&key.as_str())) {
            return Err(OrganizationSsoSamlSemanticError::InvalidConfiguration);
        }
        if object.get("protocol").and_then(Value::as_str) != Some("saml") {
            return Err(OrganizationSsoSamlSemanticError::InvalidConfiguration);
        }
        let string = |key: &str| {
            object
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or(OrganizationSsoSamlSemanticError::InvalidConfiguration)
        };
        let name_id_format = match string("name_id_format")?.as_str() {
            "persistent" => OrganizationSsoSamlNameIdFormat::Persistent,
            "transient" => OrganizationSsoSamlNameIdFormat::Transient,
            "email_address" => OrganizationSsoSamlNameIdFormat::EmailAddress,
            _ => return Err(OrganizationSsoSamlSemanticError::InvalidConfiguration),
        };
        let certificates = object
            .get("trusted_certificates")
            .and_then(Value::as_array)
            .ok_or(OrganizationSsoSamlSemanticError::InvalidConfiguration)?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or(OrganizationSsoSamlSemanticError::InvalidConfiguration)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(
            provider_version,
            string("idp_entity_id")?,
            string("sso_url")?,
            string("sp_entity_id")?,
            string("acs_url")?,
            name_id_format,
            certificates,
        )
    }

    /// 返回规范化的持久化 JSON 投影；Provider 版本由数据库行单独保存。
    #[must_use]
    pub fn to_metadata_projection(&self) -> Value {
        json!({
            "idp_entity_id": self.idp_entity_id.as_str(),
            "sso_url": self.idp_sso_url.as_str(),
            "sp_entity_id": self.sp_entity_id.as_str(),
            "acs_url": self.acs_url.as_str(),
            "name_id_format": self.name_id_format.as_str(),
            "trusted_certificates": &self.trusted_certificates,
            "protocol": "saml",
        })
    }

    /// 将 Provider 快照与当前事务 Request ID 组合为语义验证配置。
    pub fn with_request_id(
        &self,
        request_id: String,
    ) -> Result<OrganizationSsoSamlSemanticVerificationConfig, OrganizationSsoSamlSemanticError>
    {
        OrganizationSsoSamlSemanticVerificationConfig::new(
            self.idp_entity_id.clone(),
            self.idp_sso_url.clone(),
            self.sp_entity_id.clone(),
            self.acs_url.clone(),
            request_id,
            self.name_id_format,
            self.trusted_certificates.clone(),
        )
    }

    #[must_use]
    pub const fn provider_version(&self) -> i64 {
        self.provider_version
    }
}

impl OrganizationSsoSamlNameIdFormat {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Persistent => "persistent",
            Self::Transient => "transient",
            Self::EmailAddress => "email_address",
        }
    }
}

/// SAML 语义验证所需的 Provider 与登录事务不可变快照。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationSsoSamlSemanticVerificationConfig {
    idp_entity_id: String,
    idp_sso_url: String,
    sp_entity_id: String,
    acs_url: String,
    request_id: String,
    name_id_format: OrganizationSsoSamlNameIdFormat,
    signature: OrganizationSsoSamlSignatureVerificationConfig,
}

impl OrganizationSsoSamlSemanticVerificationConfig {
    /// 固定 Provider 版本、SP 路由、事务 Request ID、NameID 格式和证书集合。
    #[allow(
        clippy::too_many_arguments,
        reason = "参数与单次 SAML Provider/事务安全快照一一对应"
    )]
    pub fn new(
        idp_entity_id: String,
        idp_sso_url: String,
        sp_entity_id: String,
        acs_url: String,
        request_id: String,
        name_id_format: OrganizationSsoSamlNameIdFormat,
        trusted_certificates: Vec<String>,
    ) -> Result<Self, OrganizationSsoSamlSemanticError> {
        if !valid_entity_id(&idp_entity_id)
            || !valid_entity_id(&sp_entity_id)
            || !valid_https_endpoint(&idp_sso_url)
            || !valid_https_endpoint(&acs_url)
            || !valid_request_id(&request_id)
        {
            return Err(OrganizationSsoSamlSemanticError::InvalidConfiguration);
        }
        let signature = OrganizationSsoSamlSignatureVerificationConfig::new(trusted_certificates)
            .map_err(|_| OrganizationSsoSamlSemanticError::InvalidConfiguration)?;
        Ok(Self {
            idp_entity_id,
            idp_sso_url,
            sp_entity_id,
            acs_url,
            request_id,
            name_id_format,
            signature,
        })
    }
}

impl fmt::Debug for OrganizationSsoSamlSemanticVerificationConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationSsoSamlSemanticVerificationConfig(<redacted>)")
    }
}

/// 已通过签名、相关性、目标、受众、时间与 NameID 格式验证的最小身份投影。
#[derive(Eq, PartialEq)]
pub struct VerifiedOrganizationSsoSamlIdentity {
    subject: String,
    subject_digest: String,
    response_replay_digest: String,
    assertion_replay_digest: String,
    replay_expires_at: i64,
}

impl VerifiedOrganizationSsoSamlIdentity {
    /// 返回已验证 NameID subject；调用方不得写入日志或普通响应。
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// 返回 NameID 格式与 subject 共同形成的域分离摘要。
    #[must_use]
    pub fn subject_digest(&self) -> &str {
        &self.subject_digest
    }

    /// 返回身份断言摘要，保持与 OIDC 完成事务接口的一致命名。
    #[must_use]
    pub fn assertion_digest(&self) -> &str {
        self.subject_digest()
    }

    /// 返回 Response ID 的域分离重放摘要，不返回原始协议 ID。
    #[must_use]
    pub fn response_replay_digest(&self) -> &str {
        &self.response_replay_digest
    }

    /// 返回 Assertion ID 的域分离重放摘要，不返回原始协议 ID。
    #[must_use]
    pub fn assertion_replay_digest(&self) -> &str {
        &self.assertion_replay_digest
    }

    /// 返回两个重放摘要共同使用的最早安全截止时间（UTC Unix 秒）。
    #[must_use]
    pub const fn replay_expires_at(&self) -> i64 {
        self.replay_expires_at
    }
}

impl fmt::Debug for VerifiedOrganizationSsoSamlIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VerifiedOrganizationSsoSamlIdentity(<redacted>)")
    }
}

/// 企业 SAML 配置或断言语义失败时使用的稳定错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoSamlSemanticError {
    /// Provider 或事务快照不满足固定 SAML 安全边界。
    #[error("企业 SAML 语义验证配置无效")]
    InvalidConfiguration,
    /// Response 不满足签名、相关性、目标、受众、时间或身份规则。
    #[error("企业 SAML Response 语义验证失败")]
    Rejected,
}

/// 组合受限 XMLDSig 与 `saml-rs` 严格 SP 流程的纯语义验证器。
pub struct OrganizationSsoSamlSemanticVerifier {
    signature: OrganizationSsoSamlSignatureVerifier,
    sp: Saml<saml_rs::Sp>,
    idp: IdpDescriptor,
    pending: PendingAuthnRequest,
    name_id_format: OrganizationSsoSamlNameIdFormat,
}

impl OrganizationSsoSamlSemanticVerifier {
    /// 构造只接受当前 Provider 版本和登录事务快照的语义验证器。
    pub fn new(
        config: OrganizationSsoSamlSemanticVerificationConfig,
    ) -> Result<Self, OrganizationSsoSamlSemanticError> {
        let trusted_certificates = config.signature.trusted_certificates.clone();
        let signature = OrganizationSsoSamlSignatureVerifier::new(config.signature);
        let sp_entity_id = EntityId::try_new(config.sp_entity_id)
            .map_err(|_| OrganizationSsoSamlSemanticError::InvalidConfiguration)?;
        let idp_entity_id = EntityId::try_new(config.idp_entity_id)
            .map_err(|_| OrganizationSsoSamlSemanticError::InvalidConfiguration)?;
        let acs = AcsEndpoint::post(config.acs_url)
            .map_err(|_| OrganizationSsoSamlSemanticError::InvalidConfiguration)?;

        let mut validation = SpValidationPolicy::strict();
        // 本验证器不生成 AuthnRequest；请求签名由后续发起切片独立接入。
        validation.authn_requests = AuthnRequestSigningPolicy::DoNotSignForCompatibility;
        let sp = Saml::sp(
            SpConfig::builder(sp_entity_id)
                .acs_endpoint(acs.clone())
                .validation(validation)
                .xml(XmlPolicy {
                    limits: SAML_XML_LIMITS,
                    ..XmlPolicy::default()
                })
                .build()
                .map_err(|_| OrganizationSsoSamlSemanticError::InvalidConfiguration)?,
        )
        .map_err(|_| OrganizationSsoSamlSemanticError::InvalidConfiguration)?;

        let metadata = generate_idp_metadata(&SamlIdpMetadataConfig {
            entity_id: idp_entity_id.as_str().to_owned(),
            signing_certs: trusted_certificates,
            single_sign_on_service: vec![SamlMetadataEndpoint::new(
                Binding::Post,
                config.idp_sso_url,
            )],
            ..SamlIdpMetadataConfig::default()
        });
        // 元数据由受控快照在本进程生成；兼容标记不接收外部未签名 metadata，
        // Response/Assertion 仍必须由同一组固定证书完成严格 XMLDSig 验证。
        let idp = IdpDescriptor::from_metadata_xml_for(
            idp_entity_id.clone(),
            &metadata,
            MetadataTrustPolicy::UnsignedForCompatibility,
        )
        .map_err(|_| OrganizationSsoSamlSemanticError::InvalidConfiguration)?;
        let pending = PendingAuthnRequest::try_new(
            MessageId::try_new(config.request_id)
                .map_err(|_| OrganizationSsoSamlSemanticError::InvalidConfiguration)?,
            RelayStateParam::absent(),
            acs,
            SsoResponseBinding::Post,
            idp_entity_id,
        )
        .map_err(|_| OrganizationSsoSamlSemanticError::InvalidConfiguration)?;

        Ok(Self {
            signature,
            sp,
            idp,
            pending,
            name_id_format: config.name_id_format,
        })
    }

    /// 验证 XMLDSig 和 SP 发起断言语义，并输出可持久化的脱敏投影。
    pub fn verify(
        &self,
        saml_response: &str,
        now: SystemTime,
    ) -> Result<VerifiedOrganizationSsoSamlIdentity, OrganizationSsoSamlSemanticError> {
        let signed = self
            .signature
            .verify(saml_response)
            .map_err(|_| OrganizationSsoSamlSemanticError::Rejected)?;
        if signed.response_destination.as_deref() != Some(self.pending.acs().location().as_str()) {
            return Err(OrganizationSsoSamlSemanticError::Rejected);
        }

        let mut replay = SamlReplayProjection::default();
        let validation = SamlValidationContext::new(now, ReplayPolicy::RequireCache(&mut replay))
            .with_clock_skew(ClockSkew::from_millis(
                SAML_CLOCK_SKEW_MILLIS,
                SAML_CLOCK_SKEW_MILLIS,
            ));
        let session = self
            .sp
            .finish_sso(
                &self.idp,
                &self.pending,
                BrowserInput::<SsoResponse>::post(vec![FormField::new(
                    "SAMLResponse",
                    STANDARD.encode(saml_response),
                )]),
                validation,
            )
            .map_err(|_| OrganizationSsoSamlSemanticError::Rejected)?;

        let expected_format = self.name_id_format.as_saml_format();
        let subject = session.name_id().value();
        if session.name_id().format() != Some(&expected_format) || !valid_subject(subject) {
            return Err(OrganizationSsoSamlSemanticError::Rejected);
        }
        let projection = replay.finish()?;
        Ok(VerifiedOrganizationSsoSamlIdentity {
            subject: subject.to_owned(),
            subject_digest: digest_components(
                b"organization-sso-saml-subject-v1",
                &[self.name_id_format.digest_label(), subject.as_bytes()],
            ),
            response_replay_digest: projection.response_digest,
            assertion_replay_digest: projection.assertion_digest,
            replay_expires_at: projection.expires_at,
        })
    }
}

impl fmt::Debug for OrganizationSsoSamlSemanticVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationSsoSamlSemanticVerifier(<configuration>)")
    }
}

#[derive(Default)]
struct SamlReplayProjection {
    response_digest: Option<String>,
    assertion_digest: Option<String>,
    expires_at: Option<i64>,
}

impl SamlReplayProjection {
    fn finish(self) -> Result<CompletedSamlReplayProjection, OrganizationSsoSamlSemanticError> {
        Ok(CompletedSamlReplayProjection {
            response_digest: self
                .response_digest
                .ok_or(OrganizationSsoSamlSemanticError::Rejected)?,
            assertion_digest: self
                .assertion_digest
                .ok_or(OrganizationSsoSamlSemanticError::Rejected)?,
            expires_at: self
                .expires_at
                .ok_or(OrganizationSsoSamlSemanticError::Rejected)?,
        })
    }
}

impl ReplayCache for SamlReplayProjection {
    fn check_and_store(&mut self, key: ReplayKey, expires_at: SystemTime) -> Result<(), SamlError> {
        let expires_at = expires_at
            .duration_since(UNIX_EPOCH)
            .map_err(|_| SamlError::Invalid("SAML replay deadline predates Unix epoch".into()))?
            .as_secs();
        let expires_at = i64::try_from(expires_at)
            .map_err(|_| SamlError::Invalid("SAML replay deadline exceeds storage range".into()))?;
        if self.expires_at.is_some_and(|current| current != expires_at) {
            return Err(SamlError::Invalid(
                "SAML replay keys disagree on expiration".into(),
            ));
        }
        self.expires_at = Some(expires_at);

        let digest = digest_components(
            b"organization-sso-saml-replay-v1",
            &[key.kind().as_bytes(), key.value().as_bytes()],
        );
        let slot = match &key {
            ReplayKey::ResponseId(_) => &mut self.response_digest,
            ReplayKey::AssertionId(_) => &mut self.assertion_digest,
            _ => {
                return Err(SamlError::Invalid(
                    "unexpected SAML replay key family".into(),
                ));
            }
        };
        if slot.is_some() {
            return Err(SamlError::ReplayDetected {
                key: key.kind().to_owned(),
            });
        }
        *slot = Some(digest);
        Ok(())
    }
}

struct CompletedSamlReplayProjection {
    response_digest: String,
    assertion_digest: String,
    expires_at: i64,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ElementKind {
    Response,
    Assertion,
    Signature,
    Other,
}

struct SAMLInputInspection {
    stack: Vec<ElementKind>,
    root_seen: bool,
    response_destination: Option<String>,
    assertion_count: usize,
    certificate_count: usize,
    digest_method_count: usize,
    node_count: usize,
    signature_count: usize,
    signature_method_count: usize,
}

impl SAMLInputInspection {
    fn new() -> Self {
        Self {
            stack: Vec::new(),
            root_seen: false,
            response_destination: None,
            assertion_count: 0,
            certificate_count: 0,
            digest_method_count: 0,
            node_count: 0,
            signature_count: 0,
            signature_method_count: 0,
        }
    }

    fn open(
        &mut self,
        element: &BytesStart<'_>,
        namespace: &ResolveResult<'_>,
        empty: bool,
    ) -> Result<ElementKind, OrganizationSsoSamlSignatureError> {
        self.node_count = self
            .node_count
            .checked_add(1)
            .ok_or(OrganizationSsoSamlSignatureError::Rejected)?;
        if self.node_count > MAX_SAML_XML_NODES || self.stack.len() >= MAX_SAML_XML_DEPTH {
            return Err(OrganizationSsoSamlSignatureError::Rejected);
        }

        let attributes = inspected_attributes(element)?;
        let local_name = element.local_name();
        let local_name = local_name.as_ref();
        let is_response = local_name == b"Response";
        let is_assertion = local_name == b"Assertion";
        let is_signature = local_name == b"Signature";
        let is_dsig = resolved_namespace_is(namespace, namespace::DSIG);

        if self.stack.is_empty() {
            if self.root_seen
                || !is_response
                || !resolved_namespace_is(namespace, namespace::PROTOCOL)
            {
                return Err(OrganizationSsoSamlSignatureError::Rejected);
            }
            self.root_seen = true;
            self.response_destination = attributes.destination.clone();
        } else if is_response {
            return Err(OrganizationSsoSamlSignatureError::Rejected);
        }

        let parent = self.stack.last().copied();
        if is_assertion {
            if !resolved_namespace_is(namespace, namespace::ASSERTION)
                || parent != Some(ElementKind::Response)
                || empty
            {
                return Err(OrganizationSsoSamlSignatureError::Rejected);
            }
            self.assertion_count = self
                .assertion_count
                .checked_add(1)
                .ok_or(OrganizationSsoSamlSignatureError::Rejected)?;
            if self.assertion_count > 1 {
                return Err(OrganizationSsoSamlSignatureError::Rejected);
            }
        }

        let inside_signature = self.stack.contains(&ElementKind::Signature);
        if is_signature {
            if !is_dsig || !matches!(parent, Some(ElementKind::Response | ElementKind::Assertion)) {
                return Err(OrganizationSsoSamlSignatureError::Rejected);
            }
            self.signature_count = self
                .signature_count
                .checked_add(1)
                .ok_or(OrganizationSsoSamlSignatureError::Rejected)?;
            if self.signature_count > MAX_SAML_SIGNATURES {
                return Err(OrganizationSsoSamlSignatureError::Rejected);
            }
        } else if inside_signature && xml_signature_local_name(local_name) && !is_dsig {
            // 上游签名库按本地名定位 XMLDSig 结构，必须在进入前拒绝命名空间替换。
            return Err(OrganizationSsoSamlSignatureError::Rejected);
        }

        if inside_signature && is_dsig {
            self.validate_signature_element(local_name, attributes.algorithm.as_deref())?;
        }

        let kind = if is_response {
            ElementKind::Response
        } else if is_assertion {
            ElementKind::Assertion
        } else if is_signature {
            ElementKind::Signature
        } else {
            ElementKind::Other
        };
        Ok(kind)
    }

    fn validate_signature_element(
        &mut self,
        local_name: &[u8],
        algorithm: Option<&str>,
    ) -> Result<(), OrganizationSsoSamlSignatureError> {
        match local_name {
            b"SignatureMethod" => {
                let algorithm = required_algorithm(algorithm)?;
                if !matches!(
                    algorithm,
                    signature_algorithm::RSA_SHA256 | signature_algorithm::RSA_SHA256_MGF1
                ) {
                    return Err(OrganizationSsoSamlSignatureError::Rejected);
                }
                self.signature_method_count = self
                    .signature_method_count
                    .checked_add(1)
                    .ok_or(OrganizationSsoSamlSignatureError::Rejected)?;
            }
            b"DigestMethod" => {
                if required_algorithm(algorithm)? != digest_algorithm::SHA256 {
                    return Err(OrganizationSsoSamlSignatureError::Rejected);
                }
                self.digest_method_count = self
                    .digest_method_count
                    .checked_add(1)
                    .ok_or(OrganizationSsoSamlSignatureError::Rejected)?;
            }
            b"CanonicalizationMethod" => {
                if required_algorithm(algorithm)? != transform_algorithm::EXC_C14N {
                    return Err(OrganizationSsoSamlSignatureError::Rejected);
                }
            }
            b"Transform" => {
                let algorithm = required_algorithm(algorithm)?;
                if !matches!(
                    algorithm,
                    transform_algorithm::ENVELOPED_SIGNATURE | transform_algorithm::EXC_C14N
                ) {
                    return Err(OrganizationSsoSamlSignatureError::Rejected);
                }
            }
            b"X509Certificate" => {
                self.certificate_count = self
                    .certificate_count
                    .checked_add(1)
                    .ok_or(OrganizationSsoSamlSignatureError::Rejected)?;
                if self.certificate_count > MAX_SAML_CERTIFICATES {
                    return Err(OrganizationSsoSamlSignatureError::Rejected);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn close(&mut self) -> Result<(), OrganizationSsoSamlSignatureError> {
        self.stack
            .pop()
            .map(|_| ())
            .ok_or(OrganizationSsoSamlSignatureError::Rejected)
    }

    fn finish(self) -> Result<SamlInputProjection, OrganizationSsoSamlSignatureError> {
        if !self.stack.is_empty()
            || !self.root_seen
            || self.assertion_count != 1
            || self.signature_count == 0
            || self.signature_method_count == 0
            || self.digest_method_count == 0
        {
            return Err(OrganizationSsoSamlSignatureError::Rejected);
        }
        Ok(SamlInputProjection {
            response_destination: self.response_destination,
        })
    }
}

struct SamlInputProjection {
    response_destination: Option<String>,
}

struct InspectedAttributes {
    algorithm: Option<String>,
    destination: Option<String>,
}

fn inspected_attributes(
    element: &BytesStart<'_>,
) -> Result<InspectedAttributes, OrganizationSsoSamlSignatureError> {
    let mut algorithm = None;
    let mut destination = None;
    let mut count = 0usize;
    for attribute in element.attributes().with_checks(true) {
        let attribute = attribute.map_err(|_| OrganizationSsoSamlSignatureError::Rejected)?;
        count = count
            .checked_add(1)
            .ok_or(OrganizationSsoSamlSignatureError::Rejected)?;
        if count > MAX_SAML_XML_ATTRIBUTES {
            return Err(OrganizationSsoSamlSignatureError::Rejected);
        }
        let slot = match attribute.key.as_ref() {
            b"Algorithm" => Some(&mut algorithm),
            b"Destination" => Some(&mut destination),
            _ => None,
        };
        if let Some(slot) = slot {
            if slot.is_some() {
                return Err(OrganizationSsoSamlSignatureError::Rejected);
            }
            *slot = Some(
                attribute
                    .decoded_and_normalized_value(XmlVersion::Implicit1_0, element.decoder())
                    .map_err(|_| OrganizationSsoSamlSignatureError::Rejected)?
                    .into_owned(),
            );
        }
    }
    Ok(InspectedAttributes {
        algorithm,
        destination,
    })
}

fn inspect_saml_response(
    saml_response: &str,
) -> Result<SamlInputProjection, OrganizationSsoSamlSignatureError> {
    if saml_response.is_empty() || saml_response.len() > MAX_SAML_RESPONSE_BYTES {
        return Err(OrganizationSsoSamlSignatureError::Rejected);
    }

    let mut reader = NsReader::from_str(saml_response);
    let mut inspection = SAMLInputInspection::new();
    loop {
        let (namespace, event) = reader
            .read_resolved_event()
            .map_err(|_| OrganizationSsoSamlSignatureError::Rejected)?;
        match event {
            Event::Start(element) => {
                let kind = inspection.open(&element, &namespace, false)?;
                inspection.stack.push(kind);
            }
            Event::Empty(element) => {
                let _ = inspection.open(&element, &namespace, true)?;
            }
            Event::End(_) => inspection.close()?,
            // 禁止 DTD 可同时阻断实体展开与外部实体读取路径。
            Event::DocType(_) => return Err(OrganizationSsoSamlSignatureError::Rejected),
            Event::Eof => break,
            _ => {}
        }
    }
    inspection.finish()
}

fn resolved_namespace_is(namespace: &ResolveResult<'_>, expected: &str) -> bool {
    matches!(namespace, ResolveResult::Bound(value) if value.as_ref() == expected.as_bytes())
}

fn xml_signature_local_name(local_name: &[u8]) -> bool {
    matches!(
        local_name,
        b"Signature"
            | b"SignedInfo"
            | b"CanonicalizationMethod"
            | b"SignatureMethod"
            | b"Reference"
            | b"Transforms"
            | b"Transform"
            | b"DigestMethod"
            | b"DigestValue"
            | b"SignatureValue"
            | b"KeyInfo"
            | b"X509Data"
            | b"X509Certificate"
    )
}

fn required_algorithm(algorithm: Option<&str>) -> Result<&str, OrganizationSsoSamlSignatureError> {
    algorithm
        .filter(|value| !value.is_empty() && value.trim() == *value && value.is_ascii())
        .ok_or(OrganizationSsoSamlSignatureError::Rejected)
}

fn valid_certificate(certificate: &str) -> bool {
    certificate.len() <= MAX_SAML_CERTIFICATE_BYTES
        && certificate.is_ascii()
        && certificate.starts_with("-----BEGIN CERTIFICATE-----")
        && certificate.ends_with("-----END CERTIFICATE-----")
}

fn valid_entity_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SAML_ENTITY_ID_BYTES
        && value.trim() == value
        && value.is_ascii()
        && !value.chars().any(char::is_control)
}

fn valid_https_endpoint(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_SAML_ENDPOINT_BYTES
        || value.trim() != value
        || !value.is_ascii()
    {
        return false;
    }
    Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
    })
}

fn valid_request_id(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_SAML_REQUEST_ID_BYTES || !value.is_ascii() {
        return false;
    }
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    matches!(first, b'_' | b'A'..=b'Z' | b'a'..=b'z')
        && bytes.all(
            |byte| matches!(byte, b'_' | b'-' | b'.' | b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'),
        )
}

fn valid_subject(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SAML_SUBJECT_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn digest_components(domain: &[u8], components: &[&[u8]]) -> String {
    let mut digest = Sha256::new();
    digest.update(domain);
    for component in components {
        digest.update([0]);
        digest.update(component);
    }
    encode_digest(&digest.finalize())
}

fn encode_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use saml_rs::crypto::{construct_saml_signature, keys::load_private_key};
    use serde_json::json;

    const IDP_ENTITY_ID: &str = "https://idp.example.com/metadata";
    const IDP_SSO_URL: &str = "https://idp.example.com/sso";
    const SP_ENTITY_ID: &str = "https://gateway.example.com/saml/metadata";
    const ACS_URL: &str = "https://gateway.example.com/api/auth/organization-sso/saml/callback";
    const REQUEST_ID: &str = "_request-semantic-1";
    const VALIDATION_NOW: u64 = 946_684_800;
    const REPLAY_EXPIRATION: i64 = 946_685_160;
    const TEST_CERTIFICATE: &str =
        include_str!("fixtures/organization_sso_saml_test_certificate.pem");
    const TEST_PRIVATE_KEY: &str = include_str!("fixtures/organization_sso_saml_test_private.pem");
    const UNSIGNED_RESPONSE: &str = r#"<samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol" xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion" ID="_response-1" Version="2.0" IssueInstant="2026-08-24T00:00:00Z"><saml:Issuer>https://idp.example.com/metadata</saml:Issuer><samlp:Status><samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/></samlp:Status><saml:Assertion ID="_assertion-1" Version="2.0" IssueInstant="2026-08-24T00:00:00Z"><saml:Issuer>https://idp.example.com/metadata</saml:Issuer><saml:Subject><saml:NameID>directory-subject-123</saml:NameID></saml:Subject></saml:Assertion></samlp:Response>"#;

    #[test]
    fn provider_projection_is_closed_and_round_trips() {
        let projection = json!({
            "idp_entity_id": IDP_ENTITY_ID,
            "sso_url": IDP_SSO_URL,
            "sp_entity_id": SP_ENTITY_ID,
            "acs_url": ACS_URL,
            "name_id_format": "persistent",
            "trusted_certificates": [TEST_CERTIFICATE],
            "protocol": "saml",
        });
        let config =
            OrganizationSsoSamlProviderConfig::from_metadata_projection(7, &projection).unwrap();
        assert_eq!(config.provider_version(), 7);
        assert_eq!(config.to_metadata_projection(), projection);
        assert!(config.with_request_id(REQUEST_ID.to_owned()).is_ok());
    }

    #[test]
    fn provider_projection_rejects_unknown_fields_and_unsafe_values() {
        let valid = json!({
            "idp_entity_id": IDP_ENTITY_ID,
            "sso_url": IDP_SSO_URL,
            "sp_entity_id": SP_ENTITY_ID,
            "acs_url": ACS_URL,
            "name_id_format": "persistent",
            "trusted_certificates": [TEST_CERTIFICATE],
            "protocol": "saml",
        });
        for invalid in [
            {
                let mut value = valid.clone();
                value["unsafe"] = json!(true);
                value
            },
            {
                let mut value = valid.clone();
                value["protocol"] = json!("oidc");
                value
            },
            {
                let mut value = valid.clone();
                value["sso_url"] = json!("https://idp.example.com/sso?next=unsafe");
                value
            },
        ] {
            assert_eq!(
                OrganizationSsoSamlProviderConfig::from_metadata_projection(7, &invalid),
                Err(OrganizationSsoSamlSemanticError::InvalidConfiguration)
            );
        }
    }

    fn verifier() -> OrganizationSsoSamlSignatureVerifier {
        OrganizationSsoSamlSignatureVerifier::new(
            OrganizationSsoSamlSignatureVerificationConfig::new(vec![TEST_CERTIFICATE.to_owned()])
                .unwrap(),
        )
    }

    fn signed_response() -> String {
        let key = load_private_key(TEST_PRIVATE_KEY, None).unwrap();
        construct_saml_signature(
            UNSIGNED_RESPONSE,
            false,
            &key,
            TEST_CERTIFICATE,
            signature_algorithm::RSA_SHA256,
            &[],
            None,
        )
        .unwrap()
    }

    #[derive(Clone, Copy)]
    struct SemanticResponse<'a> {
        audience: &'a str,
        recipient: &'a str,
        destination: &'a str,
        in_response_to: &'a str,
        not_before: &'a str,
        not_on_or_after: &'a str,
        name_id_format: &'a str,
        subject: &'a str,
    }

    impl Default for SemanticResponse<'static> {
        fn default() -> Self {
            Self {
                audience: SP_ENTITY_ID,
                recipient: ACS_URL,
                destination: ACS_URL,
                in_response_to: REQUEST_ID,
                not_before: "1999-12-31T23:59:00Z",
                not_on_or_after: "2000-01-01T00:05:00Z",
                name_id_format: saml_rs::constants::name_id_format::PERSISTENT,
                subject: "directory-subject-123",
            }
        }
    }

    fn signed_semantic_response(input: SemanticResponse<'_>) -> String {
        let unsigned = format!(
            r#"<samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol" xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion" ID="_response-semantic-1" Version="2.0" IssueInstant="2000-01-01T00:00:00Z" Destination="{}" InResponseTo="{}"><saml:Issuer>{IDP_ENTITY_ID}</saml:Issuer><samlp:Status><samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/></samlp:Status><saml:Assertion ID="_assertion-semantic-1" Version="2.0" IssueInstant="2000-01-01T00:00:00Z"><saml:Issuer>{IDP_ENTITY_ID}</saml:Issuer><saml:Subject><saml:NameID Format="{}">{}</saml:NameID><saml:SubjectConfirmation Method="urn:oasis:names:tc:SAML:2.0:cm:bearer"><saml:SubjectConfirmationData NotOnOrAfter="{}" Recipient="{}" InResponseTo="{}"/></saml:SubjectConfirmation></saml:Subject><saml:Conditions NotBefore="{}" NotOnOrAfter="{}"><saml:AudienceRestriction><saml:Audience>{}</saml:Audience></saml:AudienceRestriction></saml:Conditions><saml:AuthnStatement AuthnInstant="2000-01-01T00:00:00Z" SessionNotOnOrAfter="{}"><saml:AuthnContext><saml:AuthnContextClassRef>urn:oasis:names:tc:SAML:2.0:ac:classes:PasswordProtectedTransport</saml:AuthnContextClassRef></saml:AuthnContext></saml:AuthnStatement></saml:Assertion></samlp:Response>"#,
            input.destination,
            input.in_response_to,
            input.name_id_format,
            input.subject,
            input.not_on_or_after,
            input.recipient,
            input.in_response_to,
            input.not_before,
            input.not_on_or_after,
            input.audience,
            input.not_on_or_after,
        );
        let key = load_private_key(TEST_PRIVATE_KEY, None).unwrap();
        construct_saml_signature(
            &unsigned,
            false,
            &key,
            TEST_CERTIFICATE,
            signature_algorithm::RSA_SHA256,
            &[],
            None,
        )
        .unwrap()
    }

    fn semantic_config()
    -> Result<OrganizationSsoSamlSemanticVerificationConfig, OrganizationSsoSamlSemanticError> {
        OrganizationSsoSamlSemanticVerificationConfig::new(
            IDP_ENTITY_ID.to_owned(),
            IDP_SSO_URL.to_owned(),
            SP_ENTITY_ID.to_owned(),
            ACS_URL.to_owned(),
            REQUEST_ID.to_owned(),
            OrganizationSsoSamlNameIdFormat::Persistent,
            vec![TEST_CERTIFICATE.to_owned()],
        )
    }

    fn semantic_verifier() -> OrganizationSsoSamlSemanticVerifier {
        OrganizationSsoSamlSemanticVerifier::new(semantic_config().unwrap()).unwrap()
    }

    fn validation_now() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(VALIDATION_NOW)
    }

    #[test]
    fn verifier_accepts_a_real_trusted_rsa_sha256_signature() {
        let verified = verifier().verify(&signed_response()).unwrap();

        assert_eq!(
            format!("{verified:?}"),
            "VerifiedOrganizationSsoSamlResponse"
        );
    }

    #[test]
    fn verifier_rejects_weak_algorithms_dtd_and_assertion_ambiguity() {
        let weak_algorithm = signed_response().replacen(
            signature_algorithm::RSA_SHA256,
            signature_algorithm::RSA_SHA1,
            1,
        );
        assert_eq!(
            verifier().verify(&weak_algorithm),
            Err(OrganizationSsoSamlSignatureError::Rejected)
        );

        let dtd = format!("<!DOCTYPE Response [<!ENTITY marker \"x\">]>{UNSIGNED_RESPONSE}");
        assert_eq!(
            verifier().verify(&dtd),
            Err(OrganizationSsoSamlSignatureError::Rejected)
        );

        let ambiguous = signed_response().replacen(
            "</samlp:Response>",
            "<saml:Assertion xmlns:saml=\"urn:oasis:names:tc:SAML:2.0:assertion\" ID=\"_assertion-2\"><saml:Subject/></saml:Assertion></samlp:Response>",
            1,
        );
        assert_eq!(
            verifier().verify(&ambiguous),
            Err(OrganizationSsoSamlSignatureError::Rejected)
        );
    }

    #[test]
    fn configuration_requires_a_bounded_pinned_certificate_set() {
        assert_eq!(
            OrganizationSsoSamlSignatureVerificationConfig::new(Vec::new()),
            Err(OrganizationSsoSamlSignatureError::InvalidConfiguration)
        );
        assert_eq!(
            OrganizationSsoSamlSignatureVerificationConfig::new(vec![
                "not a certificate".to_owned()
            ]),
            Err(OrganizationSsoSamlSignatureError::InvalidConfiguration)
        );
    }

    #[test]
    fn semantic_verifier_projects_a_controlled_subject_and_replay_evidence() {
        let identity = semantic_verifier()
            .verify(
                &signed_semantic_response(SemanticResponse::default()),
                validation_now(),
            )
            .unwrap();

        assert_eq!(identity.subject(), "directory-subject-123");
        assert_eq!(identity.assertion_digest(), identity.subject_digest());
        assert_eq!(identity.subject_digest().len(), 64);
        assert_eq!(identity.response_replay_digest().len(), 64);
        assert_eq!(identity.assertion_replay_digest().len(), 64);
        assert_ne!(
            identity.response_replay_digest(),
            identity.assertion_replay_digest()
        );
        assert_eq!(identity.replay_expires_at(), REPLAY_EXPIRATION);
        for digest in [
            identity.subject_digest(),
            identity.response_replay_digest(),
            identity.assertion_replay_digest(),
        ] {
            assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
        }

        let debug = format!("{identity:?}");
        assert!(!debug.contains(identity.subject()));
        assert!(!debug.contains("_response-semantic-1"));
        assert!(!debug.contains("_assertion-semantic-1"));
    }

    #[test]
    fn semantic_verifier_rejects_mismatched_protocol_context() {
        let verifier = semantic_verifier();
        let cases = [
            SemanticResponse {
                audience: "https://other.example.com/metadata",
                ..SemanticResponse::default()
            },
            SemanticResponse {
                recipient: "https://other.example.com/acs",
                ..SemanticResponse::default()
            },
            SemanticResponse {
                destination: "https://other.example.com/acs",
                ..SemanticResponse::default()
            },
            SemanticResponse {
                in_response_to: "_other-request",
                ..SemanticResponse::default()
            },
            SemanticResponse {
                not_before: "2000-01-01T00:02:00Z",
                ..SemanticResponse::default()
            },
            SemanticResponse {
                not_on_or_after: "1999-12-31T23:58:00Z",
                ..SemanticResponse::default()
            },
            SemanticResponse {
                name_id_format: saml_rs::constants::name_id_format::TRANSIENT,
                ..SemanticResponse::default()
            },
            SemanticResponse {
                subject: " directory-subject-123 ",
                ..SemanticResponse::default()
            },
        ];

        for input in cases {
            assert_eq!(
                verifier.verify(&signed_semantic_response(input), validation_now()),
                Err(OrganizationSsoSamlSemanticError::Rejected)
            );
        }
    }

    #[test]
    fn semantic_configuration_rejects_untrusted_endpoints_and_request_ids() {
        let invalid_endpoint = OrganizationSsoSamlSemanticVerificationConfig::new(
            IDP_ENTITY_ID.to_owned(),
            "http://idp.example.com/sso".to_owned(),
            SP_ENTITY_ID.to_owned(),
            ACS_URL.to_owned(),
            REQUEST_ID.to_owned(),
            OrganizationSsoSamlNameIdFormat::Persistent,
            vec![TEST_CERTIFICATE.to_owned()],
        );
        assert_eq!(
            invalid_endpoint,
            Err(OrganizationSsoSamlSemanticError::InvalidConfiguration)
        );

        let invalid_request_id = OrganizationSsoSamlSemanticVerificationConfig::new(
            IDP_ENTITY_ID.to_owned(),
            IDP_SSO_URL.to_owned(),
            SP_ENTITY_ID.to_owned(),
            ACS_URL.to_owned(),
            "request id".to_owned(),
            OrganizationSsoSamlNameIdFormat::Persistent,
            vec![TEST_CERTIFICATE.to_owned()],
        );
        assert_eq!(
            invalid_request_id,
            Err(OrganizationSsoSamlSemanticError::InvalidConfiguration)
        );
    }
}
