use std::{
    fmt,
    future::Future,
    pin::Pin,
    time::{SystemTime, UNIX_EPOCH},
};

use af_account::SystemSecretCipher;
use af_db::{
    UserSessionLookupByIdOutcome, UserSessionLookupOutcome, UserSessionRepository,
    UserSessionRepositoryError,
};
use af_domain::{GroupId, UserId};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize, Serializer};
use thiserror::Error;
use zeroize::{Zeroize as _, Zeroizing};

const SESSION_ISSUER: &str = "anyflows";
const SESSION_AUDIENCE: &str = "anyflows-management";
const SESSION_SIGNING_KEY_BYTES: usize = 32;
const MAX_SESSION_TTL_SECS: u64 = 86_400;

/// 一次登录请求的敏感凭据；密码始终位于可清零字符串中。
pub struct LoginCredentials {
    username: String,
    password: Zeroizing<String>,
    totp_code: Option<Zeroizing<String>>,
}

impl LoginCredentials {
    /// 构造精确用户名匹配的登录凭据，不执行 trim 或大小写归一。
    #[must_use]
    pub fn new(username: String, password: String) -> Self {
        Self {
            username,
            password: Zeroizing::new(password),
            totp_code: None,
        }
    }

    /// 附加登录二次验证码；验证码只在本次认证调用期间存活。
    #[must_use]
    pub fn with_totp_code(mut self, totp_code: Option<String>) -> Self {
        self.totp_code = totp_code.map(Zeroizing::new);
        self
    }

    /// 返回待查询的精确用户名。
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    /// 返回仅供认证调用短暂读取的密码字节。
    #[must_use]
    pub fn password(&self) -> &[u8] {
        self.password.as_bytes()
    }

    /// 返回可选的 TOTP 或备份码；调用方不得记录该值。
    #[must_use]
    pub fn totp_code(&self) -> Option<&str> {
        self.totp_code.as_deref().map(String::as_str)
    }
}

impl fmt::Debug for LoginCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LoginCredentials(<redacted>)")
    }
}

/// 管理会话用户角色；数据库中的 0/1 只在仓储边界转换一次。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionRole {
    /// 普通登录用户。
    User,
    /// 管理员登录用户。
    Admin,
}

impl SessionRole {
    pub(crate) fn from_database(value: i16) -> Result<Self, SessionAuthenticationError> {
        match value {
            0 => Ok(Self::User),
            1 => Ok(Self::Admin),
            _ => Err(SessionAuthenticationError::Internal),
        }
    }

    /// 返回数据库中固定使用的角色编码。
    #[must_use]
    pub const fn to_database(self) -> i16 {
        match self {
            Self::User => 0,
            Self::Admin => 1,
        }
    }
}

/// 通过 JWT 回查后的当前会话主体。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionPrincipal {
    user_id: UserId,
    role: SessionRole,
}

impl SessionPrincipal {
    /// 使用已校验的用户标识和角色构造主体。
    #[must_use]
    pub const fn new(user_id: UserId, role: SessionRole) -> Self {
        Self { user_id, role }
    }

    /// 返回稳定用户标识。
    #[must_use]
    pub const fn user_id(self) -> UserId {
        self.user_id
    }

    /// 返回当前数据库回查得到的角色。
    #[must_use]
    pub const fn role(self) -> SessionRole {
        self.role
    }
}

/// 只在准备 HTTP 响应时读取的 JWT 字符串。
pub struct SessionToken(Zeroizing<String>);

impl SessionToken {
    /// 从已经完成签名校验或即将交给响应层的 JWT 文本构造令牌。
    #[must_use]
    pub fn from_string(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    fn new(value: String) -> Self {
        Self::from_string(value)
    }

    /// 返回待序列化到登录响应的 JWT；调用方不得记录该值。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SessionToken(<redacted>)")
    }
}

impl Serialize for SessionToken {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

/// 登录成功后一次性返回的会话材料。
#[derive(Debug)]
pub struct IssuedSession {
    token: SessionToken,
    principal: SessionPrincipal,
    expires_at: u64,
}

impl IssuedSession {
    /// 组合已完成签名的令牌和当前主体，供对象安全认证适配器返回。
    #[must_use]
    pub fn from_parts(token: SessionToken, principal: SessionPrincipal, expires_at: u64) -> Self {
        Self {
            token,
            principal,
            expires_at,
        }
    }

