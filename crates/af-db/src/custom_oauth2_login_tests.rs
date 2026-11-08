use std::{error::Error, time::Duration};

use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, JsonValue, PaginatorTrait, Set};

use crate::{
    CustomOAuth2IdentityCompletion, CustomOAuth2LoginRepository, CustomOAuth2LoginRepositoryError,
    CustomOAuth2ProviderRepository, CustomOAuth2ProviderSecretUpdate,
    CustomOAuth2ProviderWriteRecord, DatabaseOptions, EncryptedCredentialEnvelope,
    MigrationOptions, connect_and_migrate,
    entity::{
        authentication_settings, custom_oauth2_identities, custom_oauth2_login_transactions,
        groups, invite_rebate_events, users, wallet_ledger_entries,
    },
};

const PROVIDER: &str = "custom_enterprise";
const SECOND_PROVIDER: &str = "custom_partner";
const STATE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SECOND_STATE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const THIRD_STATE: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const FOURTH_STATE: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
const TICKET: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const SECOND_TICKET: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const THIRD_TICKET: &str = "9999999999999999999999999999999999999999999999999999999999999999";
const FOURTH_TICKET: &str = "8888888888888888888888888888888888888888888888888888888888888888";

struct Fixture {
    pool: crate::DatabasePool,
    repository: CustomOAuth2LoginRepository,
    group_id: i64,
}

