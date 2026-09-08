use std::fmt;

use getrandom::fill;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{OrganizationSsoDomain, OrganizationSsoDomainStatus};

/// DNS 认领使用的固定 TXT 查询标签，避免把证明放在可被客户端改写的请求参数中。
pub const ORGANIZATION_SSO_DNS_PROOF_LABEL: &str = "_anyflows-sso";
/// 证明令牌的随机字节数；持久化层只保存域名隔离后的摘要。
const ORGANIZATION_SSO_PROOF_TOKEN_BYTES: usize = 32;
const ORGANIZATION_SSO_PROOF_TOKEN_HEX_BYTES: usize = ORGANIZATION_SSO_PROOF_TOKEN_BYTES * 2;
const ORGANIZATION_SSO_PROOF_DIGEST_CONTEXT: &[u8] = b"anyflows-organization-sso-domain-proof-v1";
/// 证明令牌的最短有效期，避免短时间 DNS 缓存导致认领刚完成即失效。
pub const MIN_ORGANIZATION_SSO_PROOF_TTL_SECONDS: i64 = 15 * 60;
/// 证明令牌的最长有效期，令牌轮换不能被无限期推迟。
pub const MAX_ORGANIZATION_SSO_PROOF_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;
/// 默认的持续复验间隔；实际任务仍需由受控调度器执行。
pub const DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS: i64 = 24 * 60 * 60;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 当前内置后缀分类，后续可由受控配置替换为版本化公共后缀表。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationSsoDomainSuffixKind {
    /// 域名可以作为企业认领边界。
    Registrable,
    /// 域名本身是公共后缀，不能代表一个企业。
    PublicSuffix,
    /// 域名本身是平台托管的私有后缀，不能代表一个企业。
    PrivateSuffix,
}

/// 只匹配后缀本身，不把其下的已委派子域误判为公共后缀。
pub fn classify_organization_sso_domain_suffix(
    domain: &OrganizationSsoDomain,
) -> OrganizationSsoDomainSuffixKind {
    if PRIVATE_SUFFIXES.contains(&domain.as_str()) {
        OrganizationSsoDomainSuffixKind::PrivateSuffix
    } else if PUBLIC_SUFFIXES.contains(&domain.as_str()) {
        OrganizationSsoDomainSuffixKind::PublicSuffix
    } else {
        OrganizationSsoDomainSuffixKind::Registrable
    }
}

// 这些集合覆盖常见公共多标签后缀和托管平台后缀；完整 PSL 接入留给受控 DNS 适配器。
const PUBLIC_SUFFIXES: &[&str] = &[
    "ac.uk", "co.in", "co.jp", "co.kr", "co.nz", "co.uk", "com.au", "com.cn", "com.hk", "com.tw",
    "edu.cn", "gov.cn", "gov.uk", "net.cn", "org.cn", "org.uk", "app", "ai", "cn", "com", "dev",
    "edu", "gov", "int", "io", "me", "mil", "net", "org", "site", "top", "xyz",
];
const PRIVATE_SUFFIXES: &[&str] = &[
    "appspot.com",
    "azurewebsites.net",
    "cloudfront.net",
    "fly.dev",
    "github.io",
    "gitlab.io",
    "herokuapp.com",
    "netlify.app",
    "ngrok.io",
    "onrender.com",
    "pages.dev",
    "vercel.app",
    "workers.dev",
];

/// DNS 证明令牌的构造错误；错误不会携带令牌原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoDomainProofError {
    /// 域名本身是公共后缀。
    #[error("企业 SSO 域名不能是公共后缀")]
    PublicSuffix,
    /// 域名本身是平台托管私有后缀。
    #[error("企业 SSO 域名不能是私有后缀")]
    PrivateSuffix,
    /// 令牌不是固定长度的小写十六进制文本或全零值。
    #[error("企业 SSO 证明令牌格式无效")]
    InvalidToken,
    /// 证明有效期不在允许范围内。
    #[error("企业 SSO 证明令牌有效期无效")]
    InvalidTtl,
    /// 时间计算溢出或时钟早于签发时间。
    #[error("企业 SSO 证明时间无效")]
    InvalidTime,
    /// 操作系统无法提供安全随机数。
    #[error("企业 SSO 证明随机数不可用")]
    EntropyUnavailable,
}

