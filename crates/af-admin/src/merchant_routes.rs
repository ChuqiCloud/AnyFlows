//! Consumer merchant routing never grants merchant management privileges.
use std::{future::Future,pin::Pin};
use af_db::{MerchantPoolRepository,UserRouteRepository,UserRouteError};
pub use af_db::{MerchantRouteWrite,MerchantRouteRecord,MerchantServiceStatistics,MerchantConsumerAttempt};
use crate::SessionPrincipal;

pub type MerchantRouteFuture<'a,T> = Pin<Box<dyn Future<Output=Result<T,UserRouteError>>+Send+'a>>;
pub trait MerchantRouteService:Send+Sync {
    fn attempts(&self,principal:SessionPrincipal,request_id:String)->MerchantRouteFuture<'_,Vec<MerchantConsumerAttempt>>;
    fn list(&self,principal:SessionPrincipal)->MerchantRouteFuture<'_,Vec<MerchantRouteRecord>>;
    fn get(&self,principal:SessionPrincipal,id:i64)->MerchantRouteFuture<'_,MerchantRouteRecord>;
    fn save(&self,principal:SessionPrincipal,id:Option<i64>,write:MerchantRouteWrite)->MerchantRouteFuture<'_,MerchantRouteRecord>;
    fn delete(&self,principal:SessionPrincipal,id:i64)->MerchantRouteFuture<'_,()>;
    fn statistics(&self,principal:SessionPrincipal,pool_id:i64,model:Option<String>,window_seconds:u32)
        ->MerchantRouteFuture<'_,Vec<MerchantServiceStatistics>>;
}
pub struct DatabaseMerchantRouteService { routes:UserRouteRepository,pools:MerchantPoolRepository }
impl DatabaseMerchantRouteService {
    #[must_use]
    pub const fn new(routes:UserRouteRepository,pools:MerchantPoolRepository)->Self {Self{routes,pools}}
}
impl MerchantRouteService for DatabaseMerchantRouteService {
    fn attempts(&self,p:SessionPrincipal,request_id:String)->MerchantRouteFuture<'_,Vec<MerchantConsumerAttempt>>{
        Box::pin(async move {self.routes.merchant_attempts(p.user_id(),&request_id).await})
    }
    fn list(&self,p:SessionPrincipal)->MerchantRouteFuture<'_,Vec<MerchantRouteRecord>>{
        Box::pin(async move {self.routes.merchant_routes(p.user_id()).await})
    }
    fn get(&self,p:SessionPrincipal,id:i64)->MerchantRouteFuture<'_,MerchantRouteRecord>{
        Box::pin(async move {self.routes.merchant_route(p.user_id(),id).await})
    }
    fn save(&self,p:SessionPrincipal,id:Option<i64>,write:MerchantRouteWrite)->MerchantRouteFuture<'_,MerchantRouteRecord>{
        Box::pin(async move {self.routes.write_merchant_route(p.user_id(),id,write).await})
    }
    fn delete(&self,p:SessionPrincipal,id:i64)->MerchantRouteFuture<'_,()>{
        Box::pin(async move {
            self.routes.merchant_route(p.user_id(),id).await?;
            self.routes.delete(p.user_id(),id).await
        })
    }
    fn statistics(&self,p:SessionPrincipal,pool_id:i64,model:Option<String>,window_seconds:u32)
        ->MerchantRouteFuture<'_,Vec<MerchantServiceStatistics>> {
        Box::pin(async move {
            if !self.pools.check_pool_access(p.user_id(),pool_id).await.map_err(|_|UserRouteError::Internal)? {
                return Err(UserRouteError::NotFound);
            }
            self.routes.merchant_statistics(pool_id,model.as_deref(),window_seconds).await
        })
    }
}