    /// 返回 JWT 字符串，仅供 HTTP 响应序列化使用。
    #[must_use]
    pub fn token(&self) -> &SessionToken {
        &self.token
    }

    /// 消费签发结果并把 JWT 所有权交给响应 DTO。
    #[must_use]
    pub fn into_token(self) -> SessionToken {
        self.token
    }

    /// 返回登录时的主体。
    #[must_use]
    pub const fn principal(&self) -> SessionPrincipal {
        self.principal
    }

    /// 返回 Unix 秒级过期时间。
    #[must_use]
    pub const fn expires_at(&self) -> u64 {
        self.expires_at
    }
}

/// 受保护管理请求当前有效的会话。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionAuthentication {
    principal: SessionPrincipal,
    group_id: GroupId,
    expires_at: u64,
}

impl SessionAuthentication {
    /// 组合已回查的主体和令牌过期时间。
    #[must_use]
    pub const fn new(principal: SessionPrincipal, group_id: GroupId, expires_at: u64) -> Self {
        Self {
            principal,
            group_id,
            expires_at,
        }
    }

    /// 返回请求期间使用的当前主体。
    #[must_use]
    pub const fn principal(self) -> SessionPrincipal {
        self.principal
    }

    /// 返回本次数据库回查得到的用户默认分组。
    #[must_use]
    pub const fn group_id(self) -> GroupId {
        self.group_id
    }

    /// 返回 JWT 的 Unix 秒级过期时间。
    #[must_use]
    pub const fn expires_at(self) -> u64 {
        self.expires_at
    }
}

/// HTTP 层可安全映射的管理会话失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SessionAuthenticationError {
    /// 用户名不存在、密码错误、禁用或软删除。
    #[error("登录凭据无效")]
    InvalidCredentials,
    /// 用户名密码已通过，但账户要求第二因素。
    #[error("需要二次验证")]
    TwoFactorRequired,
    /// TOTP 或备份码无效，不能继续签发会话。
    #[error("二次验证失败")]
    TwoFactorInvalid,
    /// JWT 缺失、篡改、过期或对应用户已失效。
    #[error("管理会话无效")]
    InvalidSession,
    /// 查询失败或持久化状态违反不变量。
    #[error("管理会话内部失败")]
    Internal,
}

/// 会话认证调用的对象安全 Future。
pub type SessionAuthenticationFuture<'a> = Pin<
    Box<dyn Future<Output = Result<SessionAuthentication, SessionAuthenticationError>> + Send + 'a>,
>;

/// 登录调用的对象安全 Future。
pub type SessionLoginFuture<'a> =
    Pin<Box<dyn Future<Output = Result<IssuedSession, SessionAuthenticationError>> + Send + 'a>>;

/// 管理 API 登录和 JWT 回查端口。
pub trait SessionAuthenticator: Send + Sync {
    /// 校验用户名密码并签发新会话。
    fn login<'a>(&'a self, credentials: &'a LoginCredentials) -> SessionLoginFuture<'a>;

    /// 为已经通过其他认证方式解析出的有效用户签发统一会话。
    fn issue_for_user(&self, _user_id: UserId) -> SessionLoginFuture<'_> {
        Box::pin(async { Err(SessionAuthenticationError::Internal) })
    }

    /// 校验 JWT 并回查当前用户状态。
    fn authenticate<'a>(&'a self, token: &'a str) -> SessionAuthenticationFuture<'a>;
}

/// JWT 启动配置错误；密钥格式错误不得进入运行期。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SessionAuthenticatorConfigError {
    /// 未配置签名密钥，无法安全启动管理 API。
    #[error("管理会话签名密钥缺失")]
    MissingSigningKey,
    /// 签名密钥不是 32 字节 Base64URL 无填充文本。
    #[error("管理会话签名密钥强度不足或格式无效")]
    InvalidSigningKey,
    /// 会话有效期超出硬边界。
    #[error("管理会话有效期超出允许范围")]
    InvalidTtl,
}

