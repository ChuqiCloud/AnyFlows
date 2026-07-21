use std::{collections::HashSet, fmt};

use af_domain::{IpCidr, TrustedClientIp};
use argon2::{
    Algorithm, MIN_SALT_LEN, Params, Version, password_hash::PasswordHash as ParsedPasswordHash,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use http::header::{HeaderName, HeaderValue};
use sea_orm::{
    ColIdx, DbErr, QueryResult, TryGetError, TryGetable, Value,
    entity::prelude::{DeriveValueType, Json},
    sea_query::{ArrayType, ColumnType, Nullable, StringLen, ValueType, ValueTypeErr},
};
use thiserror::Error;
use url::Url;

const MAX_CHANNEL_BASE_URL_LENGTH: usize = 2_048;
const MAX_ENVELOPE_KEY_ID_LENGTH: usize = 512;
const MAX_ENCRYPTED_PAYLOAD_LENGTH: usize = 1_048_576;
const MAX_PASSWORD_HASH_OUTPUT_LENGTH: usize = 64;
const MAX_PASSWORD_MEMORY_COST_KIB: u32 = 256 * 1_024;
const MAX_PASSWORD_PARALLELISM: u32 = 16;
const MAX_PASSWORD_TIME_COST: u32 = 10;
const MIN_PASSWORD_HASH_OUTPUT_LENGTH: usize = 16;
const XCHACHA20_POLY1305_NONCE_LENGTH: usize = 24;
const POLY1305_TAG_LENGTH: usize = 16;
const ENVELOPE_FIELDS: [&str; 5] = ["version", "algorithm", "key_id", "nonce", "ciphertext"];
const MAX_HEADER_OVERRIDE_COUNT: usize = 64;
const MAX_HEADER_NAME_LENGTH: usize = 200;
const MAX_HEADER_VALUE_LENGTH: usize = 8_192;
const MAX_TOKEN_IP_ALLOWLIST_COUNT: usize = 64;
const MAX_TOKEN_IP_ALLOWLIST_TEXT_BYTES: usize = 4_096;
/// PostgreSQL/MySQL 文本化会在数组分隔符后补空格，按其最坏结构开销保留上限。
pub(crate) const MAX_TOKEN_IP_ALLOWLIST_SERIALIZED_BYTES: i64 =
    (MAX_TOKEN_IP_ALLOWLIST_TEXT_BYTES + MAX_TOKEN_IP_ALLOWLIST_COUNT * 4) as i64;
const FORBIDDEN_OVERRIDE_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "api-key",
    "x-goog-api-key",
    "x-auth-token",
    "x-access-token",
    "x-client-secret",
    "x-amz-security-token",
    "cookie",
    "host",
    "content-length",
    "connection",
    "proxy-connection",
    "keep-alive",
    "transfer-encoding",
    "upgrade",
    "te",
    "trailer",
];

/// 敏感持久化值的结构校验错误。
#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum SensitiveValueError {
    /// 密文封套不是对象或缺少必需字段。
    #[error("密文封套字段 {field} 无效")]
    InvalidEnvelope { field: &'static str },
    /// 密码哈希不是 Argon2id PHC 字符串。
    #[error("密码哈希必须是规范化的 Argon2id PHC 字符串")]
    InvalidPasswordHash,
    /// 令牌哈希不是规范化的 SHA-256 十六进制编码。
    #[error("令牌哈希必须是 64 位小写十六进制字符串")]
    InvalidTokenHash,
    /// 令牌 IP 白名单不是受限的非空 IP/CIDR 字符串数组。
    #[error("令牌 IP 白名单格式无效")]
    InvalidTokenIpAllowlist,
    /// 渠道基础地址无效、使用了不支持的协议或携带敏感 URL 组件。
    #[error("渠道基础地址必须是无凭据、查询串和片段的 HTTP(S) URL")]
    InvalidChannelBaseUrl,
    /// 请求头覆盖不是 JSON 对象。
    #[error("请求头覆盖必须是 JSON 对象")]
    InvalidHeaderOverrides,
    /// 请求头名称不符合 HTTP 语法。
    #[error("请求头覆盖包含无效名称")]
    InvalidHeaderName,
    /// 请求头值不是字符串或不符合 HTTP 语法。
    #[error("请求头覆盖包含无效值")]
    InvalidHeaderValue,
    /// 同一请求头使用了不同大小写的重复名称。
    #[error("请求头覆盖包含重复字段")]
    DuplicateHeader,
    /// 请求头覆盖试图保存认证信息。
    #[error("请求头覆盖包含禁止设置的字段")]
    ForbiddenHeader,
}

