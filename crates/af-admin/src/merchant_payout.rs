//! Owner-scoped payout configuration; plaintext exists only at the encryption boundary.
use crate::{SessionPrincipal, SessionRole};
use af_account::{PlainSystemSecret, SystemSecretCipher, SystemSecretKind};
use af_db::{
    MerchantFinanceError, MerchantFinancePage, MerchantPayout, MerchantPayoutAccount,
    MerchantPayoutConfig, MerchantPayoutRepository, MerchantPoolRepository, encode_payout_secret,
};
use af_httpclient::HttpClientProvider;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicI64, Ordering};
use url::Url;
use zeroize::Zeroizing;

#[derive(Clone, Debug, Serialize)]
pub struct AdminMerchantPayoutSettings {
    pub version: i64,
    #[serde(flatten)]
    pub config: MerchantPayoutConfig,
    pub signing_key_configured: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantPayoutSettingsCommand {
    pub expected_version: i64,
    pub enabled: bool,
    pub auto_approve_enabled: bool,
    pub gateway_url: String,
    pub currency: String,
    pub quota_per_unit: i64,
    pub min_amount_minor: i64,
    pub max_amount_minor: i64,
    pub daily_limit_minor: i64,
    pub signing_key: Option<String>,
    #[serde(default)]
    pub clear_signing_key: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantPayoutAccountCommand {
    pub expected_version: i64,
    pub label: String,
    pub account: Option<String>,
    #[serde(default)]
    pub clear: bool,
}

pub struct DatabaseMerchantPayoutService {
    repository: MerchantPayoutRepository,
    merchants: MerchantPoolRepository,
    cipher: SystemSecretCipher,
    clients: HttpClientProvider,
    scan_after: AtomicI64,
}
impl DatabaseMerchantPayoutService {
    pub const fn new(
        repository: MerchantPayoutRepository,
        merchants: MerchantPoolRepository,
        cipher: SystemSecretCipher,
        clients: HttpClientProvider,
    ) -> Self {
        Self {
            repository,
            merchants,
            cipher,
            clients,
            scan_after: AtomicI64::new(0),
        }
    }
    async fn owner(&self, principal: SessionPrincipal) -> Result<i64, MerchantFinanceError> {
        self.merchants
            .owner_merchant(principal.user_id())
            .await
            .map_err(|_| MerchantFinanceError::Unavailable)?
            .map(|value| value.id)
            .ok_or(MerchantFinanceError::NotFound)
    }
    pub async fn settings(
        &self,
        principal: SessionPrincipal,
    ) -> Result<AdminMerchantPayoutSettings, MerchantFinanceError> {
        admin(principal)?;
        let value = self.repository.settings().await?;
        Ok(AdminMerchantPayoutSettings {
            version: value.version,
            config: value.config,
            signing_key_configured: value.secret.is_some(),
        })
    }
    pub async fn save_settings(
        &self,
        principal: SessionPrincipal,
        command: MerchantPayoutSettingsCommand,
    ) -> Result<AdminMerchantPayoutSettings, MerchantFinanceError> {
        admin(principal)?;
        let key = command.signing_key.map(Zeroizing::new);
        if command.expected_version < 0
            || command.quota_per_unit <= 0
            || command.min_amount_minor <= 0
            || command.max_amount_minor < command.min_amount_minor
            || command.daily_limit_minor < command.max_amount_minor
            || command.max_amount_minor > 1_000_000_000
            || command.daily_limit_minor > 100_000_000_000
            || ![
                "USD", "CNY", "EUR", "GBP", "HKD", "SGD", "AUD", "CAD", "CHF", "NZD", "INR", "BRL",
            ]
            .contains(&command.currency.as_str())
            || (command.clear_signing_key && key.as_ref().is_some_and(|key| !key.is_empty()))
        {
            return Err(MerchantFinanceError::InvalidInput);
        }
        let gateway_url = normalize_gateway(&command.gateway_url, command.enabled)?;
        let secret = match key.filter(|value| !value.trim().is_empty()) {
            Some(value) => {
                if value.len() < 32 || value.len() > 1024 {
                    return Err(MerchantFinanceError::InvalidInput);
                }
                let plain = PlainSystemSecret::new(value.to_string())
                    .map_err(|_| MerchantFinanceError::InvalidInput)?;
                Some(encode_payout_secret(
                    &self
                        .cipher
                        .encrypt(SystemSecretKind::MerchantPayoutGateway, &plain)
                        .map_err(|_| MerchantFinanceError::Unavailable)?,
                )?)
            }
            None => None,
        };
        let config = MerchantPayoutConfig {
            enabled: command.enabled,
            auto_approve_enabled: command.auto_approve_enabled,
            gateway_url,
            currency: command.currency,
            quota_per_unit: command.quota_per_unit,
            min_amount_minor: command.min_amount_minor,
            max_amount_minor: command.max_amount_minor,
            daily_limit_minor: command.daily_limit_minor,
        };
        let value = self
            .repository
            .save_settings(
                command.expected_version,
                config,
                secret,
                command.clear_signing_key,
            )
            .await?;
        Ok(AdminMerchantPayoutSettings {
            version: value.version,
            config: value.config,
            signing_key_configured: value.secret.is_some(),
        })
    }
    pub async fn account(
        &self,
        principal: SessionPrincipal,
    ) -> Result<MerchantPayoutAccount, MerchantFinanceError> {
        self.repository.account(self.owner(principal).await?).await
    }
    pub async fn save_account(
        &self,
        principal: SessionPrincipal,
        command: MerchantPayoutAccountCommand,
    ) -> Result<MerchantPayoutAccount, MerchantFinanceError> {
        let merchant = self.owner(principal).await?;
        let input = command.account.map(Zeroizing::new);
        if command.expected_version < 0
            || command.label.len() > 80
            || command.label.chars().any(char::is_control)
            || (command.clear && input.as_ref().is_some_and(|value| !value.is_empty()))
        {
            return Err(MerchantFinanceError::InvalidInput);
        }
        let secret = if command.clear {
            None
        } else {
            let value = input.ok_or(MerchantFinanceError::InvalidInput)?;
            if value.is_empty() || value.len() > 1024 {
                return Err(MerchantFinanceError::InvalidInput);
            }
            let plain = PlainSystemSecret::new(value.to_string())
                .map_err(|_| MerchantFinanceError::InvalidInput)?;
            Some(encode_payout_secret(
                &self
                    .cipher
                    .encrypt(SystemSecretKind::MerchantPayoutAccount(merchant), &plain)
                    .map_err(|_| MerchantFinanceError::Unavailable)?,
            )?)
        };
        self.repository
            .save_account(merchant, command.expected_version, command.label, secret)
            .await
    }
    pub async fn payouts(
        &self,
        principal: SessionPrincipal,
        administrator: bool,
        merchant: Option<i64>,
        limit: u32,
        before: Option<i64>,
    ) -> Result<MerchantFinancePage<MerchantPayout>, MerchantFinanceError> {
        let merchant = if administrator {
            admin(principal)?;
            merchant
        } else {
            Some(self.owner(principal).await?)
        };
        self.repository.payouts(merchant, limit, before).await
    }
    pub async fn enqueue(
        &self,
        principal: SessionPrincipal,
        id: i64,
        access_token_id: Option<i64>,
        request_id: &str,
    ) -> Result<MerchantPayout, MerchantFinanceError> {
        admin(principal)?;
        let initiator = af_db::MerchantPayoutInitiator::new(
            principal.user_id().get(),
            access_token_id,
            request_id.to_owned(),
            access_token_id.map(|_| "admin:submit_merchant_payout:execute".to_owned()),
        )?;
        self.repository
            .enqueue_with_initiator(id, false, Some(&initiator))
            .await
    }
    pub async fn reconcile(
        &self,
        principal: SessionPrincipal,
        id: i64,
    ) -> Result<(), MerchantFinanceError> {
        admin(principal)?;
        self.repository.reconcile(id).await
    }
    /// Each turn is bounded. Unknown external outcomes keep locked funds and query only.
    pub async fn run_once(&self) -> Result<(), MerchantFinanceError> {
        let settings = self.repository.settings().await?;
        if settings.config.enabled && settings.config.auto_approve_enabled {
            let pending = self
                .repository
                .pending_withdrawals(self.scan_after.load(Ordering::Relaxed))
                .await?;
            self.scan_after
                .store(pending.last().copied().unwrap_or(0), Ordering::Relaxed);
            for id in pending {
                match self.repository.enqueue(id, true).await {
                    Ok(_)
                    | Err(
                        MerchantFinanceError::InvalidInput
                        | MerchantFinanceError::Conflict
                        | MerchantFinanceError::InsufficientBalance
                        | MerchantFinanceError::Forbidden
                        | MerchantFinanceError::NotFound,
                    ) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        for _ in 0..10 {
            let owner = uuid::Uuid::new_v4().simple().to_string();
            let Some(claim) = self.repository.claim(&owner).await? else {
                break;
            };
            if claim.submit && !self.repository.authorize_first_submit(&claim).await? {
                continue;
            }
            let result =
                crate::merchant_payout_gateway::execute(&self.clients, &self.cipher, &claim).await;
            self.repository
                .complete(
                    &claim,
                    result.status,
                    result.reference.as_str(),
                    result.reason,
                )
                .await?;
        }
        Ok(())
    }
}
fn admin(principal: SessionPrincipal) -> Result<(), MerchantFinanceError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(MerchantFinanceError::Forbidden)
    }
}
fn normalize_gateway(value: &str, enabled: bool) -> Result<String, MerchantFinanceError> {
    if value.is_empty() && !enabled {
        return Ok(String::new());
    }
    let url = Url::parse(value).map_err(|_| MerchantFinanceError::InvalidInput)?;
    if value.len() > 2048
        || url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(MerchantFinanceError::InvalidInput);
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}