/// 使用用户会话仓储和 HS256 签名密钥的生产认证实现。
pub struct DatabaseSessionAuthenticator {
    repository: UserSessionRepository,
    codec: SessionCodec,
    two_factor_cipher: Option<SystemSecretCipher>,
}

impl DatabaseSessionAuthenticator {
    /// 校验密钥和 TTL 后构造生产会话认证器。
    pub fn new(
        repository: UserSessionRepository,
        signing_key: Option<&str>,
        ttl_secs: u64,
    ) -> Result<Self, SessionAuthenticatorConfigError> {
        let signing_key = signing_key.ok_or(SessionAuthenticatorConfigError::MissingSigningKey)?;
        let codec = SessionCodec::new(signing_key, ttl_secs)?;
        Ok(Self {
            repository,
            codec,
            two_factor_cipher: None,
        })
    }

    /// 注入已通过启动期强度校验的 TOTP 加密器。
    #[must_use]
    pub fn with_two_factor_cipher(mut self, cipher: SystemSecretCipher) -> Self {
        self.two_factor_cipher = Some(cipher);
        self
    }
}

impl SessionAuthenticator for DatabaseSessionAuthenticator {
    fn login<'a>(&'a self, credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async move {
            if credentials.username().is_empty()
                || credentials.username().len() > 64
                || credentials.password().len() > 4_096
            {
                return Err(SessionAuthenticationError::InvalidCredentials);
            }
            match self
                .repository
                .login(credentials.username(), credentials.password())
                .await
                .map_err(map_repository_error)?
            {
                UserSessionLookupOutcome::Authenticated {
                    user_id,
                    role,
                    session_version,
                    totp_secret,
                } => {
                    if let Some(totp_secret) = totp_secret {
                        let Some(cipher) = self.two_factor_cipher.as_ref() else {
                            return Err(SessionAuthenticationError::Internal);
                        };
                        let code = credentials
                            .totp_code()
                            .ok_or(SessionAuthenticationError::TwoFactorRequired)?;
                        match crate::two_factor::verify_login_code(
                            cipher,
                            user_id,
                            &totp_secret,
                            code,
                            current_timestamp().ok_or(SessionAuthenticationError::Internal)?,
                        ) {
                            Ok(crate::two_factor::LoginFactorResult::Totp) => {}
                            Ok(crate::two_factor::LoginFactorResult::Backup { replacement }) => {
                                if !self
                                    .repository
                                    .consume_totp_backup_code(user_id, totp_secret, replacement)
                                    .await
                                    .map_err(map_repository_error)?
                                {
                                    return Err(SessionAuthenticationError::TwoFactorInvalid);
                                }
                            }
                            Err(crate::two_factor::TwoFactorError::InvalidCode) => {
                                return Err(SessionAuthenticationError::TwoFactorInvalid);
                            }
                            Err(_) => return Err(SessionAuthenticationError::Internal),
                        }
                    }
                    let principal =
                        SessionPrincipal::new(user_id, SessionRole::from_database(role)?);
                    self.codec.issue(principal, session_version)
                }
                UserSessionLookupOutcome::Rejected => {
                    Err(SessionAuthenticationError::InvalidCredentials)
                }
            }
        })
    }

    fn authenticate<'a>(&'a self, token: &'a str) -> SessionAuthenticationFuture<'a> {
        Box::pin(async move {
            let claims = self.codec.decode(token)?;
            let user_id = UserId::new(
                claims
                    .sub
                    .parse::<i64>()
                    .map_err(|_| SessionAuthenticationError::InvalidSession)?,
            )
            .map_err(|_| SessionAuthenticationError::InvalidSession)?;
            match self
                .repository
                .lookup_by_id(user_id)
                .await
                .map_err(map_repository_error)?
            {
                UserSessionLookupByIdOutcome::Authenticated {
                    user_id,
                    role,
                    group_id,
                    session_version,
                    ..
                } if session_version == claims.session_version => Ok(SessionAuthentication::new(
                    SessionPrincipal::new(user_id, SessionRole::from_database(role)?),
                    group_id,
                    claims.exp,
                )),
                UserSessionLookupByIdOutcome::Authenticated { .. } => {
                    Err(SessionAuthenticationError::InvalidSession)
                }
                UserSessionLookupByIdOutcome::Rejected => {
                    Err(SessionAuthenticationError::InvalidSession)
                }
            }
        })
    }

    fn issue_for_user(&self, user_id: UserId) -> SessionLoginFuture<'_> {
        Box::pin(async move {
            match self
                .repository
                .lookup_by_id(user_id)
                .await
                .map_err(map_repository_error)?
            {
                UserSessionLookupByIdOutcome::Authenticated {
                    user_id,
                    role,
                    session_version,
                    totp_enabled,
                    ..
                } => {
                    if totp_enabled {
                        return Err(SessionAuthenticationError::TwoFactorRequired);
                    }
                    let principal =
                        SessionPrincipal::new(user_id, SessionRole::from_database(role)?);
                    self.codec.issue(principal, session_version)
                }
                UserSessionLookupByIdOutcome::Rejected => {
                    Err(SessionAuthenticationError::InvalidCredentials)
                }
            }
        })
    }
}