/// 禁止通过 `Debug` 暴露的敏感文本。
#[derive(Clone, PartialEq, Eq, DeriveValueType)]
pub(crate) struct SensitiveString(String);

impl From<String> for SensitiveString {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for SensitiveString {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl fmt::Debug for SensitiveString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// 经过格式校验且禁止日志输出的 Argon2id PHC 密码哈希。
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PasswordHash(String);

impl PasswordHash {
    /// 解析 Argon2id PHC；参数强度由创建密码哈希的账户服务负责。
    pub(crate) fn parse(value: &str) -> Result<Self, SensitiveValueError> {
        if value.len() > 255 {
            return Err(SensitiveValueError::InvalidPasswordHash);
        }
        let parsed =
            ParsedPasswordHash::new(value).map_err(|_| SensitiveValueError::InvalidPasswordHash)?;
        if Algorithm::try_from(parsed.algorithm).ok() != Some(Algorithm::Argon2id)
            || parsed
                .version
                .and_then(|version| Version::try_from(version).ok())
                != Some(Version::V0x13)
        {
            return Err(SensitiveValueError::InvalidPasswordHash);
        }

        let mut memory_cost = None;
        let mut time_cost = None;
        let mut parallelism = None;
        for (name, value) in parsed.params.iter() {
            let target = match name.as_str() {
                "m" => &mut memory_cost,
                "t" => &mut time_cost,
                "p" => &mut parallelism,
                _ => return Err(SensitiveValueError::InvalidPasswordHash),
            };
            let cost = value
                .decimal()
                .map_err(|_| SensitiveValueError::InvalidPasswordHash)?;
            if target.replace(cost).is_some() {
                return Err(SensitiveValueError::InvalidPasswordHash);
            }
        }
        let (Some(memory_cost), Some(time_cost), Some(parallelism)) =
            (memory_cost, time_cost, parallelism)
        else {
            return Err(SensitiveValueError::InvalidPasswordHash);
        };
        if memory_cost > MAX_PASSWORD_MEMORY_COST_KIB
            || time_cost > MAX_PASSWORD_TIME_COST
            || parallelism > MAX_PASSWORD_PARALLELISM
        {
            return Err(SensitiveValueError::InvalidPasswordHash);
        }

        let Some(output_length) = parsed.hash.as_ref().map(|hash| hash.len()) else {
            return Err(SensitiveValueError::InvalidPasswordHash);
        };
        if !(MIN_PASSWORD_HASH_OUTPUT_LENGTH..=MAX_PASSWORD_HASH_OUTPUT_LENGTH)
            .contains(&output_length)
            || Params::new(memory_cost, time_cost, parallelism, Some(output_length)).is_err()
        {
            return Err(SensitiveValueError::InvalidPasswordHash);
        }

        let Some(salt) = parsed.salt else {
            return Err(SensitiveValueError::InvalidPasswordHash);
        };
        let mut decoded_salt = [0_u8; 64];
        if salt
            .decode_b64(&mut decoded_salt)
            .map_or(true, |salt| salt.len() < MIN_SALT_LEN)
        {
            return Err(SensitiveValueError::InvalidPasswordHash);
        }
        Ok(Self(value.to_owned()))
    }
}

impl fmt::Debug for PasswordHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// 经过格式校验且禁止日志输出的 SHA-256 令牌哈希。
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct TokenHash(String);

impl TokenHash {
    /// 解析已经规范化为小写的 SHA-256 十六进制令牌哈希。
    pub(crate) fn parse(value: &str) -> Result<Self, SensitiveValueError> {
        if value.len() == 64
            && value
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        {
            return Ok(Self(value.to_owned()));
        }
        Err(SensitiveValueError::InvalidTokenHash)
    }
}

impl fmt::Debug for TokenHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// 已完整校验且禁止日志输出的令牌 IP/CIDR 白名单。
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct TokenIpAllowlist {
    raw: Json,
    rules: Box<[IpCidr]>,
}

impl TokenIpAllowlist {
    /// 校验非空数组、数量、总文本预算以及每一条 IP/CIDR。
    pub(crate) fn validate(value: Json) -> Result<Self, SensitiveValueError> {
        let Json::Array(entries) = &value else {
            return Err(SensitiveValueError::InvalidTokenIpAllowlist);
        };
        if entries.is_empty() || entries.len() > MAX_TOKEN_IP_ALLOWLIST_COUNT {
            return Err(SensitiveValueError::InvalidTokenIpAllowlist);
        }

        let mut total_text_bytes = 0_usize;
        let mut rules = Vec::with_capacity(entries.len());
        for entry in entries {
            let Json::String(entry) = entry else {
                return Err(SensitiveValueError::InvalidTokenIpAllowlist);
            };
            total_text_bytes = total_text_bytes
                .checked_add(entry.len())
                .filter(|total| *total <= MAX_TOKEN_IP_ALLOWLIST_TEXT_BYTES)
                .ok_or(SensitiveValueError::InvalidTokenIpAllowlist)?;
            let rule = entry
                .parse::<IpCidr>()
                .map_err(|_| SensitiveValueError::InvalidTokenIpAllowlist)?;
            rules.push(rule);
        }

        Ok(Self {
            raw: value,
            rules: rules.into_boxed_slice(),
        })
    }

