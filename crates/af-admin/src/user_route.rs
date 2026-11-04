//! 当前会话用户的智能路由管理应用边界。

use std::{future::Future, pin::Pin};

use af_db::UserRouteRepository;
pub use af_db::{
    UserRouteError, UserRouteMemberRecord, UserRouteMemberWrite, UserRouteRecord, UserRouteWrite,
};

use crate::SessionPrincipal;

pub type UserRouteListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<UserRouteRecord>, UserRouteError>> + Send + 'a>>;
pub type UserRouteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserRouteRecord, UserRouteError>> + Send + 'a>>;
pub type UserRouteDeleteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), UserRouteError>> + Send + 'a>>;

/// 所有 API 均以已认证会话中的用户标识确定所有权。
pub trait UserRouteService: Send + Sync {
    fn list(&self, principal: SessionPrincipal) -> UserRouteListFuture<'_>;
    fn get(&self, principal: SessionPrincipal, id: i64) -> UserRouteFuture<'_>;
    fn create(&self, principal: SessionPrincipal, write: UserRouteWrite) -> UserRouteFuture<'_>;
    fn update(
        &self,
        principal: SessionPrincipal,
        id: i64,
        write: UserRouteWrite,
    ) -> UserRouteFuture<'_>;
    fn delete(&self, principal: SessionPrincipal, id: i64) -> UserRouteDeleteFuture<'_>;
}

pub struct DatabaseUserRouteService {
    repository: UserRouteRepository,
}

impl DatabaseUserRouteService {
    #[must_use]
    pub const fn new(repository: UserRouteRepository) -> Self {
        Self { repository }
    }
}

impl UserRouteService for DatabaseUserRouteService {
    fn list(&self, principal: SessionPrincipal) -> UserRouteListFuture<'_> {
        Box::pin(async move { self.repository.list(principal.user_id()).await })
    }
    fn get(&self, principal: SessionPrincipal, id: i64) -> UserRouteFuture<'_> {
        Box::pin(async move { self.repository.get(principal.user_id(), id).await })
    }
    fn create(&self, principal: SessionPrincipal, write: UserRouteWrite) -> UserRouteFuture<'_> {
        Box::pin(async move { self.repository.create(principal.user_id(), write).await })
    }
    fn update(
        &self,
        principal: SessionPrincipal,
        id: i64,
        write: UserRouteWrite,
    ) -> UserRouteFuture<'_> {
        Box::pin(async move { self.repository.update(principal.user_id(), id, write).await })
    }
    fn delete(&self, principal: SessionPrincipal, id: i64) -> UserRouteDeleteFuture<'_> {
        Box::pin(async move { self.repository.delete(principal.user_id(), id).await })
    }
}
