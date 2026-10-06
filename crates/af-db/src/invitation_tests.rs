use std::{error::Error, time::Duration};

use af_domain::{GroupId, Quota};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};

use crate::{
    AdminUserCreateRecord, AdminUserRegistrationCreateOutcome, AdminUserRepository,
    DatabaseOptions, MigrationOptions, UserInvitationLookupOutcome, UserInvitationRepository,
    entity::{groups, invite_rebate_events, users},
};

const CREDITED_AT: u64 = 1_800_000_000;

#[tokio::test]
async fn registration_invitation_and_rebate_share_one_transaction() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("registration-invitation".to_owned()),
        display_name: Set("注册邀请闭环".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let group_id = GroupId::new(group.id)?;
    let users_repository = AdminUserRepository::new(pool.clone(), Duration::from_secs(5))?;
    let inviter = users_repository
        .create(user_record("inviter", group_id))
        .await?;
    let inviter_model = users::Entity::find_by_id(inviter.user_id().get())
        .one(pool.connection())
        .await?
        .expect("邀请人必须存在");

    let created = users_repository
        .create_registration(
            user_record("invitee", group_id),
            None,
            Some(&inviter_model.aff_code),
            Quota::new(40)?,
            CREDITED_AT,
        )
        .await?;
    let AdminUserRegistrationCreateOutcome::Created(invitee) = created else {
        panic!("有效邀请码必须完成注册")
    };
    let invitee_model = users::Entity::find_by_id(invitee.user_id().get())
        .one(pool.connection())
        .await?
        .expect("被邀请用户必须存在");
    assert_eq!(invitee_model.inviter_id, Some(inviter.user_id().get()));

    let inviter_model = users::Entity::find_by_id(inviter.user_id().get())
        .one(pool.connection())
        .await?
        .expect("邀请人必须存在");
    assert_eq!(inviter_model.quota, 40);
    assert_eq!(inviter_model.aff_quota, 40);
    assert_eq!(inviter_model.aff_history_quota, 40);

    let invitations = UserInvitationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let UserInvitationLookupOutcome::Found(summary) = invitations.get(inviter.user_id()).await?
    else {
        panic!("当前邀请人必须能读取自己的邀请中心")
    };
    assert_eq!(summary.invite_code(), inviter_model.aff_code);
    assert_eq!(summary.invited_count(), 1);
    assert_eq!(summary.credited_count(), 1);
    assert_eq!(summary.current_rebate_quota().units(), 40);
    assert_eq!(summary.historical_rebate_quota().units(), 40);
    assert_eq!(summary.recent_rebates().len(), 1);
    assert_eq!(summary.recent_rebates()[0].quota_amount().units(), 40);
    assert_eq!(summary.recent_rebates()[0].credited_at(), CREDITED_AT);

    assert!(matches!(
        users_repository
            .create_registration(
                user_record("invalid-invitee", group_id),
                None,
                Some("af-AAAAAAAAAAAAAAAAAAAAAA"),
                Quota::new(40)?,
                CREDITED_AT,
            )
            .await?,
        AdminUserRegistrationCreateOutcome::InvitationRejected
    ));
    assert!(
        users::Entity::find()
            .filter(users::Column::Username.eq("invalid-invitee"))
            .one(pool.connection())
            .await?
            .is_none()
    );

    let no_rebate = users_repository
        .create_registration(
            user_record("relationship-only", group_id),
            None,
            Some(&inviter_model.aff_code),
            Quota::new(0)?,
            CREDITED_AT + 1,
        )
        .await?;
    assert!(matches!(
        no_rebate,
        AdminUserRegistrationCreateOutcome::Created(_)
    ));
    let UserInvitationLookupOutcome::Found(summary) = invitations.get(inviter.user_id()).await?
    else {
        panic!("邀请汇总必须存在")
    };
    assert_eq!(summary.invited_count(), 2);
    assert_eq!(summary.credited_count(), 1);
    assert_eq!(
        invite_rebate_events::Entity::find()
            .filter(invite_rebate_events::Column::InviterUserId.eq(inviter.user_id().get()))
            .count(pool.connection())
            .await?,
        1
    );

    pool.close().await?;
    Ok(())
}

fn user_record(username: &str, group_id: GroupId) -> AdminUserCreateRecord {
    AdminUserCreateRecord::new(
        username.to_owned(),
        None,
        None,
        0,
        1,
        group_id,
        0,
        None,
        None,
    )
}
