//! Session-scoped linking of a user's external account to a platform connector.

use std::{fmt, future::Future, pin::Pin};

use serde::Serialize;
use zeroize::Zeroizing;

use crate::SessionPrincipal;

pub type ExternalAccountFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ExternalAccountError>> + Send + 'a>>;

/// Write-only platform credential held briefly while a link is verified.
pub struct ExternalAccountCredential(Zeroizing<String>);

impl ExternalAccountCredential {
    pub fn new(value: String) -> Result<Self, ExternalAccountError> {
        let value = Zeroizing::new(value);
        if value.is_empty()
            || value.len() > 8192
            || value.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(ExternalAccountError::InvalidInput);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for ExternalAccountCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ExternalAccountCredential(<redacted>)")
    }
}

pub struct ExternalAccountLinkCommand {
    connector_id: String,
    base_url: String,
    credential: ExternalAccountCredential,
}

impl ExternalAccountLinkCommand {
    pub fn new(
        connector_id: String,
        base_url: String,
        credential: String,
    ) -> Result<Self, ExternalAccountError> {
        let credential = ExternalAccountCredential::new(credential)?;
        if !valid_connector_id(&connector_id) || !valid_base_url(&base_url) {
            return Err(ExternalAccountError::InvalidInput);
        }
        Ok(Self {
            connector_id,
            base_url,
            credential,
        })
    }

    #[must_use]
    pub fn connector_id(&self) -> &str {
        &self.connector_id
    }

    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    #[must_use]
    pub fn credential(&self) -> &ExternalAccountCredential {
        &self.credential
    }

    #[must_use]
    pub fn into_parts(self) -> (String, String, ExternalAccountCredential) {
        (self.connector_id, self.base_url, self.credential)
    }
}

impl fmt::Debug for ExternalAccountLinkCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExternalAccountLinkCommand")
            .field("connector_id", &self.connector_id)
            .field("base_url", &"<redacted>")
            .field("credential", &self.credential)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalAccountBalanceStatus {
    Available,
    Unavailable,
    AuthenticationRequired,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAccountBalanceSnapshot {
    pub amount: Option<String>,
    pub unit: Option<String>,
    pub status: ExternalAccountBalanceStatus,
    pub observed_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAccount {
    pub id: i64,
    pub connector_id: String,
    pub external_subject: String,
    pub display_name: Option<String>,
    pub base_url: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub latest_balance: Option<ExternalAccountBalanceSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAccountBalancePage {
    pub connection_id: i64,
    pub items: Vec<ExternalAccountBalanceSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportedExternalConnector {
    pub id: String,
    pub display_name: String,
    pub authentication_modes: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ExternalAccountError {
    #[error("external account request is invalid")]
    InvalidInput,
    #[error("requested connector is unavailable")]
    Unsupported,
    #[error("external account is already linked")]
    Conflict,
    #[error("external account was not found")]
    NotFound,
    #[error("external platform rejected the credential")]
    AuthenticationRequired,
    #[error("external platform is temporarily unavailable")]
    Unavailable,
    #[error("external platform connector is busy")]
    RateLimited,
    #[error("external account connection limit reached")]
    LimitReached,
    #[error("external account operation failed")]
    Internal,
}

/// User-owned external accounts; calls are always scoped to the current session.
pub trait UserExternalAccountService: Send + Sync {
    fn connectors(&self) -> Vec<SupportedExternalConnector>;

    fn list(&self, principal: SessionPrincipal) -> ExternalAccountFuture<'_, Vec<ExternalAccount>>;

    fn link(
        &self,
        principal: SessionPrincipal,
        command: ExternalAccountLinkCommand,
    ) -> ExternalAccountFuture<'_, ExternalAccount>;

    fn refresh(
        &self,
        principal: SessionPrincipal,
        connection_id: i64,
    ) -> ExternalAccountFuture<'_, ExternalAccount>;

    fn unlink(
        &self,
        principal: SessionPrincipal,
        connection_id: i64,
    ) -> ExternalAccountFuture<'_, ()>;

    fn balance_history(
        &self,
        principal: SessionPrincipal,
        connection_id: i64,
        limit: u32,
    ) -> ExternalAccountFuture<'_, ExternalAccountBalancePage>;
}

fn valid_connector_id(value: &str) -> bool {
    value.len() <= 128
        && value.split('.').count() >= 2
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                && segment.as_bytes()[0].is_ascii_alphanumeric()
                && segment.as_bytes()[segment.len() - 1].is_ascii_alphanumeric()
        })
}

fn valid_base_url(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    value.len() <= 2048
        && value.trim() == value
        && url.scheme() == "https"
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_link_command_redacts_secrets_and_rejects_unsafe_urls() {
        let command = ExternalAccountLinkCommand::new(
            "org.anyflows.newapi".to_owned(),
            "https://gateway.example".to_owned(),
            "pat-secret-value".to_owned(),
        )
        .unwrap();
        assert!(!format!("{command:?}").contains("pat-secret-value"));
        assert!(
            ExternalAccountLinkCommand::new(
                "org.anyflows.newapi".to_owned(),
                "http://gateway.example".to_owned(),
                "token".to_owned(),
            )
            .is_err()
        );
    }
}
