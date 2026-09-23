use super::*;
use crate::{DatabaseOptions, MigrationOptions, entity::groups};
use sea_orm::PaginatorTrait;
use std::error::Error;

async fn fixture()
-> Result<(DatabasePool, AccountVerificationRepository, [UserId; 3]), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("verification-test".to_owned()),
        display_name: Set("Verification test".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let mut ids = Vec::new();
    for index in 0..3 {
        let user = users::ActiveModel {
            username: Set(format!("verification-{index}")),
            role: Set(if index == 2 { 1 } else { 0 }),
            status: Set(1),
            default_group_id: Set(group.id),
            aff_code: Set(format!("verification-aff-{index}")),
            settings: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        ids.push(UserId::new(user.id)?);
    }
    Ok((
        pool.clone(),
        AccountVerificationRepository::new(pool),
        ids.try_into().expect("three users"),
    ))
}

fn submission(kind: &str) -> AccountVerificationSubmit {
    let bytes = b"%PDF-1.7\nverification fixture".to_vec();
    AccountVerificationSubmit {
        kind: kind.to_owned(),
        provider: "manual".to_owned(),
        document_country: "US".to_owned(),
        document_type: if kind == "enterprise" {
            "business_registration".to_owned()
        } else {
            "passport".to_owned()
        },
        document_number: Some("A12345678".to_owned()),
        subject_name: "Verified applicant".to_owned(),
        summary: "Identity supporting document".to_owned(),
        materials: vec![OrganizationVerificationMaterialWrite {
            kind: "identity".to_owned(),
            object_reference: "unused".to_owned(),
            file_name: "identity.pdf".to_owned(),
            content_type: "application/pdf".to_owned(),
            size_bytes: bytes.len() as i64,
            content_bytes: bytes,
        }],
    }
}

#[test]
fn document_numbers_are_validated_and_only_masked_values_are_exposed() {
    assert!(valid_document_number(
        "CN",
        "national_id",
        "11010519491231002X"
    ));
    assert!(!valid_document_number(
        "CN",
        "national_id",
        "110105194912310021"
    ));
    assert!(valid_document_number("US", "passport", "A12345678"));
    assert!(!valid_document_number("US", "passport", "A123/45678"));
    assert_eq!(mask_document_number("11010519491231002X"), "11****2X");
}

#[tokio::test]
async fn only_approved_enterprise_cases_grant_qualification_and_preserve_materials()
-> Result<(), Box<dyn Error>> {
    let (pool, repo, users) = fixture().await?;
    assert!(!repo.enterprise_verified(users[0]).await?);
    let individual = repo.submit(users[0], submission("individual")).await?;
    repo.decide(
        individual.id,
        1,
        users[2],
        4,
        None,
        "individual-review".to_owned(),
    )
    .await?;
    assert!(!repo.enterprise_verified(users[0]).await?);
    let enterprise = repo.submit(users[0], submission("enterprise")).await?;
    assert!(matches!(
        repo.submit(users[0], submission("enterprise")).await,
        Err(AccountVerificationError::Conflict)
    ));
    assert!(!repo.enterprise_verified(users[0]).await?);
    let approved = repo
        .decide(
            enterprise.id,
            1,
            users[2],
            4,
            None,
            "enterprise-review".to_owned(),
        )
        .await?;
    assert_eq!(approved.version, 2);
    assert!(repo.enterprise_verified(users[0]).await?);
    assert!(!repo.enterprise_verified(users[1]).await?);
    assert!(matches!(
        repo.submit(users[0], submission("enterprise")).await,
        Err(AccountVerificationError::Conflict)
    ));
    let material = repo.materials(Some(users[0]), enterprise.id).await?;
    assert_eq!(material.len(), 1);
    assert_eq!(
        repo.download(Some(users[0]), enterprise.id, material[0].id)
            .await?
            .2,
        b"%PDF-1.7\nverification fixture"
    );
    assert_eq!(
        platform_audit_logs::Entity::find()
            .count(pool.connection())
            .await?,
        2
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn records_and_materials_are_user_scoped_and_reviews_require_another_admin()
-> Result<(), Box<dyn Error>> {
    let (pool, repo, users) = fixture().await?;
    let case = repo.submit(users[0], submission("enterprise")).await?;
    let material = repo.materials(Some(users[0]), case.id).await?;
    assert!(matches!(
        repo.get(Some(users[1]), case.id).await,
        Err(AccountVerificationError::NotFound)
    ));
    assert!(matches!(
        repo.materials(Some(users[1]), case.id).await,
        Err(AccountVerificationError::NotFound)
    ));
    assert!(matches!(
        repo.download(Some(users[1]), case.id, material[0].id).await,
        Err(AccountVerificationError::NotFound)
    ));
    assert!(repo.list(Some(users[1]), None, None, 25).await?.is_empty());
    assert!(matches!(
        repo.decide(case.id, 1, users[1], 4, None, "forbidden".to_owned())
            .await,
        Err(AccountVerificationError::Forbidden)
    ));
    let own = repo.submit(users[2], submission("enterprise")).await?;
    assert!(matches!(
        repo.decide(own.id, 1, users[2], 4, None, "self-review".to_owned())
            .await,
        Err(AccountVerificationError::SelfReview)
    ));
    assert!(matches!(
        repo.download(Some(users[2]), own.id, material[0].id).await,
        Err(AccountVerificationError::NotFound)
    ));
    assert_eq!(repo.get(None, case.id).await?.status, 1);
    assert_eq!(
        platform_audit_logs::Entity::find()
            .count(pool.connection())
            .await?,
        0
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn supplements_keep_prior_decisions_and_cursor_pages_do_not_repeat_cases()
-> Result<(), Box<dyn Error>> {
    let (pool, repo, users) = fixture().await?;
    let first = repo.submit(users[0], submission("enterprise")).await?;
    repo.decide(
        first.id,
        1,
        users[2],
        3,
        Some("Please upload a clearer copy".to_owned()),
        "supplement".to_owned(),
    )
    .await?;
    let second = repo.submit(users[0], submission("enterprise")).await?;
    repo.decide(
        second.id,
        1,
        users[2],
        5,
        Some("Document expired".to_owned()),
        "reject".to_owned(),
    )
    .await?;
    let third = repo.submit(users[0], submission("enterprise")).await?;
    assert!(matches!(
        repo.decide(third.id, 2, users[2], 4, None, "stale".to_owned())
            .await,
        Err(AccountVerificationError::Conflict)
    ));
    let page = repo.list(Some(users[0]), None, None, 2).await?;
    assert_eq!(
        page.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![third.id, second.id]
    );
    let next = repo.list(Some(users[0]), Some(second.id), None, 2).await?;
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].id, first.id);
    assert_eq!(
        next[0].review_reason.as_deref(),
        Some("Please upload a clearer copy")
    );
    assert_eq!(repo.materials(Some(users[0]), first.id).await?.len(), 1);
    assert_eq!(repo.list(None, None, Some(5), 25).await?[0].id, second.id);
    for (before, status, limit) in [
        (Some(0), None, 25),
        (None, Some(2), 25),
        (None, None, 0),
        (None, None, 51),
    ] {
        assert!(matches!(
            repo.list(None, before, status, limit).await,
            Err(AccountVerificationError::Invalid)
        ));
    }
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn audit_failure_rolls_back_review_and_qualification() -> Result<(), Box<dyn Error>> {
    let (pool, repo, users) = fixture().await?;
    let case = repo.submit(users[0], submission("enterprise")).await?;
    pool.connection().execute_unprepared("CREATE TRIGGER fail_review_audit BEFORE INSERT ON platform_audit_logs BEGIN SELECT RAISE(ABORT, 'audit unavailable'); END").await?;
    assert!(matches!(
        repo.decide(case.id, 1, users[2], 4, None, "rollback".to_owned())
            .await,
        Err(AccountVerificationError::Unavailable)
    ));
    let unchanged = repo.get(Some(users[0]), case.id).await?;
    assert_eq!((unchanged.status, unchanged.version), (1, 1));
    assert!(!repo.enterprise_verified(users[0]).await?);
    pool.close().await?;
    Ok(())
}

#[test]
fn material_validation_rejects_spoofed_types_paths_and_size_mismatches() {
    assert!(validate_verification_material(
        "image/png",
        b"\x89PNG\r\n\x1a\nfixture"
    ));
    assert!(validate_verification_material(
        "image/jpeg",
        &[0xff, 0xd8, 0xff, 0x00]
    ));
    assert!(validate_verification_material(
        "image/webp",
        b"RIFF0000WEBPfixture"
    ));
    assert!(!validate_verification_material(
        "image/png",
        b"%PDF-fixture"
    ));
    assert!(!validate_verification_material("text/html", b"<html>"));
    assert!(!validate_verification_material("application/pdf", b""));
    for mutation in 0..6 {
        let mut write = submission("individual");
        match mutation {
            0 => write.materials.clear(),
            1 => write.materials[0].size_bytes += 1,
            2 => write.materials[0].file_name = "../identity.pdf".to_owned(),
            3 => write.kind = "admin".to_owned(),
            4 => write.subject_name = " ".to_owned(),
            _ => write
                .materials
                .extend((0..5).map(|_| submission("individual").materials.remove(0))),
        }
        assert_eq!(
            validate_submit(&write),
            Err(AccountVerificationError::Invalid)
        );
    }
    let mut oversized = submission("enterprise");
    oversized.materials[0].content_bytes.resize(10_000_001, 0);
    oversized.materials[0].size_bytes = 10_000_001;
    assert_eq!(
        validate_submit(&oversized),
        Err(AccountVerificationError::Invalid)
    );
    let mut aggregate = submission("enterprise");
    aggregate.materials[0].content_bytes.resize(9_000_000, 0);
    aggregate.materials[0].size_bytes = 9_000_000;
    for _ in 0..2 {
        let mut extra = submission("enterprise").materials.remove(0);
        extra.content_bytes.resize(9_000_000, 0);
        extra.size_bytes = 9_000_000;
        aggregate.materials.push(extra);
    }
    assert_eq!(
        validate_submit(&aggregate),
        Err(AccountVerificationError::Invalid)
    );
}
