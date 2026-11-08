//! Consumer merchant routing never grants merchant management privileges.
use std::{future::Future,pin::Pin};
use af_db::{MerchantPoolRepository,UserRouteRepository,UserRouteError};
pub use af_db::{MerchantRouteWrite,MerchantRouteRecord,MerchantServiceStatistics,MerchantConsumerAttempt,MerchantServiceTimeline};
use crate::SessionPrincipal;

pub type MerchantRouteFuture<'a,T> = Pin<Box<dyn Future<Output=Result<T,UserRouteError>>+Send+'a>>;
pub trait MerchantRouteService:Send+Sync {
    fn own_statistics(&self,principal:SessionPrincipal,model:Option<String>,window_seconds:u32,protocol:Option<String>,streaming:Option<bool>)
        ->MerchantRouteFuture<'_,Vec<MerchantServiceStatistics>>;
    fn timeline(&self,principal:SessionPrincipal,pool_id:i64,model:Option<String>,window_seconds:u32,protocol:Option<String>,streaming:Option<bool>)
        ->MerchantRouteFuture<'_,Vec<MerchantServiceTimeline>>;
    fn attempts(&self,principal:SessionPrincipal,request_id:String)->MerchantRouteFuture<'_,Vec<MerchantConsumerAttempt>>;
    fn list(&self,principal:SessionPrincipal)->MerchantRouteFuture<'_,Vec<MerchantRouteRecord>>;
    fn get(&self,principal:SessionPrincipal,id:i64)->MerchantRouteFuture<'_,MerchantRouteRecord>;
    fn save(&self,principal:SessionPrincipal,id:Option<i64>,write:MerchantRouteWrite)->MerchantRouteFuture<'_,MerchantRouteRecord>;
    fn delete(&self,principal:SessionPrincipal,id:i64)->MerchantRouteFuture<'_,()>;
    fn statistics(&self,principal:SessionPrincipal,pool_id:i64,model:Option<String>,window_seconds:u32,protocol:Option<String>,streaming:Option<bool>)
        ->MerchantRouteFuture<'_,Vec<MerchantServiceStatistics>>;
}
pub struct DatabaseMerchantRouteService { routes:UserRouteRepository,pools:MerchantPoolRepository }
impl DatabaseMerchantRouteService {
    #[must_use]
    pub const fn new(routes:UserRouteRepository,pools:MerchantPoolRepository)->Self {Self{routes,pools}}
}
impl MerchantRouteService for DatabaseMerchantRouteService {
    fn own_statistics(&self,p:SessionPrincipal,model:Option<String>,window_seconds:u32,protocol:Option<String>,streaming:Option<bool>)
        ->MerchantRouteFuture<'_,Vec<MerchantServiceStatistics>>{
        Box::pin(async move {
            let merchant=self.pools.owner_merchant(p.user_id()).await.map_err(|_|UserRouteError::Internal)?
                .ok_or(UserRouteError::NotFound)?;
            let mut page=1;let mut pools=std::collections::BTreeSet::new();
            loop {
                let list=self.pools.list_services(Some(p.user_id()),af_domain::MerchantListQuery{page:Some(page),page_size:Some(100),..Default::default()})
                    .await.map_err(|_|UserRouteError::Internal)?;
                for service in list.items {pools.insert(service.pool_id);}
                if u64::from(page)*100>=list.total {break;}
                page+=1;
            }
            let mut result=Vec::new();
            for pool in pools {
                let statistics=self.routes.merchant_statistics_filtered(pool,model.as_deref(),window_seconds,protocol.as_deref(),streaming).await?;
                result.extend(statistics.into_iter().filter(|row|row.merchant_id==merchant.id));
            }
            Ok(result)
        })
    }
    fn timeline(&self,p:SessionPrincipal,pool_id:i64,model:Option<String>,window_seconds:u32,protocol:Option<String>,streaming:Option<bool>)
        ->MerchantRouteFuture<'_,Vec<MerchantServiceTimeline>>{
        Box::pin(async move {
            if !self.pools.check_pool_access(p.user_id(),pool_id).await.map_err(|_|UserRouteError::Internal)? {
                return Err(UserRouteError::NotFound);
            }
            self.routes.merchant_timeline(pool_id,model.as_deref(),window_seconds,protocol.as_deref(),streaming).await
        })
    }
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
    fn statistics(&self,p:SessionPrincipal,pool_id:i64,model:Option<String>,window_seconds:u32,protocol:Option<String>,streaming:Option<bool>)
        ->MerchantRouteFuture<'_,Vec<MerchantServiceStatistics>> {
        Box::pin(async move {
            if !self.pools.check_pool_access(p.user_id(),pool_id).await.map_err(|_|UserRouteError::Internal)? {
                return Err(UserRouteError::NotFound);
            }
            self.routes.merchant_statistics_filtered(pool_id,model.as_deref(),window_seconds,protocol.as_deref(),streaming).await
        })
    }
}
