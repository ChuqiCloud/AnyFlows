//! Merchant-only commands use session ownership; administrator channels are never exposed.
use crate::{SessionPrincipal, SessionRole};
use af_account::{CredentialDecryptor, CredentialEncryptor, PlainCredentialSecret};
use af_db::MerchantPoolRepository;
use af_domain::{
    MerchantApplication, MerchantCatalogService, MerchantListQuery, MerchantOwnedService,
    MerchantPage, MerchantPool, MerchantPoolError, MerchantPoolSettings, MerchantPoolTerms,
    MerchantPoolWrite, MerchantProfile, MerchantReview, MerchantServiceWrite,
};
use std::{future::Future, net::IpAddr, pin::Pin};

pub type MerchantFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, MerchantPoolError>> + Send + 'a>>;

pub trait MerchantPoolService: Send + Sync {
    fn pools(
        &self,
        principal: SessionPrincipal,
        query: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantPool>>;
    fn catalog(
        &self,
        principal: SessionPrincipal,
        pool: i64,
        query: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantCatalogService>>;
    fn catalog_service(
        &self,
        principal: SessionPrincipal,
        pool: i64,
        id: i64,
    ) -> MerchantFuture<'_, MerchantCatalogService>;
    fn profile(&self, principal: SessionPrincipal) -> MerchantFuture<'_, Option<MerchantProfile>>;
    fn quote(
        &self,
        principal: SessionPrincipal,
        write: af_domain::MerchantQuoteWrite,
    ) -> MerchantFuture<'_, af_domain::MerchantQuote>;
    fn apply(
        &self,
        principal: SessionPrincipal,
        write: MerchantApplication,
    ) -> MerchantFuture<'_, MerchantProfile>;
    fn available_pools(
        &self,
        principal: SessionPrincipal,
        query: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantPool>>;
    fn services(
        &self,
        principal: SessionPrincipal,
        query: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantOwnedService>>;
    fn service(
        &self,
        principal: SessionPrincipal,
        id: i64,
    ) -> MerchantFuture<'_, MerchantOwnedService>;
    fn write_service(
        &self,
        principal: SessionPrincipal,
        id: Option<i64>,
        write: MerchantServiceWrite,
    ) -> MerchantFuture<'_, MerchantOwnedService>;
    fn submit_service(
        &self,
        principal: SessionPrincipal,
        id: i64,
    ) -> MerchantFuture<'_, MerchantOwnedService>;
    fn pause_service(
        &self,
        principal: SessionPrincipal,
        id: i64,
        paused: bool,
    ) -> MerchantFuture<'_, MerchantOwnedService>;
    fn rotate_service_key(
        &self,
        principal: SessionPrincipal,
        id: i64,
        key: String,
    ) -> MerchantFuture<'_, MerchantOwnedService>;
    fn admin_pools(
        &self,
        principal: SessionPrincipal,
        query: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantPool>>;
    fn write_pool(
        &self,
        principal: SessionPrincipal,
        id: Option<i64>,
        write: MerchantPoolWrite,
    ) -> MerchantFuture<'_, MerchantPool>;
    fn admin_merchants(
        &self,
        principal: SessionPrincipal,
        query: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantProfile>>;
    fn review_merchant(
        &self,
        principal: SessionPrincipal,
        id: i64,
        review: MerchantReview,
    ) -> MerchantFuture<'_, MerchantProfile>;
    fn admin_services(
        &self,
        principal: SessionPrincipal,
        query: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantOwnedService>>;
    fn admin_service(
        &self,
        principal: SessionPrincipal,
        id: i64,
    ) -> MerchantFuture<'_, MerchantOwnedService>;
    fn review_service(
        &self,
        principal: SessionPrincipal,
        id: i64,
        review: MerchantReview,
    ) -> MerchantFuture<'_, MerchantOwnedService>;
    fn resume_service(
        &self,
        principal: SessionPrincipal,
        id: i64,
    ) -> MerchantFuture<'_, MerchantOwnedService>;
    fn settings(&self, principal: SessionPrincipal) -> MerchantFuture<'_, MerchantPoolSettings>;
    fn write_settings(
        &self,
        principal: SessionPrincipal,
        write: MerchantPoolSettings,
    ) -> MerchantFuture<'_, MerchantPoolSettings>;
    fn terms(
        &self,
        principal: SessionPrincipal,
        pool: i64,
        merchant: i64,
    ) -> MerchantFuture<'_, MerchantPoolTerms>;
    fn write_terms(
        &self,
        principal: SessionPrincipal,
        pool: i64,
        merchant: i64,
        write: MerchantPoolTerms,
    ) -> MerchantFuture<'_, MerchantPoolTerms>;
}

pub struct DatabaseMerchantPoolService {
    repository: MerchantPoolRepository,
    encryptor: CredentialEncryptor,
    decryptor: CredentialDecryptor,
    blocked_hosts: Vec<String>,
}

impl DatabaseMerchantPoolService {
    #[must_use]
    pub fn new(
        repository: MerchantPoolRepository,
        encryptor: CredentialEncryptor,
        decryptor: CredentialDecryptor,
    ) -> Self {
        Self {
            repository,
            encryptor,
            decryptor,
            blocked_hosts: Vec::new(),
        }
    }
    #[must_use]
    pub fn with_blocked_hosts(mut self, hosts: Vec<String>) -> Self {
        self.blocked_hosts = hosts
            .into_iter()
            .map(|host| host.to_ascii_lowercase())
            .collect();
        self
    }
    async fn validate_endpoint(&self, value: &str) -> Result<(), MerchantPoolError> {
        let parsed = url::Url::parse(value).map_err(|_| MerchantPoolError::InvalidInput)?;
        if parsed.scheme() != "https"
            || parsed.port_or_known_default() != Some(443)
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || value.len() > 2048
        {
            return Err(MerchantPoolError::InvalidInput);
        }
        let host = parsed
            .host_str()
            .ok_or(MerchantPoolError::InvalidInput)?
            .trim_end_matches('.')
            .to_ascii_lowercase();
        if host == "localhost"
            || host.ends_with(".localhost")
            || host.ends_with(".local")
            || host.ends_with(".internal")
            || self
                .blocked_hosts
                .iter()
                .any(|blocked| blocked.trim_end_matches('.') == host)
        {
            return Err(MerchantPoolError::InvalidInput);
        }
        if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
            return if public_ip(ip) {
                Ok(())
            } else {
                Err(MerchantPoolError::InvalidInput)
            };
        }
        let addresses = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::net::lookup_host((host.as_str(), 443)),
        )
        .await
        .map_err(|_| MerchantPoolError::Unavailable)?
        .map_err(|_| MerchantPoolError::InvalidInput)?;
        let mut count = 0;
        for address in addresses {
            count += 1;
            if !public_ip(address.ip()) {
                return Err(MerchantPoolError::InvalidInput);
            }
        }
        if count == 0 {
            return Err(MerchantPoolError::InvalidInput);
        }
        Ok(())
    }
}

impl MerchantPoolService for DatabaseMerchantPoolService {
    fn pools(
        &self,
        p: SessionPrincipal,
        q: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantPool>> {
        Box::pin(async move { self.repository.list_pools(Some(p.user_id()), q).await })
    }
    fn catalog(
        &self,
        p: SessionPrincipal,
        pool: i64,
        q: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantCatalogService>> {
        Box::pin(async move { self.repository.catalog(p.user_id(), pool, q).await })
    }
    fn catalog_service(
        &self,
        p: SessionPrincipal,
        pool: i64,
        id: i64,
    ) -> MerchantFuture<'_, MerchantCatalogService> {
        Box::pin(async move { self.repository.catalog_service(p.user_id(), pool, id).await })
    }
    fn profile(&self, p: SessionPrincipal) -> MerchantFuture<'_, Option<MerchantProfile>> {
        Box::pin(async move { self.repository.owner_merchant(p.user_id()).await })
    }
    fn quote(
        &self,
        p: SessionPrincipal,
        write: af_domain::MerchantQuoteWrite,
    ) -> MerchantFuture<'_, af_domain::MerchantQuote> {
        Box::pin(async move { self.repository.quote(p.user_id(), write).await })
    }
    fn apply(
        &self,
        p: SessionPrincipal,
        write: MerchantApplication,
    ) -> MerchantFuture<'_, MerchantProfile> {
        Box::pin(async move { self.repository.apply(p.user_id(), write).await })
    }
    fn available_pools(
        &self,
        p: SessionPrincipal,
        q: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantPool>> {
        Box::pin(async move { self.repository.available_supply_pools(p.user_id(), q).await })
    }
    fn services(
        &self,
        p: SessionPrincipal,
        q: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantOwnedService>> {
        Box::pin(async move { self.repository.list_services(Some(p.user_id()), q).await })
    }
    fn service(&self, p: SessionPrincipal, id: i64) -> MerchantFuture<'_, MerchantOwnedService> {
        Box::pin(async move { self.repository.get_service(Some(p.user_id()), id).await })
    }
    fn write_service(
        &self,
        p: SessionPrincipal,
        id: Option<i64>,
        write: MerchantServiceWrite,
    ) -> MerchantFuture<'_, MerchantOwnedService> {
        Box::pin(async move {
            write.validate()?;
            let merchant = self
                .repository
                .owner_merchant(p.user_id())
                .await?
                .ok_or(MerchantPoolError::Forbidden)?;
            if merchant.status != "approved" {
                return Err(MerchantPoolError::Forbidden);
            }
            if let Some(id) = id {
                self.repository.get_service(Some(p.user_id()), id).await?;
            }
            self.validate_endpoint(&write.base_url).await?;
            if let Some(key) = &write.api_key
                && let Ok(key) = crate::PresentedApiKey::parse(key)
                && self
                    .repository
                    .platform_key_exists(key.digest().as_str())
                    .await?
            {
                return Err(MerchantPoolError::InvalidInput);
            }
            self.repository
                .write_service(
                    p.user_id(),
                    id,
                    write,
                    |channel, credential, previous, key| {
                        let secret = if let Some(key) = key {
                            PlainCredentialSecret::api_key(key)
                                .map_err(|_| MerchantPoolError::InvalidInput)?
                        } else {
                            let previous = previous.ok_or(MerchantPoolError::InvalidInput)?;
                            let secret = self
                                .decryptor
                                .decrypt_envelope(
                                    previous.channel_id,
                                    previous.credential_id.get(),
                                    af_domain::CredentialKind::ApiKey,
                                    &previous.envelope,
                                )
                                .map_err(|_| MerchantPoolError::Unavailable)?;
                            PlainCredentialSecret::api_key(secret.expose_secret().to_owned())
                                .map_err(|_| MerchantPoolError::Unavailable)?
                        };
                        self.encryptor
                            .encrypt(channel, credential.get(), &secret)
                            .map_err(|_| MerchantPoolError::Unavailable)
                    },
                )
                .await
        })
    }
    fn submit_service(
        &self,
        p: SessionPrincipal,
        id: i64,
    ) -> MerchantFuture<'_, MerchantOwnedService> {
        Box::pin(async move { self.repository.submit_service(p.user_id(), id).await })
    }
    fn pause_service(
        &self,
        p: SessionPrincipal,
        id: i64,
        paused: bool,
    ) -> MerchantFuture<'_, MerchantOwnedService> {
        Box::pin(async move { self.repository.pause_service(p.user_id(), id, paused).await })
    }
    fn rotate_service_key(
        &self,
        p: SessionPrincipal,
        id: i64,
        key: String,
    ) -> MerchantFuture<'_, MerchantOwnedService> {
        Box::pin(async move {
            if key.is_empty() || key.len() > 8192 {
                return Err(MerchantPoolError::InvalidInput);
            }
            if let Ok(presented) = crate::PresentedApiKey::parse(&key)
                && self
                    .repository
                    .platform_key_exists(presented.digest().as_str())
                    .await?
            {
                return Err(MerchantPoolError::InvalidInput);
            }
            let secret =
                PlainCredentialSecret::api_key(key).map_err(|_| MerchantPoolError::InvalidInput)?;
            self.repository
                .rotate_service_key(p.user_id(), id, |channel, credential| {
                    self.encryptor
                        .encrypt(channel, credential.get(), &secret)
                        .map_err(|_| MerchantPoolError::Unavailable)
                })
                .await
        })
    }
    fn admin_pools(
        &self,
        p: SessionPrincipal,
        q: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantPool>> {
        Box::pin(async move {
            admin(p)?;
            self.repository.list_pools(None, q).await
        })
    }
    fn write_pool(
        &self,
        p: SessionPrincipal,
        id: Option<i64>,
        write: MerchantPoolWrite,
    ) -> MerchantFuture<'_, MerchantPool> {
        Box::pin(async move {
            admin(p)?;
            self.repository.write_pool(p.user_id(), id, write).await
        })
    }
    fn admin_merchants(
        &self,
        p: SessionPrincipal,
        q: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantProfile>> {
        Box::pin(async move {
            admin(p)?;
            self.repository.list_profiles(q).await
        })
    }
    fn review_merchant(
        &self,
        p: SessionPrincipal,
        id: i64,
        review: MerchantReview,
    ) -> MerchantFuture<'_, MerchantProfile> {
        Box::pin(async move {
            admin(p)?;
            self.repository
                .review_merchant(p.user_id(), id, review)
                .await
        })
    }
    fn admin_services(
        &self,
        p: SessionPrincipal,
        q: MerchantListQuery,
    ) -> MerchantFuture<'_, MerchantPage<MerchantOwnedService>> {
        Box::pin(async move {
            admin(p)?;
            self.repository.list_services(None, q).await
        })
    }
    fn admin_service(
        &self,
        p: SessionPrincipal,
        id: i64,
    ) -> MerchantFuture<'_, MerchantOwnedService> {
        Box::pin(async move {
            admin(p)?;
            self.repository.get_service(None, id).await
        })
    }
    fn review_service(
        &self,
        p: SessionPrincipal,
        id: i64,
        review: MerchantReview,
    ) -> MerchantFuture<'_, MerchantOwnedService> {
        Box::pin(async move {
            admin(p)?;
            if review.decision == "approve" {
                let service = self.repository.get_service(None, id).await?;
                let revision = service
                    .draft
                    .as_ref()
                    .or(service.published.as_ref())
                    .ok_or(MerchantPoolError::Conflict)?;
                self.validate_endpoint(&revision.base_url).await?;
            }
            self.repository
                .review_service(p.user_id(), id, review)
                .await
        })
    }
    fn resume_service(
        &self,
        p: SessionPrincipal,
        id: i64,
    ) -> MerchantFuture<'_, MerchantOwnedService> {
        Box::pin(async move {
            admin(p)?;
            let service = self.repository.get_service(None, id).await?;
            let published = service
                .published
                .as_ref()
                .ok_or(MerchantPoolError::Conflict)?;
            self.validate_endpoint(&published.base_url).await?;
            self.repository.resume_service(p.user_id(), id).await
        })
    }
    fn settings(&self, p: SessionPrincipal) -> MerchantFuture<'_, MerchantPoolSettings> {
        Box::pin(async move {
            admin(p)?;
            self.repository.settings().await
        })
    }
    fn write_settings(
        &self,
        p: SessionPrincipal,
        write: MerchantPoolSettings,
    ) -> MerchantFuture<'_, MerchantPoolSettings> {
        Box::pin(async move {
            admin(p)?;
            self.repository.write_settings(p.user_id(), write).await
        })
    }
    fn terms(
        &self,
        p: SessionPrincipal,
        pool: i64,
        merchant: i64,
    ) -> MerchantFuture<'_, MerchantPoolTerms> {
        Box::pin(async move {
            admin(p)?;
            self.repository.terms(pool, merchant).await
        })
    }
    fn write_terms(
        &self,
        p: SessionPrincipal,
        pool: i64,
        merchant: i64,
        write: MerchantPoolTerms,
    ) -> MerchantFuture<'_, MerchantPoolTerms> {
        Box::pin(async move {
            admin(p)?;
            self.repository
                .write_terms(p.user_id(), pool, merchant, write)
                .await
        })
    }
}

fn admin(principal: SessionPrincipal) -> Result<(), MerchantPoolError> {
    if principal.role() != SessionRole::Admin {
        return Err(MerchantPoolError::Forbidden);
    }
    Ok(())
}
fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_documentation()
                || a == 0
                || a >= 240
                || (a == 100 && (64..=127).contains(&b))
                || (a == 192 && b == 0 && c == 0)
                || (a == 198 && matches!(b, 18 | 19)))
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            (s[0] & 0xe000) == 0x2000 && !(s[0] == 0x2001 && (s[1] == 0x0db8 || s[1] < 0x0200))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::public_ip;
    #[test]
    fn merchant_endpoint_rejects_private_and_special_addresses() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "198.18.0.1",
            "::1",
            "::ffff:127.0.0.1",
            "2001:db8::1",
        ] {
            assert!(!public_ip(address.parse().expect("fixture address")));
        }
        for address in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(public_ip(address.parse().expect("fixture address")));
        }
    }
}