/// 只在内存中保留的高熵 DNS 证明令牌。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationSsoDomainProofToken(String);

impl OrganizationSsoDomainProofToken {
    /// 生成 256 位随机令牌，并以固定小写十六进制表示。
    pub fn generate() -> Result<Self, OrganizationSsoDomainProofError> {
        let mut bytes = [0_u8; ORGANIZATION_SSO_PROOF_TOKEN_BYTES];
        fill(&mut bytes).map_err(|_| OrganizationSsoDomainProofError::EntropyUnavailable)?;
        let mut token = String::with_capacity(ORGANIZATION_SSO_PROOF_TOKEN_HEX_BYTES);
        for byte in bytes {
            token.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            token.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        Self::try_from(token)
    }

    /// 从接口或测试输入恢复令牌；生产接口不应把它重新写入数据库。
    pub fn new(value: impl Into<String>) -> Result<Self, OrganizationSsoDomainProofError> {
        let value = value.into();
        if value.len() != ORGANIZATION_SSO_PROOF_TOKEN_HEX_BYTES
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || value.bytes().all(|byte| byte == b'0')
        {
            return Err(OrganizationSsoDomainProofError::InvalidToken);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for OrganizationSsoDomainProofToken {
    type Error = OrganizationSsoDomainProofError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Debug for OrganizationSsoDomainProofToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationSsoDomainProofToken(<redacted>)")
    }
}

/// 一次 DNS 认领尝试的内存证明；持久化层只应保存 `digest` 和时间。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationSsoDomainProof {
    domain: OrganizationSsoDomain,
    token: OrganizationSsoDomainProofToken,
    issued_at: i64,
    expires_at: i64,
}

impl OrganizationSsoDomainProof {
    /// 为可认领域名签发新的高熵证明令牌。
    pub fn issue(
        domain: OrganizationSsoDomain,
        issued_at: i64,
        ttl_seconds: i64,
    ) -> Result<Self, OrganizationSsoDomainProofError> {
        let token = OrganizationSsoDomainProofToken::generate()?;
        Self::from_token(domain, token, issued_at, ttl_seconds)
    }

    /// 使用已生成令牌构造证明，便于仓储和确定性回归测试复用同一校验。
    pub fn from_token(
        domain: OrganizationSsoDomain,
        token: OrganizationSsoDomainProofToken,
        issued_at: i64,
        ttl_seconds: i64,
    ) -> Result<Self, OrganizationSsoDomainProofError> {
        if !matches!(
            classify_organization_sso_domain_suffix(&domain),
            OrganizationSsoDomainSuffixKind::Registrable
        ) || !(MIN_ORGANIZATION_SSO_PROOF_TTL_SECONDS..=MAX_ORGANIZATION_SSO_PROOF_TTL_SECONDS)
            .contains(&ttl_seconds)
        {
            return Err(match classify_organization_sso_domain_suffix(&domain) {
                OrganizationSsoDomainSuffixKind::PublicSuffix => {
                    OrganizationSsoDomainProofError::PublicSuffix
                }
                OrganizationSsoDomainSuffixKind::PrivateSuffix => {
                    OrganizationSsoDomainProofError::PrivateSuffix
                }
                OrganizationSsoDomainSuffixKind::Registrable => {
                    OrganizationSsoDomainProofError::InvalidTtl
                }
            });
        }
        let expires_at = issued_at
            .checked_add(ttl_seconds)
            .ok_or(OrganizationSsoDomainProofError::InvalidTime)?;
        Ok(Self {
            domain,
            token,
            issued_at,
            expires_at,
        })
    }

    #[must_use]
    pub fn domain(&self) -> &OrganizationSsoDomain {
        &self.domain
    }

    #[must_use]
    pub fn token(&self) -> &OrganizationSsoDomainProofToken {
        &self.token
    }

    /// 返回固定 TXT 查询名；客户端不能自定义查询名或域名映射。
    #[must_use]
    pub fn txt_record_name(&self) -> String {
        format!(
            "{ORGANIZATION_SSO_DNS_PROOF_LABEL}.{}",
            self.domain.as_str()
        )
    }

    #[must_use]
    pub const fn issued_at(&self) -> i64 {
        self.issued_at
    }

    #[must_use]
    pub const fn expires_at(&self) -> i64 {
        self.expires_at
    }

    #[must_use]
    pub fn is_expired(&self, now: i64) -> bool {
        now >= self.expires_at
    }

    /// 按域名和固定上下文计算隔离摘要，防止跨用途复用同一令牌摘要。
    #[must_use]
    pub fn digest(&self) -> String {
        Self::digest_for_token(&self.domain, &self.token)
    }

    /// 按固定用途上下文计算摘要；令牌只在调用栈内短暂存在，不进入持久化层。
    #[must_use]
    pub fn digest_for_token(
        domain: &OrganizationSsoDomain,
        token: &OrganizationSsoDomainProofToken,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(ORGANIZATION_SSO_PROOF_DIGEST_CONTEXT);
        hasher.update([0]);
        hasher.update(domain.as_str().as_bytes());
        hasher.update([0]);
        hasher.update(token.as_str().as_bytes());
        hex_digest(hasher.finalize().into())
    }
}

impl fmt::Debug for OrganizationSsoDomainProof {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoDomainProof")
            .field("domain", &self.domain)
            .field("token", &"<redacted>")
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// DNS 适配器传入的规范 TXT RDATA；适配器必须在边界去除协议层引号和分片。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationSsoDnsTxtAnswers(Vec<String>);

impl OrganizationSsoDnsTxtAnswers {
    #[must_use]
    pub fn new(answers: Vec<String>) -> Self {
        Self(answers)
    }

    #[must_use]
    fn contains_exact(&self, token: &OrganizationSsoDomainProofToken) -> bool {
        self.0.iter().any(|answer| answer == token.as_str())
    }

    /// 复验时从 TXT 答案恢复短暂 token 并比较域隔离摘要，不依赖一次性证明有效期。
    #[must_use]
    pub fn matches_digest(&self, domain: &OrganizationSsoDomain, expected_digest: &str) -> bool {
        self.0.iter().any(|answer| {
            OrganizationSsoDomainProofToken::new(answer.clone())
                .ok()
                .is_some_and(|token| {
                    OrganizationSsoDomainProof::digest_for_token(domain, &token) == expected_digest
                })
        })
    }
}

/// Verified 域名持续复验成功后的下一次复验时间。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationSsoDnsReverification {
    recheck_at: i64,
}

impl OrganizationSsoDnsReverification {
    /// 复验只比较持久化摘要，不把一次性认领证明的过期时间当作 TXT 失效。
    pub fn verify(
        domain: &OrganizationSsoDomain,
        current_status: OrganizationSsoDomainStatus,
        proof_digest: &str,
        answers: &OrganizationSsoDnsTxtAnswers,
        now: i64,
        recheck_interval_seconds: i64,
    ) -> Result<Self, OrganizationSsoDnsReverificationError> {
        if current_status != OrganizationSsoDomainStatus::Verified {
            return Err(OrganizationSsoDnsReverificationError::InvalidStatus);
        }
        if now < 0
            || !(MIN_ORGANIZATION_SSO_PROOF_TTL_SECONDS..=MAX_ORGANIZATION_SSO_PROOF_TTL_SECONDS)
                .contains(&recheck_interval_seconds)
        {
            return Err(OrganizationSsoDnsReverificationError::InvalidTime);
        }
        if proof_digest.len() != 64
            || !proof_digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(OrganizationSsoDnsReverificationError::InvalidDigest);
        }
        if !answers.matches_digest(domain, proof_digest) {
            return Err(OrganizationSsoDnsReverificationError::ProofMismatch);
        }
        let recheck_at = now
            .checked_add(recheck_interval_seconds)
            .ok_or(OrganizationSsoDnsReverificationError::InvalidTime)?;
        Ok(Self { recheck_at })
    }

    /// 返回成功复验后的下一次检查时刻。
    #[must_use]
    pub const fn recheck_at(self) -> i64 {
        self.recheck_at
    }
}

/// 持续复验失败分类；不携带 DNS 原文或证明令牌。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoDnsReverificationError {
    #[error("企业 SSO 域名状态不可复验")]
    InvalidStatus,
    #[error("企业 SSO 域名证明摘要无效")]
    InvalidDigest,
    #[error("企业 SSO DNS TXT 证明不匹配")]
    ProofMismatch,
    #[error("企业 SSO 域名复验时间无效")]
    InvalidTime,
}

