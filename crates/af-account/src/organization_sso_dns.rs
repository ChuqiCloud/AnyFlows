use std::{collections::HashSet, net::SocketAddr, time::Duration};

use af_domain::{
    OrganizationSsoDnsTxtAnswers, OrganizationSsoDnsVerification,
    OrganizationSsoDnsVerificationError, OrganizationSsoDomainProof, OrganizationSsoDomainStatus,
};
use async_trait::async_trait;
use hickory_resolver::{
    TokioResolver,
    config::{ConnectionConfig, NameServerConfig, ResolveHosts, ResolverConfig, ResolverOpts},
    net::runtime::TokioRuntimeProvider,
    proto::rr::{RData, RecordType},
};
use thiserror::Error;
use tokio::time::timeout;

/// 单次 DNS 证明查询允许使用的最大 nameserver 数量。
pub const MAX_ORGANIZATION_SSO_DNS_NAMESERVERS: usize = 4;
/// 单次 DNS 查询允许返回的 TXT 记录数量上限。
pub const MAX_ORGANIZATION_SSO_DNS_TXT_ANSWERS: usize = 32;
/// 单条 TXT 记录拼接后的最大字节数。
pub const MAX_ORGANIZATION_SSO_DNS_TXT_ANSWER_BYTES: usize = 4096;
/// 所有 TXT 记录拼接后的最大字节数。
pub const MAX_ORGANIZATION_SSO_DNS_TXT_TOTAL_BYTES: usize = 16 * 1024;

/// DNS 解析适配器配置错误；错误信息不携带 nameserver 原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoDnsResolverConfigError {
    #[error("企业 SSO DNS nameserver 配置无效")]
    InvalidNameservers,
    #[error("企业 SSO DNS 查询超时配置无效")]
    InvalidTimeout,
}

/// DNS 解析或 TXT 边界错误；不向上层传播域名、地址或 TXT 原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoDnsResolverError {
    #[error("企业 SSO DNS 查询超时")]
    Timeout,
    #[error("企业 SSO DNS 查询失败")]
    LookupFailed,
    #[error("企业 SSO DNS TXT 答案过多")]
    TooManyAnswers,
    #[error("企业 SSO DNS TXT 答案过大")]
    AnswerTooLarge,
    #[error("企业 SSO DNS TXT 答案编码无效")]
    InvalidAnswerEncoding,
    #[error("企业 SSO DNS 证明校验失败")]
    Verification(OrganizationSsoDnsVerificationError),
}

/// 受控 DNS TXT 解析器；查询名只从领域证明生成，调用方不能自定义。
#[derive(Clone)]
pub struct ControlledOrganizationSsoDnsResolver {
    resolver: TokioResolver,
    timeout: Duration,
}

impl ControlledOrganizationSsoDnsResolver {
    /// 仅使用显式配置的 nameserver，并关闭系统 hosts/search/cache 影响。
    pub fn new(
        nameservers: &[SocketAddr],
        query_timeout: Duration,
    ) -> Result<Self, OrganizationSsoDnsResolverConfigError> {
        if nameservers.is_empty()
            || nameservers.len() > MAX_ORGANIZATION_SSO_DNS_NAMESERVERS
            || nameservers.iter().any(|address| address.port() == 0)
        {
            return Err(OrganizationSsoDnsResolverConfigError::InvalidNameservers);
        }
        if query_timeout.is_zero() || query_timeout > Duration::from_secs(30) {
            return Err(OrganizationSsoDnsResolverConfigError::InvalidTimeout);
        }

        let mut seen = HashSet::with_capacity(nameservers.len());
        let mut servers = Vec::with_capacity(nameservers.len());
        for address in nameservers {
            if !seen.insert(*address) {
                return Err(OrganizationSsoDnsResolverConfigError::InvalidNameservers);
            }
            let mut udp = ConnectionConfig::udp();
            udp.port = address.port();
            let mut tcp = ConnectionConfig::tcp();
            tcp.port = address.port();
            servers.push(NameServerConfig::new(address.ip(), true, vec![udp, tcp]));
        }
        let config = ResolverConfig::from_parts(None, Vec::new(), servers);
        let mut options = ResolverOpts::default();
        options.timeout = query_timeout;
        options.attempts = 1;
        options.num_concurrent_reqs = 1;
        options.cache_size = 0;
        options.use_hosts_file = ResolveHosts::Never;
        options.ndots = 1;
        let resolver = TokioResolver::builder_with_config(config, TokioRuntimeProvider::default())
            .with_options(options)
            .build()
            .map_err(|_| OrganizationSsoDnsResolverConfigError::InvalidNameservers)?;

        Ok(Self {
            resolver,
            timeout: query_timeout,
        })
    }

    /// 查询证明领域生成的固定 TXT 名，并把协议层分片归一化为领域答案。
    pub async fn lookup_txt(
        &self,
        proof: &OrganizationSsoDomainProof,
    ) -> Result<OrganizationSsoDnsTxtAnswers, OrganizationSsoDnsResolverError> {
        self.lookup_txt_name(&proof.txt_record_name()).await
    }

    /// 按领域域名查询固定 `_anyflows-sso` TXT 记录，供后台持续复验使用。
    pub async fn lookup_txt_for_domain(
        &self,
        domain: &af_domain::OrganizationSsoDomain,
    ) -> Result<OrganizationSsoDnsTxtAnswers, OrganizationSsoDnsResolverError> {
        self.lookup_txt_name(&format!(
            "{}.{domain}",
            af_domain::ORGANIZATION_SSO_DNS_PROOF_LABEL,
            domain = domain.as_str()
        ))
        .await
    }