impl fmt::Debug for DatabaseSessionAuthenticator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseSessionAuthenticator(<redacted>)")
    }
}

fn map_repository_error(error: UserSessionRepositoryError) -> SessionAuthenticationError {
    let _ = error;
    SessionAuthenticationError::Internal
}

struct SessionCodec {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    validation: Validation,
    ttl_secs: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SessionClaims {
    sub: String,
    iss: String,
    aud: String,
    exp: u64,
    iat: u64,
    session_version: i64,
}

impl SessionCodec {
    fn new(signing_key: &str, ttl_secs: u64) -> Result<Self, SessionAuthenticatorConfigError> {
        if !(1..=MAX_SESSION_TTL_SECS).contains(&ttl_secs) {
            return Err(SessionAuthenticatorConfigError::InvalidTtl);
        }
        let mut decoded = [0_u8; SESSION_SIGNING_KEY_BYTES];
        let length = URL_SAFE_NO_PAD
            .decode_slice(signing_key, &mut decoded)
            .map_err(|_| SessionAuthenticatorConfigError::InvalidSigningKey)?;
        if length != SESSION_SIGNING_KEY_BYTES {
            return Err(SessionAuthenticatorConfigError::InvalidSigningKey);
        }
        let encoding_key = EncodingKey::from_secret(&decoded);
        let decoding_key = DecodingKey::from_secret(&decoded);
        decoded.zeroize();
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 0;
        validation.set_issuer(&[SESSION_ISSUER]);
        validation.set_audience(&[SESSION_AUDIENCE]);
        Ok(Self {
            encoding_key,
            decoding_key,
            validation,
            ttl_secs,
        })
    }

    fn issue(
        &self,
        principal: SessionPrincipal,
        session_version: i64,
    ) -> Result<IssuedSession, SessionAuthenticationError> {
        if session_version < 1 {
            return Err(SessionAuthenticationError::Internal);
        }
        let issued_at = current_timestamp().ok_or(SessionAuthenticationError::Internal)?;
        let expires_at = issued_at
            .checked_add(self.ttl_secs)
            .ok_or(SessionAuthenticationError::Internal)?;
        let claims = SessionClaims {
            sub: principal.user_id().get().to_string(),
            iss: SESSION_ISSUER.to_owned(),
            aud: SESSION_AUDIENCE.to_owned(),
            exp: expires_at,
            iat: issued_at,
            session_version,
        };
        let token = encode(&Header::new(Algorithm::HS256), &claims, &self.encoding_key)
            .map_err(|_| SessionAuthenticationError::Internal)?;
        Ok(IssuedSession {
            token: SessionToken::new(token),
            principal,
            expires_at,
        })
    }

    fn decode(&self, token: &str) -> Result<SessionClaims, SessionAuthenticationError> {
        if token.is_empty() || token.len() > 4_096 || token.chars().any(char::is_whitespace) {
            return Err(SessionAuthenticationError::InvalidSession);
        }
        let decoded = decode::<SessionClaims>(token, &self.decoding_key, &self.validation)
            .map_err(|_| SessionAuthenticationError::InvalidSession)?;
        let now = current_timestamp().ok_or(SessionAuthenticationError::Internal)?;
        if decoded.claims.iss != SESSION_ISSUER
            || decoded.claims.aud != SESSION_AUDIENCE
            || decoded.claims.iat == 0
            || decoded.claims.exp <= decoded.claims.iat
            || decoded.claims.iat > now.saturating_add(60)
            || decoded.claims.exp - decoded.claims.iat > self.ttl_secs
            || decoded.claims.session_version < 1
        {
            return Err(SessionAuthenticationError::InvalidSession);
        }
        Ok(decoded.claims)
    }
}

fn current_timestamp() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signing_key() -> String {
        URL_SAFE_NO_PAD.encode([0x42; SESSION_SIGNING_KEY_BYTES])
    }