#[tokio::test]
async fn custom_state_binds_provider_version_and_is_single_use() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    fixture
        .repository
        .create_state(PROVIDER, 1, STATE, 100)
        .await?;
    assert_eq!(
        fixture.repository.claim_state(PROVIDER, STATE, 100).await,
        Err(CustomOAuth2LoginRepositoryError::Rejected)
    );
    assert_eq!(
        fixture
            .repository
            .create_state(PROVIDER, 1, STATE, 200)
            .await,
        Err(CustomOAuth2LoginRepositoryError::Conflict)
    );

    fixture
        .repository
        .create_state(PROVIDER, 1, SECOND_STATE, 200)
        .await?;
    assert_eq!(
        fixture
            .repository
            .claim_state(SECOND_PROVIDER, SECOND_STATE, 100)
            .await,
        Err(CustomOAuth2LoginRepositoryError::Rejected)
    );
    let claim = fixture
        .repository
        .claim_state(PROVIDER, SECOND_STATE, 100)
        .await?;
    assert_eq!(claim.configuration_version(), 1);
    assert_eq!(
        fixture
            .repository
            .claim_state(PROVIDER, SECOND_STATE, 100)
            .await,
        Err(CustomOAuth2LoginRepositoryError::Rejected)
    );
    assert_eq!(
        fixture
            .repository
            .complete_identity(
                claim,
                PROVIDER,
                "external-1",
                "custom_enterprise_external_1",
                TICKET,
                150,
                110,
            )
            .await?,
        CustomOAuth2IdentityCompletion::Rejected
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn custom_identity_obeys_registration_and_ticket_boundaries() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    enable_registration(&fixture, true).await?;
    fixture
        .repository
        .create_state(PROVIDER, 1, STATE, 200)
        .await?;
    let claim = fixture.repository.claim_state(PROVIDER, STATE, 100).await?;
    let user_id = match fixture
        .repository
        .complete_identity(
            claim,
            PROVIDER,
            "external-1",
            "custom_enterprise_external_1",
            TICKET,
            150,
            110,
        )
        .await?
    {
        CustomOAuth2IdentityCompletion::Issued(user_id) => user_id,
        CustomOAuth2IdentityCompletion::Rejected => return Err("已开启注册却拒绝未知身份".into()),
    };
    assert_eq!(
        fixture.repository.consume_ticket(TICKET, 120).await?,
        user_id
    );
    assert_eq!(
        fixture.repository.consume_ticket(TICKET, 120).await,
        Err(CustomOAuth2LoginRepositoryError::Rejected)
    );
    let user = users::Entity::find_by_id(user_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("自定义 OAuth2 新用户必须持久化");
    assert_eq!(user.password_hash, None);
    assert_eq!(user.email, None);

    enable_registration(&fixture, false).await?;
    fixture
        .repository
        .create_state(PROVIDER, 1, SECOND_STATE, 300)
        .await?;
    let claim = fixture
        .repository
        .claim_state(PROVIDER, SECOND_STATE, 200)
        .await?;
    assert_eq!(
        fixture
            .repository
            .complete_identity(
                claim,
                PROVIDER,
                "external-1",
                "unused_existing_username",
                SECOND_TICKET,
                260,
                210,
            )
            .await?,
        CustomOAuth2IdentityCompletion::Issued(user_id)
    );

    let mut disabled_user = user.into_active_model();
    disabled_user.status = Set(2);
    disabled_user.update(fixture.pool.connection()).await?;
    fixture
        .repository
        .create_state(PROVIDER, 1, THIRD_STATE, 300)
        .await?;
    let claim = fixture
        .repository
        .claim_state(PROVIDER, THIRD_STATE, 200)
        .await?;
    assert_eq!(
        fixture
            .repository
            .complete_identity(
                claim,
                PROVIDER,
                "external-1",
                "unused_disabled_username",
                THIRD_TICKET,
                260,
                210,
            )
            .await,
        Err(CustomOAuth2LoginRepositoryError::Rejected)
    );

    fixture
        .repository
        .create_state(PROVIDER, 1, FOURTH_STATE, 300)
        .await?;
    let claim = fixture
        .repository
        .claim_state(PROVIDER, FOURTH_STATE, 200)
        .await?;
    assert_eq!(
        fixture
            .repository
            .complete_identity(
                claim,
                PROVIDER,
                "external-2",
                "custom_enterprise_external_2",
                FOURTH_TICKET,
                260,
                210,
            )
            .await?,
        CustomOAuth2IdentityCompletion::Rejected
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn custom_oauth2_invitation_rewards_are_atomic_and_only_for_new_identities()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    enable_registration(&fixture, true).await?;
    set_invitation_rewards(&fixture, 20, 7).await?;
    let inviter = create_inviter(&fixture, 100).await?;
    fixture
        .repository
        .create_state(PROVIDER, 1, STATE, 200)
        .await?;
    let claim = fixture.repository.claim_state(PROVIDER, STATE, 100).await?;
    let issued = fixture
        .repository
        .complete_identity_with_invitation(
            claim,
            PROVIDER,
            "invited-subject",
            "invited-oauth-user",
            TICKET,
            150,
            110,
            Some(&inviter.aff_code),
        )
        .await?;
    let CustomOAuth2IdentityCompletion::Issued(user_id) = issued else {
        return Err("有效邀请应创建 OAuth 用户".into());
    };
    let invited = users::Entity::find_by_id(user_id.get())
        .one(fixture.pool.connection())
        .await?
        .unwrap();
    assert_eq!(invited.inviter_id, Some(inviter.id));
    assert_eq!(invited.quota, 130);
    let rewarded = users::Entity::find_by_id(inviter.id)
        .one(fixture.pool.connection())
        .await?
        .unwrap();
    assert_eq!(rewarded.quota, 120);
    assert_eq!(rewarded.aff_quota, 20);
    assert_eq!(rewarded.aff_history_quota, 20);
    let events = invite_rebate_events::Entity::find()
        .all(fixture.pool.connection())
        .await?;
    assert_eq!(events.len(), 1);
    let ledger = wallet_ledger_entries::Entity::find_by_id(events[0].wallet_ledger_entry_id)
        .one(fixture.pool.connection())
        .await?
        .unwrap();
    assert_eq!(ledger.user_id, inviter.id);
    assert_eq!(ledger.quota_delta, 20);
    assert_eq!((ledger.balance_before, ledger.balance_after), (100, 120));
    assert_eq!(
        fixture
            .repository
            .complete_identity_with_invitation(
                claim,
                PROVIDER,
                "invited-subject",
                "unused-replayed-username",
                SECOND_TICKET,
                150,
                110,
                Some(&inviter.aff_code),
            )
            .await,
        Err(CustomOAuth2LoginRepositoryError::Rejected)
    );
    enable_registration(&fixture, false).await?;
    fixture
        .repository
        .create_state(PROVIDER, 1, SECOND_STATE, 300)
        .await?;
    let existing_claim = fixture
        .repository
        .claim_state(PROVIDER, SECOND_STATE, 200)
        .await?;
    assert_eq!(
        fixture
            .repository
            .complete_identity_with_invitation(
                existing_claim,
                PROVIDER,
                "invited-subject",
                "unused-existing-username",
                SECOND_TICKET,
                260,
                210,
                Some("af-AAAAAAAAAAAAAAAAAAAAAA"),
            )
            .await?,
        CustomOAuth2IdentityCompletion::Issued(user_id)
    );
    assert_eq!(
        invite_rebate_events::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    assert_eq!(
        wallet_ledger_entries::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        3
    );
    enable_registration(&fixture, true).await?;
    set_invitation_rewards(&fixture, 0, 0).await?;
    fixture
        .repository
        .create_state(PROVIDER, 1, THIRD_STATE, 400)
        .await?;
    let zero_claim = fixture
        .repository
        .claim_state(PROVIDER, THIRD_STATE, 300)
        .await?;
    let zero_reward = fixture
        .repository
        .complete_identity_with_invitation(
            zero_claim,
            PROVIDER,
            "zero-reward-subject",
            "zero-reward-oauth-user",
            THIRD_TICKET,
            360,
            310,
            Some(&inviter.aff_code),
        )
        .await?;
    let CustomOAuth2IdentityCompletion::Issued(zero_user_id) = zero_reward else {
        return Err("零奖励仍应记录邀请关系".into());
    };
    let zero_user = users::Entity::find_by_id(zero_user_id.get())
        .one(fixture.pool.connection())
        .await?
        .unwrap();
    assert_eq!(zero_user.inviter_id, Some(inviter.id));
    assert_eq!(zero_user.quota, 123);
    assert_eq!(
        invite_rebate_events::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn custom_oauth2_invitation_rejection_rolls_back_user_identity_and_wallet()
-> Result<(), Box<dyn Error>> {
    for scenario in [
        "unknown",
        "disabled",
        "deleted",
        "closed",
        "overflow",
        "invitee-overflow",
    ] {
        let fixture = fixture().await?;
        enable_registration(&fixture, scenario != "closed").await?;
        set_invitation_rewards(&fixture, 20, 7).await?;
        let inviter_quota = if scenario == "overflow" {
            i64::MAX
        } else {
            100
        };
        let inviter = create_inviter(&fixture, inviter_quota).await?;
        if scenario == "invitee-overflow" {
            let settings = authentication_settings::Entity::find_by_id(1_i16)
                .one(fixture.pool.connection())
                .await?
                .unwrap();
            let mut settings = settings.into_active_model();
            settings.registration_initial_quota = Set(i64::MAX);
            settings.update(fixture.pool.connection()).await?;
        }
        if matches!(scenario, "disabled" | "deleted") {
            let mut inactive = inviter.clone().into_active_model();
            if scenario == "disabled" {
                inactive.status = Set(2);
            } else {
                inactive.deleted_at = Set(Some(
                    sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc(),
                ));
            }
            inactive.update(fixture.pool.connection()).await?;
        }
        fixture
            .repository
            .create_state(PROVIDER, 1, STATE, 200)
            .await?;
        let claim = fixture.repository.claim_state(PROVIDER, STATE, 100).await?;
        let result = fixture
            .repository
            .complete_identity_with_invitation(
                claim,
                PROVIDER,
                "rejected-subject",
                "rejected-oauth-user",
                TICKET,
                150,
                110,
                Some(if scenario == "unknown" {
                    "af-AAAAAAAAAAAAAAAAAAAAAA"
                } else {
                    &inviter.aff_code
                }),
            )
            .await;
        if matches!(scenario, "overflow" | "invitee-overflow") {
            assert!(result.is_err());
        } else {
            assert_eq!(result?, CustomOAuth2IdentityCompletion::Rejected);
        }
        assert_eq!(
            users::Entity::find()
                .count(fixture.pool.connection())
                .await?,
            1
        );
        assert_eq!(
            custom_oauth2_identities::Entity::find()
                .count(fixture.pool.connection())
                .await?,
            0
        );
        assert_eq!(
            invite_rebate_events::Entity::find()
                .count(fixture.pool.connection())
                .await?,
            0
        );
        assert_eq!(
            wallet_ledger_entries::Entity::find()
                .count(fixture.pool.connection())
                .await?,
            1
        );
        let login = custom_oauth2_login_transactions::Entity::find_by_id(claim.transaction_id())
            .one(fixture.pool.connection())
            .await?
            .unwrap();
        assert!(login.claimed_at.is_some());
        assert_eq!(login.user_id, None);
        assert_eq!(login.ticket_digest, None);
        assert_eq!(login.ticket_expires_at, None);
        let unchanged = users::Entity::find_by_id(inviter.id)
            .one(fixture.pool.connection())
            .await?
            .unwrap();
        assert_eq!(unchanged.quota, inviter_quota);
        assert_eq!(unchanged.aff_quota, 0);
        fixture.pool.close().await?;
    }
    Ok(())
}

async fn set_invitation_rewards(
    fixture: &Fixture,
    inviter_quota: i64,
    invitee_quota: i64,
) -> Result<(), Box<dyn Error>> {
    let settings = authentication_settings::Entity::find_by_id(1_i16)
        .one(fixture.pool.connection())
        .await?
        .unwrap();
    let mut settings = settings.into_active_model();
    settings.invitation_rebate_quota = Set(inviter_quota);
    settings.invitation_invitee_quota = Set(invitee_quota);
    settings.update(fixture.pool.connection()).await?;
    Ok(())
}

async fn create_inviter(fixture: &Fixture, quota: i64) -> Result<users::Model, Box<dyn Error>> {
    let repository = crate::AdminUserRepository::new(fixture.pool.clone(), Duration::from_secs(5))?;
    let inviter = repository
        .create(crate::AdminUserCreateRecord::new(
            "oauth-inviter".to_owned(),
            None,
            None,
            0,
            1,
            af_domain::GroupId::new(fixture.group_id)?,
            quota,
            None,
            None,
        ))
        .await?;
    Ok(users::Entity::find_by_id(inviter.user_id().get())
        .one(fixture.pool.connection())
        .await?
        .unwrap())
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let providers = CustomOAuth2ProviderRepository::new(pool.clone(), Duration::from_secs(2))?;
    for (key, marker) in [(PROVIDER, 1_u8), (SECOND_PROVIDER, 2_u8)] {
        providers
            .save_provider(
                key,
                CustomOAuth2ProviderWriteRecord::new(
                    0,
                    "企业登录".to_owned(),
                    "client-id".to_owned(),
                    "https://login.example.test/oauth/authorize".to_owned(),
                    "https://login.example.test/oauth/token".to_owned(),
                    "https://login.example.test/oauth/userinfo".to_owned(),
                    "openid profile".to_owned(),
                    "sub".to_owned(),
                    true,
                    CustomOAuth2ProviderSecretUpdate::Replace(test_envelope(marker)?),
                ),
            )
            .await?;
    }
    let group = groups::ActiveModel {
        name: Set("custom-oauth2-users".to_owned()),
        display_name: Set("Custom OAuth2 Users".to_owned()),
        flags: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(Fixture {
        repository: CustomOAuth2LoginRepository::new(pool.clone(), Duration::from_secs(2))?,
        pool,
        group_id: group.id,
    })
}

async fn enable_registration(fixture: &Fixture, enabled: bool) -> Result<(), Box<dyn Error>> {
    let model = authentication_settings::Entity::find_by_id(1_i16)
        .one(fixture.pool.connection())
        .await?
        .expect("认证设置固定行必须存在");
    let mut active = model.into_active_model();
    active.registration_enabled = Set(enabled);
    active.registration_default_group_id = Set(Some(fixture.group_id));
    active.registration_initial_quota = Set(123);
    active.update(fixture.pool.connection()).await?;
    Ok(())
}

fn test_envelope(marker: u8) -> Result<EncryptedCredentialEnvelope, Box<dyn Error>> {
    Ok(EncryptedCredentialEnvelope::new(
        "primary-key",
        [marker; crate::CREDENTIAL_ENVELOPE_NONCE_BYTES],
        vec![marker; crate::CREDENTIAL_ENVELOPE_TAG_BYTES],
    )?)
}