    /// 判断可信客户端 IP 是否命中任意一条完整校验后的规则。
    pub(crate) fn allows(&self, client_ip: TrustedClientIp) -> bool {
        self.rules.iter().any(|rule| rule.contains(client_ip))
    }
}

impl fmt::Debug for TokenIpAllowlist {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// 存放版本化密文封套的 JSON；解密后的凭据仍须使用强类型结构。
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct EncryptedJson(Json);

impl EncryptedJson {
    /// 校验并构造版本化密文封套。
    pub(crate) fn from_envelope(value: Json) -> Result<Self, SensitiveValueError> {
        let Json::Object(fields) = &value else {
            return Err(SensitiveValueError::InvalidEnvelope { field: "object" });
        };
        if fields.len() != ENVELOPE_FIELDS.len()
            || !ENVELOPE_FIELDS
                .iter()
                .all(|field| fields.contains_key(*field))
        {
            return Err(SensitiveValueError::InvalidEnvelope { field: "fields" });
        }
        if fields.get("version").and_then(Json::as_u64) != Some(1) {
            return Err(SensitiveValueError::InvalidEnvelope { field: "version" });
        }
        if fields.get("algorithm").and_then(Json::as_str) != Some("xchacha20poly1305") {
            return Err(SensitiveValueError::InvalidEnvelope { field: "algorithm" });
        }
        let Some(key_id) = fields.get("key_id").and_then(Json::as_str) else {
            return Err(SensitiveValueError::InvalidEnvelope { field: "key_id" });
        };
        if key_id.is_empty()
            || key_id.len() > MAX_ENVELOPE_KEY_ID_LENGTH
            || key_id.trim() != key_id
            || key_id.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(SensitiveValueError::InvalidEnvelope { field: "key_id" });
        }