    #[test]
    fn hs256_session_round_trip_and_sensitive_debug_are_stable() {
        let key = signing_key();
        let codec = SessionCodec::new(&key, 3_600).unwrap();
        let principal = SessionPrincipal::new(UserId::new(7).unwrap(), SessionRole::Admin);
        let issued = codec.issue(principal, 1).unwrap();
        let claims = codec.decode(issued.token().as_str()).unwrap();

        assert_eq!(claims.sub, "7");
        assert_eq!(issued.principal(), principal);
        assert_eq!(issued.expires_at() - claims.iat, 3_600);
        assert_eq!(format!("{:?}", issued.token()), "SessionToken(<redacted>)");
        let credentials = LoginCredentials::new("admin".to_owned(), "secret-password".to_owned());
        assert_eq!(format!("{credentials:?}"), "LoginCredentials(<redacted>)");
        assert!(!format!("{issued:?}").contains(issued.token().as_str()));
        assert!(!format!("{issued:?}").contains(&key));
        assert!(!format!("{credentials:?}").contains("secret-password"));
    }

    #[test]
    fn tampered_expired_and_wrong_algorithm_tokens_are_rejected() {
        let key = signing_key();
        let codec = SessionCodec::new(&key, 3_600).unwrap();
        let principal = SessionPrincipal::new(UserId::new(7).unwrap(), SessionRole::User);
        let issued = codec.issue(principal, 1).unwrap();
        let mut tampered = issued.token().as_str().to_owned();
        let replacement = if tampered.ends_with('a') { 'b' } else { 'a' };
        tampered.pop();
        tampered.push(replacement);
        assert_eq!(
            codec.decode(&tampered).unwrap_err(),
            SessionAuthenticationError::InvalidSession
        );

        let now = current_timestamp().unwrap();
        let expired = SessionClaims {
            sub: "7".to_owned(),
            iss: SESSION_ISSUER.to_owned(),
            aud: SESSION_AUDIENCE.to_owned(),
            exp: now - 1,
            iat: now - 100,
            session_version: 1,
        };
        let secret = [0x42; SESSION_SIGNING_KEY_BYTES];
        let expired_token = encode(
            &Header::new(Algorithm::HS256),
            &expired,
            &EncodingKey::from_secret(&secret),
        )
        .unwrap();
        assert_eq!(
            codec.decode(&expired_token).unwrap_err(),
            SessionAuthenticationError::InvalidSession
        );

        let wrong_algorithm = encode(
            &Header::new(Algorithm::HS384),
            &SessionClaims {
                exp: now + 60,
                iat: now,
                ..expired
            },
            &EncodingKey::from_secret(&secret),
        )
        .unwrap();
        assert_eq!(
            codec.decode(&wrong_algorithm).unwrap_err(),
            SessionAuthenticationError::InvalidSession
        );
    }

    #[test]
    fn invalid_key_and_ttl_are_rejected_without_echoing_key() {
        for key in ["short", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="] {
            let error = match SessionCodec::new(key, 3_600) {
                Ok(_) => panic!("无效会话密钥不应通过校验"),
                Err(error) => error,
            };
            assert_eq!(error, SessionAuthenticatorConfigError::InvalidSigningKey);
            assert!(!format!("{error:?}\n{error}").contains(key));
        }
        assert!(matches!(
            SessionCodec::new(&signing_key(), 0),
            Err(SessionAuthenticatorConfigError::InvalidTtl)
        ));
        assert!(matches!(
            SessionCodec::new(&signing_key(), MAX_SESSION_TTL_SECS + 1),
            Err(SessionAuthenticatorConfigError::InvalidTtl)
        ));
    }
}
