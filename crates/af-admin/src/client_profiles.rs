use af_db::ClientProfileRepository;
pub use af_db::{
    ClientProfilePage, ClientProfileQuery, ClientProfileRecord, ClientProfileReference,
    ClientProfileReferencePage, ClientProfileRepositoryError, ClientProfileRevisionPage,
    ClientProfileWrite,
};
use af_domain::{ChannelId, ClientProfileBinding, ClientProfileSnapshot};
use thiserror::Error;

use crate::{SessionPrincipal, SessionRole};

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AdminClientProfileError {
    #[error("仅平台管理员可管理客户端档案")]
    Forbidden,
    #[error(transparent)]
    Repository(#[from] ClientProfileRepositoryError),
}

#[derive(Debug)]
pub struct DatabaseAdminClientProfileService {
    repository: ClientProfileRepository,
}

impl DatabaseAdminClientProfileService {
    pub const fn new(repository: ClientProfileRepository) -> Self {
        Self { repository }
    }

    pub async fn list(
        &self,
        principal: SessionPrincipal,
        query: ClientProfileQuery,
    ) -> Result<ClientProfilePage, AdminClientProfileError> {
        require_admin(principal)?;
        Ok(self.repository.list(query).await?)
    }
    pub async fn get(
        &self,
        principal: SessionPrincipal,
        key: &str,
    ) -> Result<ClientProfileRecord, AdminClientProfileError> {
        require_admin(principal)?;
        Ok(self.repository.get(key).await?)
    }
    pub async fn revision(
        &self,
        principal: SessionPrincipal,
        key: &str,
        revision: i64,
    ) -> Result<ClientProfileSnapshot, AdminClientProfileError> {
        require_admin(principal)?;
        Ok(self.repository.revision(key, revision).await?)
    }
    pub async fn revisions(
        &self,
        principal: SessionPrincipal,
        key: &str,
        before: Option<i64>,
        limit: usize,
    ) -> Result<ClientProfileRevisionPage, AdminClientProfileError> {
        require_admin(principal)?;
        Ok(self.repository.revisions(key, before, limit).await?)
    }
    pub async fn save(
        &self,
        principal: SessionPrincipal,
        key: &str,
        write: ClientProfileWrite,
    ) -> Result<ClientProfileSnapshot, AdminClientProfileError> {
        require_admin(principal)?;
        Ok(self
            .repository
            .save(key, write, principal.user_id())
            .await?)
    }
    pub async fn set_enabled(
        &self,
        principal: SessionPrincipal,
        key: &str,
        expected_version: i64,
        enabled: bool,
    ) -> Result<ClientProfileRecord, AdminClientProfileError> {
        require_admin(principal)?;
        Ok(self
            .repository
            .set_enabled(key, expected_version, enabled)
            .await?)
    }
    pub async fn delete_revision(
        &self,
        principal: SessionPrincipal,
        key: &str,
        revision: i64,
        expected_version: i64,
    ) -> Result<(), AdminClientProfileError> {
        require_admin(principal)?;
        Ok(self
            .repository
            .delete_revision(key, revision, expected_version)
            .await?)
    }
    pub async fn references(
        &self,
        principal: SessionPrincipal,
        key: &str,
        after: Option<i64>,
        limit: usize,
    ) -> Result<ClientProfileReferencePage, AdminClientProfileError> {
        require_admin(principal)?;
        Ok(self.repository.references(key, after, limit).await?)
    }
    pub async fn channel_binding(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
    ) -> Result<Option<ClientProfileReference>, AdminClientProfileError> {
        require_admin(principal)?;
        Ok(self.repository.channel_binding(channel).await?)
    }
    pub async fn bind(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
        binding: ClientProfileBinding,
        expected_version: i64,
    ) -> Result<ClientProfileReference, AdminClientProfileError> {
        require_admin(principal)?;
        Ok(self
            .repository
            .bind(channel, binding, expected_version)
            .await?)
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminClientProfileError> {
    (principal.role() == SessionRole::Admin)
        .then_some(())
        .ok_or(AdminClientProfileError::Forbidden)
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_domain::UserId;

    #[test]
    fn ordinary_users_cannot_manage_upstream_identity() {
        let user = UserId::new(1).unwrap();
        assert_eq!(
            require_admin(SessionPrincipal::new(user, SessionRole::User)),
            Err(AdminClientProfileError::Forbidden)
        );
        assert_eq!(
            require_admin(SessionPrincipal::new(user, SessionRole::Admin)),
            Ok(())
        );
    }
}
