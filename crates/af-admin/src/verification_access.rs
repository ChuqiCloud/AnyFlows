use af_db::{AccountVerificationRepository, AccountVerificationSettingsRepository};
use af_domain::UserId;

#[cfg(test)]
#[path = "verification_access_tests.rs"]
mod tests;

/// 充值和模型调用共享的实时实名准入策略。
pub struct VerificationAccessService {
    settings: AccountVerificationSettingsRepository,
    verifications: AccountVerificationRepository,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerificationAccessStatus {
    pub verified: bool,
    pub require_verification_for_topup: bool,
    pub require_verification_for_api: bool,
}

#[derive(Clone, Copy, Debug, Eq, thiserror::Error, PartialEq)]
pub enum VerificationAccessError {
    #[error("请先完成实名认证")]
    Required,
    #[error("实名认证准入服务暂不可用")]
    Unavailable,
}

impl VerificationAccessService {
    #[must_use]
    pub const fn new(
        settings: AccountVerificationSettingsRepository,
        verifications: AccountVerificationRepository,
    ) -> Self {
        Self {
            settings,
            verifications,
        }
    }

    pub async fn status(
        &self,
        user: UserId,
    ) -> Result<VerificationAccessStatus, VerificationAccessError> {
        let settings = self
            .settings
            .settings()
            .await
            .map_err(|_| VerificationAccessError::Unavailable)?;
        let verified = self
            .verifications
            .verified(user)
            .await
            .map_err(|_| VerificationAccessError::Unavailable)?;
        Ok(VerificationAccessStatus {
            verified,
            require_verification_for_topup: settings.require_verification_for_topup,
            require_verification_for_api: settings.require_verification_for_api,
        })
    }

    pub async fn ensure_topup(&self, user: UserId) -> Result<(), VerificationAccessError> {
        self.ensure(user, false).await
    }

    pub async fn ensure_api(&self, user: UserId) -> Result<(), VerificationAccessError> {
        self.ensure(user, true).await
    }

    async fn ensure(&self, user: UserId, api: bool) -> Result<(), VerificationAccessError> {
        let settings = self
            .settings
            .settings()
            .await
            .map_err(|_| VerificationAccessError::Unavailable)?;
        let required = if api {
            settings.require_verification_for_api
        } else {
            settings.require_verification_for_topup
        };
        if !required {
            return Ok(());
        }
        if self
            .verifications
            .verified(user)
            .await
            .map_err(|_| VerificationAccessError::Unavailable)?
        {
            Ok(())
        } else {
            Err(VerificationAccessError::Required)
        }
    }
}