impl fmt::Debug for OrganizationSsoDnsTxtAnswers {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoDnsTxtAnswers")
            .field("answer_count", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// DNS 证明失败分类；不携带 DNS 原文或证明令牌。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoDnsVerificationError {
    /// 被撤销域名不能通过复验恢复。
    #[error("企业 SSO 域名已撤销")]
    Revoked,
    /// 证明令牌已过期，需要轮换后重新认领。
    #[error("企业 SSO 域名证明已过期")]
    ProofExpired,
    /// TXT 查询没有精确匹配的服务端令牌。
    #[error("企业 SSO DNS TXT 证明不匹配")]
    ProofMismatch,
    /// 复验时间不能早于证明签发时间。
    #[error("企业 SSO DNS 复验时间无效")]
    InvalidTime,
    /// 复验时间间隔溢出或不在允许范围内。
    #[error("企业 SSO DNS 复验间隔无效")]
    InvalidRecheckInterval,
}

/// 通过 DNS 认领后可写入仓储的非敏感结果。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationSsoDnsVerification {
    proof_digest: String,
    verified_at: i64,
    recheck_at: i64,
}

impl OrganizationSsoDnsVerification {
    /// 只根据受控解析结果判定，不接受客户端自报的“已验证”标志。
    pub fn verify(
        proof: &OrganizationSsoDomainProof,
        current_status: OrganizationSsoDomainStatus,
        answers: &OrganizationSsoDnsTxtAnswers,
        now: i64,
        recheck_interval_seconds: i64,
    ) -> Result<Self, OrganizationSsoDnsVerificationError> {
        if current_status == OrganizationSsoDomainStatus::Revoked {
            return Err(OrganizationSsoDnsVerificationError::Revoked);
        }
        if now < proof.issued_at() {
            return Err(OrganizationSsoDnsVerificationError::InvalidTime);
        }
        if proof.is_expired(now) {
            return Err(OrganizationSsoDnsVerificationError::ProofExpired);
        }
        if !answers.contains_exact(proof.token()) {
            return Err(OrganizationSsoDnsVerificationError::ProofMismatch);
        }
        if !(MIN_ORGANIZATION_SSO_PROOF_TTL_SECONDS..=MAX_ORGANIZATION_SSO_PROOF_TTL_SECONDS)
            .contains(&recheck_interval_seconds)
        {
            return Err(OrganizationSsoDnsVerificationError::InvalidRecheckInterval);
        }
        let recheck_at = now
            .checked_add(recheck_interval_seconds)
            .ok_or(OrganizationSsoDnsVerificationError::InvalidRecheckInterval)?;
        Ok(Self {
            proof_digest: proof.digest(),
            verified_at: now,
            recheck_at,
        })
    }

    #[must_use]
    pub fn proof_digest(&self) -> &str {
        &self.proof_digest
    }

    #[must_use]
    pub const fn verified_at(&self) -> i64 {
        self.verified_at
    }

    #[must_use]
    pub const fn recheck_at(&self) -> i64 {
        self.recheck_at
    }
}

impl fmt::Debug for OrganizationSsoDnsVerification {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoDnsVerification")
            .field("proof_digest", &"<redacted>")
            .field("verified_at", &self.verified_at)
            .field("recheck_at", &self.recheck_at)
            .finish()
    }
}

fn hex_digest(bytes: [u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in bytes {
        output.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}