    async fn lookup_txt_name(
        &self,
        record_name: &str,
    ) -> Result<OrganizationSsoDnsTxtAnswers, OrganizationSsoDnsResolverError> {
        let query_name = format!("{}.", record_name);
        let lookup = timeout(
            self.timeout,
            self.resolver.lookup(query_name, RecordType::TXT),
        )
        .await
        .map_err(|_| OrganizationSsoDnsResolverError::Timeout)?
        .map_err(|_| OrganizationSsoDnsResolverError::LookupFailed)?;
        let mut answers = Vec::new();
        let mut total_bytes = 0_usize;
        for record in lookup.answers() {
            let RData::TXT(txt) = &record.data else {
                continue;
            };
            if answers.len() >= MAX_ORGANIZATION_SSO_DNS_TXT_ANSWERS {
                return Err(OrganizationSsoDnsResolverError::TooManyAnswers);
            }
            let mut bytes = Vec::new();
            for fragment in &txt.txt_data {
                if bytes.len().saturating_add(fragment.len())
                    > MAX_ORGANIZATION_SSO_DNS_TXT_ANSWER_BYTES
                {
                    return Err(OrganizationSsoDnsResolverError::AnswerTooLarge);
                }
                bytes.extend_from_slice(fragment);
            }
            total_bytes = total_bytes.saturating_add(bytes.len());
            if total_bytes > MAX_ORGANIZATION_SSO_DNS_TXT_TOTAL_BYTES {
                return Err(OrganizationSsoDnsResolverError::AnswerTooLarge);
            }
            answers.push(
                String::from_utf8(bytes)
                    .map_err(|_| OrganizationSsoDnsResolverError::InvalidAnswerEncoding)?,
            );
        }
        Ok(OrganizationSsoDnsTxtAnswers::new(answers))
    }

    /// 只把受控解析结果交给领域校验，客户端不能注入答案或 Verified 标志。
    pub async fn verify(
        &self,
        proof: &OrganizationSsoDomainProof,
        current_status: OrganizationSsoDomainStatus,
        now: i64,
        recheck_interval_seconds: i64,
    ) -> Result<OrganizationSsoDnsVerification, OrganizationSsoDnsResolverError> {
        let answers = self.lookup_txt(proof).await?;
        OrganizationSsoDnsVerification::verify(
            proof,
            current_status,
            &answers,
            now,
            recheck_interval_seconds,
        )
        .map_err(OrganizationSsoDnsResolverError::Verification)
    }
}

/// 便于业务层注入受控解析器并在测试中替换网络实现。
#[async_trait]
pub trait OrganizationSsoDnsTxtResolver: Send + Sync {
    async fn lookup_txt(
        &self,
        proof: &OrganizationSsoDomainProof,
    ) -> Result<OrganizationSsoDnsTxtAnswers, OrganizationSsoDnsResolverError>;
}

/// 按域名查询固定 TXT 记录，供持续复验任务使用；不接受客户端自定义查询名。
#[async_trait]
pub trait OrganizationSsoDnsDomainResolver: Send + Sync {
    async fn lookup_txt_for_domain(
        &self,
        domain: &af_domain::OrganizationSsoDomain,
    ) -> Result<OrganizationSsoDnsTxtAnswers, OrganizationSsoDnsResolverError>;
}

#[async_trait]
impl OrganizationSsoDnsTxtResolver for ControlledOrganizationSsoDnsResolver {
    async fn lookup_txt(
        &self,
        proof: &OrganizationSsoDomainProof,
    ) -> Result<OrganizationSsoDnsTxtAnswers, OrganizationSsoDnsResolverError> {
        Self::lookup_txt(self, proof).await
    }
}

#[async_trait]
impl OrganizationSsoDnsDomainResolver for ControlledOrganizationSsoDnsResolver {
    async fn lookup_txt_for_domain(
        &self,
        domain: &af_domain::OrganizationSsoDomain,
    ) -> Result<OrganizationSsoDnsTxtAnswers, OrganizationSsoDnsResolverError> {
        ControlledOrganizationSsoDnsResolver::lookup_txt_for_domain(self, domain).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_rejects_unsafe_configuration_boundaries() {
        let address = "127.0.0.1:53".parse().expect("测试 nameserver 地址有效");
        assert!(matches!(
            ControlledOrganizationSsoDnsResolver::new(&[], Duration::from_secs(1)),
            Err(OrganizationSsoDnsResolverConfigError::InvalidNameservers)
        ));
        assert!(matches!(
            ControlledOrganizationSsoDnsResolver::new(
                &[address; MAX_ORGANIZATION_SSO_DNS_NAMESERVERS + 1],
                Duration::from_secs(1),
            ),
            Err(OrganizationSsoDnsResolverConfigError::InvalidNameservers)
        ));
        assert!(matches!(
            ControlledOrganizationSsoDnsResolver::new(&[address, address], Duration::from_secs(1)),
            Err(OrganizationSsoDnsResolverConfigError::InvalidNameservers)
        ));
        assert!(matches!(
            ControlledOrganizationSsoDnsResolver::new(&[address], Duration::ZERO),
            Err(OrganizationSsoDnsResolverConfigError::InvalidTimeout)
        ));
        assert!(matches!(
            ControlledOrganizationSsoDnsResolver::new(&[address], Duration::from_secs(31)),
            Err(OrganizationSsoDnsResolverConfigError::InvalidTimeout)
        ));
    }
}
