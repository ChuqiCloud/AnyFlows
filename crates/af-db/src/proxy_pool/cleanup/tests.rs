use super::*;
use crate::{CredentialProxyRepository, CredentialProxyScheme, DatabaseOptions, MigrationOptions};

fn envelope() -> EncryptedCredentialEnvelope {
    EncryptedCredentialEnvelope::new("test", [1; 24], vec![2; 32]).unwrap()
}

async fn setup() -> (DatabasePool, ProxyPoolRepository, ProxyId, Vec<ProxyId>) {
    let database = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:").unwrap(),
        MigrationOptions::default(),
    )
    .await
    .unwrap();
    let repo = ProxyPoolRepository::new(database.clone(), Duration::from_secs(10));
    let id = repo
        .create("cleanup-pool".to_owned(), true, 60, |_| Ok(envelope()))
        .await
        .unwrap();
    let candidates = (1..=3)
        .map(|index| {
            (
                format!("{index:064x}"),
                String::new(),
                record(format!("node-{index}")),
            )
        })
        .collect();
    repo.replace_nodes(id, 1, candidates, |_, _| Ok(None))
        .await
        .unwrap();
    let ids = repo
        .get(id)
        .await
        .unwrap()
        .nodes
        .iter()
        .map(|node| node.proxy.proxy_id())
        .collect();
    (database, repo, id, ids)
}

fn record(name: String) -> CredentialProxyCreateRecord {
    CredentialProxyCreateRecord::new(
        name,
        CredentialProxyScheme::Http,
        "192.0.2.1".to_owned(),
        8080,
        None,
        false,
        true,
    )
}

fn request(
    pool: Option<ProxyId>,
    mode: ProxyCleanupMode,
    ids: Vec<ProxyId>,
    preview: bool,
) -> ProxyCleanupRequest {
    ProxyCleanupRequest {
        target: ProxyCleanupTarget::Nodes,
        mode,
        pool_id: pool,
        revision: pool.map(|_| 1),
        ids,
        after_id: None,
        preview,
    }
}

#[tokio::test]
async fn proxy_pool_cleanup_distinguishes_failed_pending_and_stale_results() {
    let (_database, repo, pool, ids) = setup().await;
    repo.finish(
        pool,
        1,
        "ready",
        vec![(ids[0], 1, None), (ids[1], 1, Some(5))],
    )
    .await
    .unwrap();
    let preview = repo
        .cleanup(request(Some(pool), ProxyCleanupMode::Failed, vec![], true))
        .await
        .unwrap();
    assert_eq!(preview.eligible_ids, vec![ids[0].get()]);
    assert_eq!(
        repo.get(pool).await.unwrap().nodes.len(),
        3,
        "preview must not remove nodes"
    );
    // A new successful result between preview and confirmation is rechecked.
    repo.finish(pool, 1, "ready", vec![(ids[0], 1, Some(4))])
        .await
        .unwrap();
    let result = repo
        .cleanup(request(
            Some(pool),
            ProxyCleanupMode::Failed,
            vec![ids[0]],
            false,
        ))
        .await
        .unwrap();
    assert!(result.deleted_ids.is_empty());
    assert_eq!(result.skipped_ids, vec![ids[0].get()]);
    assert_eq!(repo.get(pool).await.unwrap().revision, 2);
    assert!(matches!(
        repo.cleanup(request(Some(pool), ProxyCleanupMode::Failed, vec![], true))
            .await,
        Err(ProxyPoolRepositoryError::Conflict)
    ));
    assert!(matches!(
        repo.finish(pool, 1, "ready", vec![(ids[0], 1, None)]).await,
        Err(ProxyPoolRepositoryError::Conflict)
    ));
}

