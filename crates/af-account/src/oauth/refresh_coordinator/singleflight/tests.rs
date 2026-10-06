use std::{future::pending, num::NonZeroUsize, time::Duration};

use af_domain::{ChannelId, CredentialId};

use super::*;

const TEST_RETENTION: Duration = Duration::from_millis(40);

#[tokio::test]
async fn followers_share_completed_result_and_cleanup_after_retention() {
    let registry = OAuthRefreshSingleflight::new(NonZeroUsize::new(2).unwrap(), TEST_RETENTION);
    let key = key(1, 1);
    let OAuthRefreshFlightAcquisition::Leader(leader) = registry.acquire(key).unwrap() else {
        panic!("首次获取必须成为 leader");
    };
    let OAuthRefreshFlightAcquisition::Follower(follower) = registry.acquire(key).unwrap() else {
        panic!("同一版本的第二次获取必须成为 follower");
    };

    assert_eq!(
        leader.complete(Ok(OAuthRefreshCoordinatorOutcome::Stored)),
        Ok(OAuthRefreshCoordinatorOutcome::Stored)
    );
    assert_eq!(
        follower.wait().await,
        Ok(OAuthRefreshCoordinatorOutcome::Stored)
    );
    assert_eq!(registry.entry_count(), 1);

    tokio::time::sleep(TEST_RETENTION + Duration::from_millis(30)).await;
    assert_eq!(registry.entry_count(), 0);
}

#[tokio::test]
async fn leader_cancellation_and_panic_wake_followers() {
    let registry = OAuthRefreshSingleflight::new(NonZeroUsize::new(2).unwrap(), TEST_RETENTION);

    let cancellation_key = key(1, 1);
    let OAuthRefreshFlightAcquisition::Leader(cancelled_leader) =
        registry.acquire(cancellation_key).unwrap()
    else {
        panic!("首次获取必须成为 leader");
    };
    let OAuthRefreshFlightAcquisition::Follower(cancelled_follower) =
        registry.acquire(cancellation_key).unwrap()
    else {
        panic!("重复获取必须成为 follower");
    };
    let cancelled_task = tokio::spawn(async move {
        let _leader = cancelled_leader;
        pending::<()>().await;
    });
    tokio::task::yield_now().await;
    cancelled_task.abort();
    assert!(cancelled_task.await.unwrap_err().is_cancelled());
    assert_eq!(
        cancelled_follower.wait().await,
        Err(OAuthRefreshCoordinatorError::LeaderAborted)
    );

    let panic_key = key(2, 1);
    let OAuthRefreshFlightAcquisition::Leader(panicked_leader) =
        registry.acquire(panic_key).unwrap()
    else {
        panic!("新凭据必须成为 leader");
    };
    let OAuthRefreshFlightAcquisition::Follower(panicked_follower) =
        registry.acquire(panic_key).unwrap()
    else {
        panic!("重复获取必须成为 follower");
    };
    let panicked_task = tokio::spawn(async move {
        let _leader = panicked_leader;
        panic!("测试 leader panic");
    });
    assert!(panicked_task.await.unwrap_err().is_panic());
    assert_eq!(
        panicked_follower.wait().await,
        Err(OAuthRefreshCoordinatorError::LeaderAborted)
    );
}

#[test]
fn capacity_rejects_only_when_all_slots_are_running() {
    let registry =
        OAuthRefreshSingleflight::new(NonZeroUsize::new(1).unwrap(), Duration::from_secs(30));
    let first_key = key(1, 1);
    let second_key = key(2, 1);
    let OAuthRefreshFlightAcquisition::Leader(first_leader) = registry.acquire(first_key).unwrap()
    else {
        panic!("首次获取必须成为 leader");
    };
    assert!(matches!(
        registry.acquire(second_key),
        Err(OAuthRefreshSingleflightError::CapacityExceeded)
    ));

    let _ = first_leader.complete(Ok(OAuthRefreshCoordinatorOutcome::Stored));
    let OAuthRefreshFlightAcquisition::Follower(first_follower) =
        registry.acquire(first_key).unwrap()
    else {
        panic!("保留期内必须复用已完成结果");
    };
    assert_eq!(
        first_follower.receiver.borrow().to_owned(),
        OAuthRefreshFlightState::Completed(Ok(OAuthRefreshCoordinatorOutcome::Stored))
    );

    let OAuthRefreshFlightAcquisition::Leader(second_leader) =
        registry.acquire(second_key).unwrap()
    else {
        panic!("已完成缓存必须可被提前淘汰");
    };
    assert_eq!(registry.entry_count(), 1);
    drop(second_leader);
}

#[test]
fn flight_key_separates_channel_credential_and_revision() {
    let base = key(1, 1);
    assert_ne!(base, key(1, 2));
    assert_ne!(base, key(2, 1));
    assert_ne!(
        base,
        OAuthRefreshFlightKey::new(ChannelId::new(2).unwrap(), CredentialId::new(1).unwrap(), 1,)
    );
}

fn key(credential_id: i64, revision: i64) -> OAuthRefreshFlightKey {
    OAuthRefreshFlightKey::new(
        ChannelId::new(1).unwrap(),
        CredentialId::new(credential_id).unwrap(),
        revision,
    )
}