        let nonce = decode_envelope_field(
            fields.get("nonce"),
            "nonce",
            XCHACHA20_POLY1305_NONCE_LENGTH,
        )?;
        if nonce.len() != XCHACHA20_POLY1305_NONCE_LENGTH {
            return Err(SensitiveValueError::InvalidEnvelope { field: "nonce" });
        }
        let ciphertext = decode_envelope_field(
            fields.get("ciphertext"),
            "ciphertext",
            MAX_ENCRYPTED_PAYLOAD_LENGTH,
        )?;
        if ciphertext.len() < POLY1305_TAG_LENGTH {
            return Err(SensitiveValueError::InvalidEnvelope {
                field: "ciphertext",
            });
        }
        Ok(Self(value))
    }
}

/// 解码无填充 Base64URL 封套字段，并在分配前限制编码后长度。
fn decode_envelope_field(
    value: Option<&Json>,
    field: &'static str,
    max_decoded_length: usize,
) -> Result<Vec<u8>, SensitiveValueError> {
    let Some(encoded) = value.and_then(Json::as_str) else {
        return Err(SensitiveValueError::InvalidEnvelope { field });
    };
    let max_encoded_length = max_decoded_length.saturating_mul(4).div_ceil(3);
    if encoded.is_empty() || encoded.len() > max_encoded_length {
        return Err(SensitiveValueError::InvalidEnvelope { field });
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| SensitiveValueError::InvalidEnvelope { field })?;
    if decoded.len() > max_decoded_length {
        return Err(SensitiveValueError::InvalidEnvelope { field });
    }
    Ok(decoded)
}

impl fmt::Debug for EncryptedJson {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// 可能含安全配置的 JSON，禁止通过 `Debug` 输出完整内容。
#[derive(Clone, PartialEq, Eq, DeriveValueType)]
pub(crate) struct SensitiveJson(Json);

impl From<Json> for SensitiveJson {
    fn from(value: Json) -> Self {
        Self(value)
    }
}

impl fmt::Debug for SensitiveJson {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// 禁止 userinfo 且默认脱敏的渠道基础地址。
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ChannelBaseUrl(String);

impl ChannelBaseUrl {
    /// 解析不携带认证信息、查询串或片段的 HTTP(S) 渠道地址。
    pub(crate) fn parse(value: &str) -> Result<Self, SensitiveValueError> {
        if value.len() > MAX_CHANNEL_BASE_URL_LENGTH {
            return Err(SensitiveValueError::InvalidChannelBaseUrl);
        }
        let parsed = Url::parse(value).map_err(|_| SensitiveValueError::InvalidChannelBaseUrl)?;
        let supported_scheme = matches!(parsed.scheme(), "http" | "https");
        let has_credentials = !parsed.username().is_empty() || parsed.password().is_some();
        let has_query_or_fragment = parsed.query().is_some() || parsed.fragment().is_some();
        if !supported_scheme || !parsed.has_host() || has_credentials || has_query_or_fragment {
            return Err(SensitiveValueError::InvalidChannelBaseUrl);
        }
        Ok(Self(parsed.into()))
    }
}

impl fmt::Debug for ChannelBaseUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// 已拒绝认证头且默认脱敏的请求头覆盖配置。
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct HeaderOverrides(Json);

impl HeaderOverrides {
    /// 校验并构造不含认证字段的请求头覆盖配置。
    pub(crate) fn validate(value: Json) -> Result<Self, SensitiveValueError> {
        let Json::Object(headers) = &value else {
            return Err(SensitiveValueError::InvalidHeaderOverrides);
        };
        if headers.len() > MAX_HEADER_OVERRIDE_COUNT {
            return Err(SensitiveValueError::InvalidHeaderOverrides);
        }
        let mut normalized_names = HashSet::with_capacity(headers.len());
        for (raw_name, raw_value) in headers {
            if raw_name.len() > MAX_HEADER_NAME_LENGTH {
                return Err(SensitiveValueError::InvalidHeaderName);
            }
            let name = HeaderName::try_from(raw_name.as_str())
                .map_err(|_| SensitiveValueError::InvalidHeaderName)?;
            let normalized_name = name.as_str();
            if !normalized_names.insert(normalized_name.to_owned()) {
                return Err(SensitiveValueError::DuplicateHeader);
            }
            if FORBIDDEN_OVERRIDE_HEADERS.contains(&normalized_name) {
                return Err(SensitiveValueError::ForbiddenHeader);
            }
            let Json::String(raw_value) = raw_value else {
                return Err(SensitiveValueError::InvalidHeaderValue);
            };
            if raw_value.len() > MAX_HEADER_VALUE_LENGTH {
                return Err(SensitiveValueError::InvalidHeaderValue);
            }
            HeaderValue::try_from(raw_value.as_str())
                .map_err(|_| SensitiveValueError::InvalidHeaderValue)?;
        }
        Ok(Self(value))
    }
}

impl fmt::Debug for HeaderOverrides {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

// DeriveValueType 会直接包装数据库值，校验型类型必须在两条解码路径上重新验证。
macro_rules! impl_validated_string_value {
    ($type:ident, $constructor:ident, $column_type:expr) => {
        impl From<$type> for Value {
            fn from(source: $type) -> Self {
                source.0.into()
            }
        }

        impl TryGetable for $type {
            fn try_get_by<I>(result: &QueryResult, index: I) -> Result<Self, TryGetError>
            where
                I: ColIdx,
            {
                let raw = <String as TryGetable>::try_get_by(result, index)?;
                $type::$constructor(&raw)
                    .map_err(|error| invalid_db_value("String", stringify!($type), error))
            }
        }

        impl ValueType for $type {
            fn try_from(value: Value) -> Result<Self, ValueTypeErr> {
                let raw = <String as ValueType>::try_from(value)?;
                $type::$constructor(&raw).map_err(|_| ValueTypeErr)
            }

            fn type_name() -> String {
                stringify!($type).to_owned()
            }

            fn array_type() -> ArrayType {
                ArrayType::String
            }

            fn column_type() -> ColumnType {
                $column_type
            }
        }

        impl Nullable for $type {
            fn null() -> Value {
                <String as Nullable>::null()
            }
        }
    };
}

macro_rules! impl_validated_json_value {
    ($type:ident, $constructor:ident) => {
        impl From<$type> for Value {
            fn from(source: $type) -> Self {
                source.0.into()
            }
        }

        impl TryGetable for $type {
            fn try_get_by<I>(result: &QueryResult, index: I) -> Result<Self, TryGetError>
            where
                I: ColIdx,
            {
                let raw = <Json as TryGetable>::try_get_by(result, index)?;
                $type::$constructor(raw)
                    .map_err(|error| invalid_db_value("Json", stringify!($type), error))
            }
        }

        impl ValueType for $type {
            fn try_from(value: Value) -> Result<Self, ValueTypeErr> {
                let raw = <Json as ValueType>::try_from(value)?;
                $type::$constructor(raw).map_err(|_| ValueTypeErr)
            }

            fn type_name() -> String {
                stringify!($type).to_owned()
            }

            fn array_type() -> ArrayType {
                ArrayType::Json
            }

            fn column_type() -> ColumnType {
                ColumnType::JsonBinary
            }
        }

        impl Nullable for $type {
            fn null() -> Value {
                <Json as Nullable>::null()
            }
        }
    };
}

/// 将非法持久化值转换为不携带原始敏感内容的类型错误。
fn invalid_db_value(
    from: &'static str,
    into: &'static str,
    source: SensitiveValueError,
) -> TryGetError {
    DbErr::TryIntoErr {
        from,
        into,
        source: Box::new(source),
    }
    .into()
}

impl_validated_string_value!(TokenHash, parse, ColumnType::Char(Some(64)));
impl_validated_string_value!(PasswordHash, parse, ColumnType::String(StringLen::N(255)));
impl_validated_string_value!(ChannelBaseUrl, parse, ColumnType::Text);
impl_validated_json_value!(EncryptedJson, from_envelope);
impl_validated_json_value!(HeaderOverrides, validate);

impl From<TokenIpAllowlist> for Value {
    fn from(source: TokenIpAllowlist) -> Self {
        source.raw.into()
    }
}

impl TryGetable for TokenIpAllowlist {
    fn try_get_by<I>(result: &QueryResult, index: I) -> Result<Self, TryGetError>
    where
        I: ColIdx,
    {
        let raw = <Json as TryGetable>::try_get_by(result, index)?;
        Self::validate(raw)
            .map_err(|error| invalid_db_value("Json", stringify!(TokenIpAllowlist), error))
    }
}

impl ValueType for TokenIpAllowlist {
    fn try_from(value: Value) -> Result<Self, ValueTypeErr> {
        let raw = <Json as ValueType>::try_from(value)?;
        Self::validate(raw).map_err(|_| ValueTypeErr)
    }

