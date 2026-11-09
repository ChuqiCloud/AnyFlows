use af_admin::DatabaseMerchantPayoutService;
use std::{future::Future, sync::Arc, time::Duration};

/// Cancellation leaves a durable processing lease; the next instance only queries.
pub(crate) async fn run_until<F>(service: Arc<DatabaseMerchantPayoutService>, shutdown: F)
where
    F: Future<Output = ()>,
{
    tokio::pin!(shutdown);
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {biased;()=&mut shutdown=>return,_=interval.tick()=>{}}
        tokio::select! {biased;()=&mut shutdown=>return,result=service.run_once()=>{
            if result.is_err(){tracing::warn!("商户出款任务暂不可用，将在下轮恢复对账");}
        }}
    }
}
