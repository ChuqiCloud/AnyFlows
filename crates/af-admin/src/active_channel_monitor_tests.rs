use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use af_db::{ActiveChannelMonitorRepository, DatabaseOptions, MigrationOptions};
use af_domain::{ChannelId, CredentialId};

use crate::{
    ActiveChannelMonitorWorker, ActiveChannelProbe, ActiveChannelProbeFuture,
    ActiveChannelProbeResult,
};

#[derive(Default)]
struct CountingProbe(AtomicUsize);

impl ActiveChannelProbe for CountingProbe {
    fn probe_target<'a>(
        &'a self,
        _channel_id: ChannelId,
        _credential_id: CredentialId,
        _model: &'a str,
    ) -> ActiveChannelProbeFuture<'a> {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::SeqCst);
            ActiveChannelProbeResult::Healthy { latency_ms: 1 }
        })
    }
}

#[tokio::test]
async fn worker_does_not_call_probe_when_global_monitoring_is_defaulted_off()
-> Result<(), Box<dyn std::error::Error>> {
    let pool = af_db::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let probe = Arc::new(CountingProbe::default());
    let worker = ActiveChannelMonitorWorker::new(
        ActiveChannelMonitorRepository::new(pool.clone()),
        probe.clone(),
    );

    assert_eq!(worker.run_once().await?, 0);
    assert_eq!(probe.0.load(Ordering::SeqCst), 0);
    pool.close().await?;
    Ok(())
}