    fn type_name() -> String {
        stringify!(TokenIpAllowlist).to_owned()
    }

    fn array_type() -> ArrayType {
        ArrayType::Json
    }

    fn column_type() -> ColumnType {
        ColumnType::JsonBinary
    }
}

impl Nullable for TokenIpAllowlist {
    fn null() -> Value {
        <Json as Nullable>::null()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PASSWORD_HASH: &str =
        "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$MDEyMzQ1Njc4OWFiY2RlZg";

    #[test]
    fn sensitive_values_never_render_payloads() {
        let text = SensitiveString::from("sensitive-token");
        let password_hash = PasswordHash::parse(TEST_PASSWORD_HASH).unwrap();
        let token_hash = TokenHash::parse(&"a".repeat(64)).unwrap();
        let json = EncryptedJson::from_envelope(encrypted_envelope()).unwrap();
        let settings = SensitiveJson::from(Json::String("sensitive-settings".to_owned()));
        let base_url = ChannelBaseUrl::parse("https://api.example.com").unwrap();
        let headers = HeaderOverrides::validate(Json::Object(Default::default())).unwrap();
        let allowlist =
            TokenIpAllowlist::validate(Json::Array(vec![Json::String("192.0.2.0/24".to_owned())]))
                .unwrap();

        for rendered in [
            format!("{text:?}"),
            format!("{password_hash:?}"),
            format!("{token_hash:?}"),
            format!("{json:?}"),
            format!("{settings:?}"),
            format!("{base_url:?}"),
            format!("{headers:?}"),
            format!("{allowlist:?}"),
        ] {
            assert_eq!(rendered, "<redacted>");
        }
    }

    #[test]
    fn password_hashes_follow_argon2id_verifier_limits() {
        assert!(PasswordHash::parse(TEST_PASSWORD_HASH).is_ok());

        for invalid_hash in [
            "$argon2id$v=19$m=19456,t=2,p=1,x=1$c2FsdHNhbHQ$MDEyMzQ1Njc4OWFiY2RlZg",
            "$argon2id$v=19$m=19456,m=8,t=2,p=1$c2FsdHNhbHQ$MDEyMzQ1Njc4OWFiY2RlZg",
            "$argon2id$v=19$m=8,t=1,p=2$c2FsdHNhbHQ$MDEyMzQ1Njc4OWFiY2RlZg",
            "$argon2id$v=19$m=19456,t=2,p=1$YWJjZA$MDEyMzQ1Njc4OWFiY2RlZg",
            "$argon2id$v=19$m=262145,t=2,p=1$c2FsdHNhbHQ$MDEyMzQ1Njc4OWFiY2RlZg",
            "$argon2id$v=19$m=19456,t=11,p=1$c2FsdHNhbHQ$MDEyMzQ1Njc4OWFiY2RlZg",
            "$argon2id$v=19$m=19456,t=2,p=17$c2FsdHNhbHQ$MDEyMzQ1Njc4OWFiY2RlZg",
            "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$MDEyMzQ1Njc4OQ",
        ] {
            assert!(PasswordHash::parse(invalid_hash).is_err());
        }
    }

    #[test]
    fn structured_secrets_reject_unsafe_values() {
        assert!(EncryptedJson::from_envelope(Json::Object(Default::default())).is_err());
        assert!(PasswordHash::parse("plaintext-password").is_err());
        assert!(PasswordHash::parse(&TEST_PASSWORD_HASH.replace("argon2id", "argon2i")).is_err());
        assert!(TokenHash::parse("plaintext-token").is_err());
        assert!(TokenHash::parse(&"A".repeat(64)).is_err());
        for invalid in [
            Json::Null,
            Json::Array(Vec::new()),
            Json::Array(vec![Json::String("invalid-ip".to_owned())]),
        ] {
            assert!(TokenIpAllowlist::validate(invalid).is_err());
        }
        assert!(ChannelBaseUrl::parse("https://user:password@example.com").is_err());
        assert!(ChannelBaseUrl::parse("https://example.com?api_key=secret").is_err());
        assert!(ChannelBaseUrl::parse("https://example.com#secret").is_err());
        assert!(
            ChannelBaseUrl::parse(&format!(
                "https://example.com/{}",
                "a".repeat(MAX_CHANNEL_BASE_URL_LENGTH)
            ))
            .is_err()
        );

        let headers = Json::Object(
            [(
                "Authorization".to_owned(),
                Json::String("secret".to_owned()),
            )]
            .into_iter()
            .collect(),
        );
        assert!(HeaderOverrides::validate(headers).is_err());

        for headers in [
            Json::Object(
                [(" invalid".to_owned(), Json::String("value".to_owned()))]
                    .into_iter()
                    .collect(),
            ),
            Json::Object(
                [("x-trace".to_owned(), Json::from(1))]
                    .into_iter()
                    .collect(),
            ),
            Json::Object(
                [(
                    "x-trace".to_owned(),
                    Json::String("value\r\ninjected: true".to_owned()),
                )]
                .into_iter()
                .collect(),
            ),
            Json::Object(
                [
                    ("X-Trace".to_owned(), Json::String("first".to_owned())),
                    ("x-trace".to_owned(), Json::String("second".to_owned())),
                ]
                .into_iter()
                .collect(),
            ),
            Json::Object(
                [("Host".to_owned(), Json::String("example.com".to_owned()))]
                    .into_iter()
                    .collect(),
            ),
        ] {
            assert!(HeaderOverrides::validate(headers).is_err());
        }

        let too_many_headers = Json::Object(
            (0..=MAX_HEADER_OVERRIDE_COUNT)
                .map(|index| (format!("x-test-{index}"), Json::String("value".to_owned())))
                .collect(),
        );
        assert!(HeaderOverrides::validate(too_many_headers).is_err());

        let oversized_value = Json::Object(
            [(
                "x-test".to_owned(),
                Json::String("a".repeat(MAX_HEADER_VALUE_LENGTH + 1)),
            )]
            .into_iter()
            .collect(),
        );
        assert!(HeaderOverrides::validate(oversized_value).is_err());

        let mut unsupported_algorithm = encrypted_envelope();
        unsupported_algorithm["algorithm"] = Json::String("none".to_owned());
        assert!(EncryptedJson::from_envelope(unsupported_algorithm).is_err());

        let mut plaintext_ciphertext = encrypted_envelope();
        plaintext_ciphertext["ciphertext"] = Json::String("raw-api-key".to_owned());
        assert!(EncryptedJson::from_envelope(plaintext_ciphertext).is_err());

        let Json::Object(mut unexpected_fields) = encrypted_envelope() else {
            unreachable!("测试密文封套必须是对象")
        };
        unexpected_fields.insert("api_key".to_owned(), Json::String("raw-api-key".to_owned()));
        assert!(EncryptedJson::from_envelope(Json::Object(unexpected_fields)).is_err());
    }

    #[test]
    fn database_value_conversion_revalidates_payloads() {
        assert!(<TokenHash as ValueType>::try_from(Value::from("z".repeat(64))).is_err());
        assert!(
            <PasswordHash as ValueType>::try_from(Value::from("raw-password".to_owned())).is_err()
        );
        assert!(
            <ChannelBaseUrl as ValueType>::try_from(Value::from(
                "https://user:database-secret@example.com".to_owned()
            ))
            .is_err()
        );
        assert!(
            <EncryptedJson as ValueType>::try_from(Value::from(Json::Object(Default::default())))
                .is_err()
        );

        let headers = Json::Object(
            [(
                "Authorization".to_owned(),
                Json::String("database-secret".to_owned()),
            )]
            .into_iter()
            .collect(),
        );
        assert!(<HeaderOverrides as ValueType>::try_from(Value::from(headers)).is_err());
    }

    fn encrypted_envelope() -> Json {
        Json::Object(
            [
                ("version".to_owned(), Json::from(1)),
                (
                    "algorithm".to_owned(),
                    Json::String("xchacha20poly1305".to_owned()),
                ),
                ("key_id".to_owned(), Json::String("test-key".to_owned())),
                (
                    "nonce".to_owned(),
                    Json::String(URL_SAFE_NO_PAD.encode([0x42; XCHACHA20_POLY1305_NONCE_LENGTH])),
                ),
                (
                    "ciphertext".to_owned(),
                    Json::String(URL_SAFE_NO_PAD.encode([0xa5; 32])),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