#[tokio::test]
async fn proxy_pool_cleanup_rechecks_bindings_and_keeps_node_directory() {
    let (_database, repo, pool, ids) = setup().await;
    let pool_request = |preview| ProxyCleanupRequest {
        target: ProxyCleanupTarget::Pools,
        mode: ProxyCleanupMode::Selected,
        pool_id: None,
        revision: None,
        ids: vec![pool],
        after_id: None,
        preview,
    };
    assert_eq!(
        repo.cleanup(pool_request(true)).await.unwrap().eligible_ids,
        vec![pool.get()]
    );
    repo.bind(0, Some(pool)).await.unwrap();
    let protected = repo.cleanup(pool_request(false)).await.unwrap();
    assert_eq!(protected.protected_ids, vec![pool.get()]);
    assert!(protected.deleted_ids.is_empty());
    repo.bind(0, None).await.unwrap();
    assert_eq!(
        repo.cleanup(pool_request(false)).await.unwrap().deleted_ids,
        vec![pool.get()]
    );
    for id in ids {
        assert!(
            repo.proxy(id).await.is_ok(),
            "pool deletion must retain manual node targets"
        );
    }
}

#[tokio::test]
async fn proxy_pool_cleanup_protects_members_and_continues_unused_batches() {
    let (database, repo, _pool, ids) = setup().await;
    let protected = repo
        .cleanup(request(
            None,
            ProxyCleanupMode::Selected,
            ids.clone(),
            false,
        ))
        .await
        .unwrap();
    assert_eq!(protected.protected_ids.len(), 3);
    let proxies =
        CredentialProxyRepository::new(database.clone(), Duration::from_secs(10)).unwrap();
    let mut unused = Vec::new();
    for index in 0..257 {
        unused.push(
            proxies
                .create(record(format!("unused-{index}")), |_| Ok(None))
                .await
                .unwrap()
                .proxy_id(),
        );
    }
    let preview = repo
        .cleanup(request(None, ProxyCleanupMode::Unused, vec![], true))
        .await
        .unwrap();
    assert_eq!(preview.eligible_ids.len(), 256);
    assert!(preview.more_available);
    assert_eq!(preview.next_after_id, Some(unused[255].get()));
    let mut next = request(None, ProxyCleanupMode::Unused, vec![], true);
    next.after_id = Some(unused[255]);
    let next = repo.cleanup(next).await.unwrap();
    assert_eq!(next.eligible_ids, vec![unused[256].get()]);
    assert!(!next.more_available);
    let result = repo
        .cleanup(request(
            None,
            ProxyCleanupMode::Unused,
            vec![unused[256]],
            false,
        ))
        .await
        .unwrap();
    assert_eq!(result.deleted_ids, vec![unused[256].get()]);
    assert!(matches!(
        repo.proxy(unused[256]).await,
        Err(ProxyPoolRepositoryError::NotFound)
    ));
}

#[tokio::test]
async fn proxy_pool_cleanup_deletes_failed_nodes_and_protects_active_work() {
    let (_database, repo, pool, ids) = setup().await;
    let at = TimeDateTimeWithTimeZone::now_utc().unix_timestamp();
    assert!(repo.claim(pool, 1, at, true).await.unwrap());
    assert!(matches!(
        repo.cleanup(request(Some(pool), ProxyCleanupMode::Failed, vec![], true))
            .await,
        Err(ProxyPoolRepositoryError::Conflict)
    ));
    assert!(matches!(
        repo.delete(pool).await,
        Err(ProxyPoolRepositoryError::Conflict)
    ));
    repo.finish(
        pool,
        1,
        "ready",
        vec![(ids[0], 1, None), (ids[1], 1, Some(5))],
    )
    .await
    .unwrap();
    let result = repo
        .cleanup(request(
            Some(pool),
            ProxyCleanupMode::Failed,
            vec![ids[0]],
            false,
        ))
        .await
        .unwrap();
    assert_eq!(result.deleted_ids, vec![ids[0].get()]);
    let remaining = repo.get(pool).await.unwrap();
    assert_eq!(remaining.revision, 2);
    assert_eq!(remaining.nodes.len(), 2);
    assert!(
        remaining
            .nodes
            .iter()
            .any(|node| node.proxy.proxy_id() == ids[1] && node.healthy)
    );
    assert!(
        remaining
            .nodes
            .iter()
            .any(|node| node.proxy.proxy_id() == ids[2] && node.checked_at.is_none())
    );
    assert!(matches!(
        repo.proxy(ids[0]).await,
        Err(ProxyPoolRepositoryError::NotFound)
    ));
}
