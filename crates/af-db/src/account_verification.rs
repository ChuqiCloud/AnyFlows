//! Account identity reviews are independent of organization provisioning.
//! Each submission remains immutable; a supplement creates another case.
use crate::{
    DatabasePool, DatabaseTimestamp,
    entity::{
        SensitiveString, account_verification_materials as materials,
        account_verifications as cases, platform_audit_logs, users,
    },
};
use af_domain::{PlatformPermission, UserId};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, EntityTrait, FromQueryResult,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait, sea_query::LockType,
};
use serde::{Deserialize, Serialize};
use std::{future::Future, time::Duration};
use thiserror::Error;

#[cfg(test)]
#[path = "account_verification_tests.rs"]
mod tests;

// AnyFlows authorization window, independent of Alipay's upstream token lifetime.
const ALIPAY_AUTHORIZATION_SECONDS: i64 = 600;

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AccountVerificationError {
    #[error("认证请求无效")]
    Invalid,
    #[error("认证记录不存在")]
    NotFound,
    #[error("认证状态已变更，请刷新后重试")]
    Conflict,
    #[error("认证访问权限不足")]
    Forbidden,
    #[error("不能审核自己的认证记录")]
    SelfReview,
    #[error("认证服务暂不可用")]
    Unavailable,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AccountVerificationRecord {
    pub id: i64,
    pub user_id: i64,
    pub kind: String,
    pub provider: String,
    pub(crate) provider_reference: Option<String>,
    pub provider_action_url: Option<String>,
    pub provider_expires_at: Option<i64>,
    pub provider_status: Option<String>,
    pub document_country: String,
    pub document_type: String,
    pub document_number_masked: Option<String>,
    pub subject_name: String,
    pub summary: String,
    pub status: i16,
    pub version: i64,
    pub review_reason: Option<String>,
    pub reviewer_user_id: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 外部实名认证 provider 初始化时接收的最小业务数据。
///
/// 该类型只在应用服务与 provider 边界之间传递，不直接暴露给 HTTP 层。
#[derive(Clone)]
pub struct AccountVerificationProviderRequest {
    pub kind: String,
    pub document_country: String,
    pub document_type: String,
    pub document_number: String,
    pub subject_name: String,
}

#[derive(Clone)]
pub struct AccountVerificationProviderStart {
    pub reference: String,
    pub action_url: String,
    pub status: String,
}

#[derive(Clone)]
pub struct AccountVerificationProviderResult {
    pub status: String,
    pub terminal_status: Option<i16>,
    pub reason: Option<String>,
}

#[derive(Clone, FromQueryResult, Serialize)]
pub struct AccountVerificationMaterial {
    pub id: i64,
    pub case_id: i64,
    pub kind: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
}

pub struct AccountVerificationSubmit {
    pub kind: String,
    pub provider: String,
    pub provider_reference: Option<String>,
    pub provider_action_url: Option<String>,
    pub provider_status: Option<String>,
    pub document_country: String,
    pub document_type: String,
    pub document_number: Option<String>,
    pub subject_name: String,
    pub summary: String,
    pub materials: Vec<VerificationMaterialWrite>,
}

/// Uploaded evidence shared by identity providers and manual account reviews.
pub struct VerificationMaterialWrite {
    pub kind: String,
    pub object_reference: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub content_bytes: Vec<u8>,
}

#[derive(Clone)]
pub struct AccountVerificationRepository {
    pool: DatabasePool,
}

pub fn validate_account_verification_submit(
    write: &AccountVerificationSubmit,
) -> Result<(), AccountVerificationError> {
    validate_submit(write)
}

impl AccountVerificationRepository {
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    async fn run<T>(
        &self,
        task: impl Future<Output = Result<T, AccountVerificationError>>,
    ) -> Result<T, AccountVerificationError> {
        tokio::time::timeout(Duration::from_secs(30), task)
            .await
            .map_err(|_| AccountVerificationError::Unavailable)?
    }

    pub async fn enterprise_verified(
        &self,
        user: UserId,
    ) -> Result<bool, AccountVerificationError> {
        self.run(has_enterprise_verification(self.pool.connection(), user))
            .await
    }

    pub async fn list(
        &self,
        user: Option<UserId>,
        before: Option<i64>,
        status: Option<i16>,
        limit: u64,
    ) -> Result<Vec<AccountVerificationRecord>, AccountVerificationError> {
        self.run(async {
            if !(1..=50).contains(&limit)
                || before.is_some_and(|v| v <= 0)
                || status.is_some_and(|v| ![1, 3, 4, 5].contains(&v))
            {
                return Err(AccountVerificationError::Invalid);
            }
            let mut query = cases::Entity::find();
            if let Some(user) = user {
                query = query.filter(cases::Column::UserId.eq(user.get()));
            }
            if let Some(before) = before {
                query = query.filter(cases::Column::Id.lt(before));
            }
            if let Some(status) = status {
                // Expiry is exposed in whole Unix seconds. Use an exclusive next-second
                // bound so timestamp fractions cannot put expired rows in the pending page.
                let cutoff = DatabaseTimestamp::from_unix_timestamp(
                    DatabaseTimestamp::now_utc().unix_timestamp() - ALIPAY_AUTHORIZATION_SECONDS
                        + 1,
                )
                .map_err(|_| AccountVerificationError::Unavailable)?;
                let expired = Condition::all()
                    .add(cases::Column::Status.eq(1))
                    .add(cases::Column::Provider.eq("alipay"))
                    .add(cases::Column::CreatedAt.lt(cutoff));
                query = match status {
                    1 => query.filter(cases::Column::Status.eq(1)).filter(
                        Condition::any()
                            .add(cases::Column::Provider.ne("alipay"))
                            .add(cases::Column::CreatedAt.gte(cutoff)),
                    ),
                    5 => query.filter(
                        Condition::any()
                            .add(cases::Column::Status.eq(5))
                            .add(expired),
                    ),
                    _ => query.filter(cases::Column::Status.eq(status)),
                };
            }
            Ok(query
                .order_by_desc(cases::Column::Id)
                .limit(limit)
                .all(self.pool.connection())
                .await
                .map_err(unavailable)?
                .into_iter()
                .map(record)
                .collect())
        })
        .await
    }

    pub async fn get(
        &self,
        user: Option<UserId>,
        id: i64,
    ) -> Result<AccountVerificationRecord, AccountVerificationError> {
        self.run(load_case(self.pool.connection(), user, id)).await
    }

    pub async fn provider_reference(
        &self,
        user: UserId,
        id: i64,
    ) -> Result<(String, String), AccountVerificationError> {
        self.run(async {
            let case = cases::Entity::find_by_id(id)
                .filter(cases::Column::UserId.eq(user.get()))
                .one(self.pool.connection())
                .await
                .map_err(unavailable)?
                .ok_or(AccountVerificationError::NotFound)?;
            let reference = case
                .provider_reference
                .filter(|value| !value.is_empty())
                .ok_or(AccountVerificationError::Invalid)?;
            Ok((case.provider, reference))
        })
        .await
    }

    /// Locate an Alipay CertDoc case by its OAuth state without exposing the
    /// provider reference to the HTTP layer. The state is a high-entropy,
    /// single-flow capability, so the callback does not need a browser bearer
    /// token after the provider redirects back.
    pub async fn provider_reference_by_state(
        &self,
        state: &str,
    ) -> Result<(UserId, i64, String, String), AccountVerificationError> {
        if state.len() != 32 || !state.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(AccountVerificationError::Invalid);
        }
        self.run(async {
            let case = cases::Entity::find()
                .filter(cases::Column::Provider.eq("alipay"))
                .filter(cases::Column::ProviderReference.ends_with(format!(".{state}")))
                .one(self.pool.connection())
                .await
                .map_err(unavailable)?
                .ok_or(AccountVerificationError::NotFound)?;
            if case.status != 1 || alipay_expired(&case, DatabaseTimestamp::now_utc()) {
                return Err(AccountVerificationError::Conflict);
            }
            let reference = case
                .provider_reference
                .ok_or(AccountVerificationError::Invalid)?;
            let user = UserId::new(case.user_id).map_err(|_| AccountVerificationError::Invalid)?;
            Ok((user, case.id, case.provider, reference))
        })
        .await
    }

    /// Avoid starting another upstream flow for an application that cannot be submitted.
    /// submit() repeats this check under the user lock to handle concurrent requests.
    pub async fn ensure_can_submit(
        &self,
        user: UserId,
        kind: &str,
    ) -> Result<(), AccountVerificationError> {
        self.run(async {
            let current = cases::Entity::find()
                .filter(cases::Column::UserId.eq(user.get()))
                .filter(cases::Column::Kind.eq(kind))
                .order_by_desc(cases::Column::Id)
                .one(self.pool.connection())
                .await
                .map_err(unavailable)?;
            if current
                .as_ref()
                .is_some_and(|c| blocks_submission(c, DatabaseTimestamp::now_utc()))
            {
                return Err(AccountVerificationError::Conflict);
            }
            Ok(())
        })
        .await
    }

    pub async fn materials(
        &self,
        user: Option<UserId>,
        id: i64,
    ) -> Result<Vec<AccountVerificationMaterial>, AccountVerificationError> {
        self.run(async {
            load_case(self.pool.connection(), user, id).await?;
            materials::Entity::find()
                .select_only()
                .columns([
                    materials::Column::Id,
                    materials::Column::CaseId,
                    materials::Column::Kind,
                    materials::Column::FileName,
                    materials::Column::ContentType,
                    materials::Column::SizeBytes,
                ])
                .filter(materials::Column::CaseId.eq(id))
                .order_by_asc(materials::Column::Id)
                .into_model::<AccountVerificationMaterial>()
                .all(self.pool.connection())
                .await
                .map_err(unavailable)
        })
        .await
    }

    pub async fn download(
        &self,
        user: Option<UserId>,
        id: i64,
        material_id: i64,
    ) -> Result<(String, String, Vec<u8>), AccountVerificationError> {
        self.run(async {
            load_case(self.pool.connection(), user, id).await?;
            let item = materials::Entity::find_by_id(material_id)
                .filter(materials::Column::CaseId.eq(id))
                .one(self.pool.connection())
                .await
                .map_err(unavailable)?
                .ok_or(AccountVerificationError::NotFound)?;
            Ok((
                item.file_name.as_str().to_owned(),
                item.content_type,
                item.content_bytes,
            ))
        })
        .await
    }

    pub async fn submit(
        &self,
        user: UserId,
        write: AccountVerificationSubmit,
    ) -> Result<AccountVerificationRecord, AccountVerificationError> {
        validate_submit(&write)?;
        self.run(async {
            let tx = self.pool.connection().begin().await.map_err(unavailable)?;
            let applicant = users::Entity::find_by_id(user.get())
                .lock(LockType::Update)
                .one(&tx)
                .await
                .map_err(unavailable)?
                .ok_or(AccountVerificationError::Forbidden)?;
            if applicant.deleted_at.is_some() || applicant.status != 1 {
                return Err(AccountVerificationError::Forbidden);
            }
            let current = cases::Entity::find()
                .filter(cases::Column::UserId.eq(user.get()))
                .filter(cases::Column::Kind.eq(&write.kind))
                .order_by_desc(cases::Column::Id)
                .lock(LockType::Update)
                .one(&tx)
                .await
                .map_err(unavailable)?;
            let now = DatabaseTimestamp::now_utc();
            if current.as_ref().is_some_and(|c| blocks_submission(c, now)) {
                return Err(AccountVerificationError::Conflict);
            }
            if let Some(expired) = current.filter(|c| c.status == 1 && alipay_expired(c, now)) {
                cases::ActiveModel {
                    id: Set(expired.id),
                    status: Set(5),
                    provider_status: Set(Some("expired".to_owned())),
                    provider_action_url: Set(None),
                    version: Set(expired.version + 1),
                    updated_at: Set(now),
                    ..Default::default()
                }
                .update(&tx)
                .await
                .map_err(unavailable)?;
            }
            let case = cases::ActiveModel {
                user_id: Set(user.get()),
                kind: Set(write.kind),
                provider: Set(write.provider),
                provider_reference: Set(write.provider_reference),
                provider_action_url: Set(write.provider_action_url),
                provider_status: Set(write.provider_status),
                document_country: Set(write.document_country),
                document_type: Set(write.document_type),
                document_number_masked: Set(write
                    .document_number
                    .as_deref()
                    .map(mask_document_number)),
                subject_name: Set(SensitiveString::from(write.subject_name)),
                summary: Set(SensitiveString::from(write.summary)),
                status: Set(1),
                version: Set(1),
                reviewer_user_id: Set(None),
                review_reason: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            }
            .insert(&tx)
            .await
            .map_err(unavailable)?;
            for item in write.materials {
                materials::ActiveModel {
                    case_id: Set(case.id),
                    kind: Set(item.kind),
                    file_name: Set(SensitiveString::from(item.file_name)),
                    content_type: Set(item.content_type),
                    size_bytes: Set(item.size_bytes),
                    content_bytes: Set(item.content_bytes),
                    ..Default::default()
                }
                .insert(&tx)
                .await
                .map_err(unavailable)?;
            }
            tx.commit().await.map_err(unavailable)?;
            Ok(record(case))
        })
        .await
    }

    /// 保存外部实名认证 provider 的查询结果，并在 provider 已给出终态时收敛认证状态。
    pub async fn apply_provider_result(
        &self,
        user: UserId,
        id: i64,
        provider_status: String,
        terminal_status: Option<i16>,
        reason: Option<String>,
    ) -> Result<AccountVerificationRecord, AccountVerificationError> {
        if id <= 0
            || !valid_text(&provider_status, 32)
            || terminal_status.is_some_and(|status| ![4, 5].contains(&status))
            || reason
                .as_deref()
                .is_some_and(|value| !valid_text(value, 512))
        {
            return Err(AccountVerificationError::Invalid);
        }
        self.run(async {
            let tx = self.pool.connection().begin().await.map_err(unavailable)?;
            let case = cases::Entity::find_by_id(id)
                .filter(cases::Column::UserId.eq(user.get()))
                .lock(LockType::Update)
                .one(&tx)
                .await
                .map_err(unavailable)?
                .ok_or(AccountVerificationError::NotFound)?;
            if case.provider == "manual"
                || ![1, 3].contains(&case.status)
                || alipay_expired(&case, DatabaseTimestamp::now_utc())
            {
                return Err(AccountVerificationError::Conflict);
            }
            let now = DatabaseTimestamp::now_utc();
            let updated = cases::ActiveModel {
                id: Set(id),
                provider_status: Set(Some(provider_status)),
                status: Set(terminal_status.unwrap_or(case.status)),
                version: Set(if terminal_status.is_some() {
                    case.version + 1
                } else {
                    case.version
                }),
                review_reason: Set(reason.map(SensitiveString::from)),
                updated_at: Set(now),
                ..Default::default()
            }
            .update(&tx)
            .await
            .map_err(unavailable)?;
            tx.commit().await.map_err(unavailable)?;
            Ok(record(updated))
        })
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn decide(
        &self,
        id: i64,
        version: i64,
        reviewer: UserId,
        status: i16,
        reason: Option<String>,
        request_id: String,
    ) -> Result<AccountVerificationRecord, AccountVerificationError> {
        if id <= 0
            || version <= 0
            || ![3, 4, 5].contains(&status)
            || reason.as_deref().is_some_and(|v| !valid_text(v, 512))
            || (status != 4 && reason.is_none())
        {
            return Err(AccountVerificationError::Invalid);
        }
        self.run(async {
            let tx = self.pool.connection().begin().await.map_err(unavailable)?;
            let admin = users::Entity::find_by_id(reviewer.get())
                .lock(LockType::Update)
                .one(&tx)
                .await
                .map_err(unavailable)?
                .ok_or(AccountVerificationError::Forbidden)?;
            if admin.deleted_at.is_some() || admin.status != 1 || admin.role != 1 {
                return Err(AccountVerificationError::Forbidden);
            }
            let case = cases::Entity::find_by_id(id)
                .lock(LockType::Update)
                .one(&tx)
                .await
                .map_err(unavailable)?
                .ok_or(AccountVerificationError::NotFound)?;
            if case.status != 1 || case.version != version || case.provider != "manual" {
                return Err(AccountVerificationError::Conflict);
            }
            if case.user_id == reviewer.get() {
                return Err(AccountVerificationError::SelfReview);
            }
            let now = DatabaseTimestamp::now_utc();
            let updated = cases::ActiveModel {
                id: Set(id),
                status: Set(status),
                version: Set(version + 1),
                reviewer_user_id: Set(Some(reviewer.get())),
                review_reason: Set(reason.map(SensitiveString::from)),
                updated_at: Set(now),
                ..Default::default()
            }
            .update(&tx)
            .await
            .map_err(unavailable)?;
            platform_audit_logs::ActiveModel {
                operator_user_id: Set(reviewer.get()),
                permission_code: Set(PlatformPermission::AccountVerificationsManage
                    .code()
                    .to_owned()),
                route: Set("/api/admin/account-verifications/{case_id}/decision".to_owned()),
                operation: Set("decide".to_owned()),
                resource: Set("account_verification".to_owned()),
                resource_id: Set(Some(id.to_string())),
                outcome: Set(1),
                before_value: Set(Some(
                    serde_json::json!({"status":case.status,"version":version}).to_string(),
                )),
                after_value: Set(Some(
                    serde_json::json!({"status":status,"version":version+1}).to_string(),
                )),
                audit_info: Set(None),
                request_id: Set(request_id),
                created_at: Set(now),
                ..Default::default()
            }
            .insert(&tx)
            .await
            .map_err(unavailable)?;
            tx.commit().await.map_err(unavailable)?;
            Ok(record(updated))
        })
        .await
    }
}

pub(crate) async fn has_enterprise_verification<C: ConnectionTrait>(
    db: &C,
    user: UserId,
) -> Result<bool, AccountVerificationError> {
    Ok(cases::Entity::find()
        .filter(cases::Column::UserId.eq(user.get()))
        .filter(cases::Column::Kind.eq("enterprise"))
        .filter(cases::Column::Status.eq(4))
        .one(db)
        .await
        .map_err(unavailable)?
        .is_some())
}

async fn load_case<C: ConnectionTrait>(
    db: &C,
    user: Option<UserId>,
    id: i64,
) -> Result<AccountVerificationRecord, AccountVerificationError> {
    let mut query = cases::Entity::find_by_id(id);
    if let Some(user) = user {
        query = query.filter(cases::Column::UserId.eq(user.get()));
    }
    query
        .one(db)
        .await
        .map_err(unavailable)?
        .map(record)
        .ok_or(AccountVerificationError::NotFound)
}

fn record(c: cases::Model) -> AccountVerificationRecord {
    let expired = c.status == 1 && alipay_expired(&c, DatabaseTimestamp::now_utc());
    let provider_expires_at = (c.provider == "alipay")
        .then(|| c.created_at.unix_timestamp() + ALIPAY_AUTHORIZATION_SECONDS);
    let actionable = c.status == 1 && !expired;
    AccountVerificationRecord {
        id: c.id,
        user_id: c.user_id,
        kind: c.kind,
        provider: c.provider,
        provider_reference: c.provider_reference,
        provider_action_url: c.provider_action_url.filter(|_| actionable),
        provider_expires_at,
        provider_status: if expired {
            Some("expired".to_owned())
        } else {
            c.provider_status
        },
        document_country: c.document_country,
        document_type: c.document_type,
        document_number_masked: c.document_number_masked,
        subject_name: c.subject_name.as_str().to_owned(),
        summary: c.summary.as_str().to_owned(),
        status: if expired { 5 } else { c.status },
        version: c.version,
        review_reason: c.review_reason.map(|r| r.as_str().to_owned()),
        reviewer_user_id: c.reviewer_user_id,
        created_at: c.created_at.unix_timestamp(),
        updated_at: c.updated_at.unix_timestamp(),
    }
}

fn alipay_expired(case: &cases::Model, now: DatabaseTimestamp) -> bool {
    case.provider == "alipay"
        && now.unix_timestamp() >= case.created_at.unix_timestamp() + ALIPAY_AUTHORIZATION_SECONDS
}

fn blocks_submission(case: &cases::Model, now: DatabaseTimestamp) -> bool {
    case.status == 4 || (case.status == 1 && !alipay_expired(case, now))
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn unavailable(_: sea_orm::DbErr) -> AccountVerificationError {
    AccountVerificationError::Unavailable
}

pub fn validate_verification_material(content_type: &str, content: &[u8]) -> bool {
    !content.is_empty()
        && content.len() <= 10_000_000
        && match content_type {
            "image/png" => content.starts_with(b"\x89PNG\r\n\x1a\n"),
            "image/jpeg" => content.starts_with(&[0xff, 0xd8, 0xff]),
            "image/webp" => content.starts_with(b"RIFF") && content.get(8..12) == Some(b"WEBP"),
            "application/pdf" => content.starts_with(b"%PDF-"),
            _ => false,
        }
}

fn validate_submit(write: &AccountVerificationSubmit) -> Result<(), AccountVerificationError> {
    if !["individual", "enterprise"].contains(&write.kind.as_str())
        || !["manual", "alipay"].contains(&write.provider.as_str())
        || write.document_country.len() != 2
        || !write
            .document_country
            .bytes()
            .all(|b| b.is_ascii_uppercase())
        || ![
            "identity",
            "national_id",
            "passport",
            "residence_permit",
            "foreign_passport",
            "hkm_macao_pass",
            "taiwan_pass",
            "business_registration",
            "other",
        ]
        .contains(&write.document_type.as_str())
        || (write.kind == "enterprise"
            && !["identity", "business_registration"].contains(&write.document_type.as_str()))
        || (write.provider == "alipay"
            && (write.kind != "individual"
                || write.document_country != "CN"
                || write.document_type != "national_id"
                || write.document_number.is_none()))
        || write.document_number.as_deref().is_some_and(|v| {
            !valid_document_number(&write.document_country, &write.document_type, v)
        })
        || !valid_text(&write.subject_name, 128)
        || write.summary.len() > 512
        || (!write.summary.is_empty() && !valid_text(&write.summary, 512))
        || write
            .provider_reference
            .as_deref()
            .is_some_and(|value| !valid_text(value, 128))
        || write
            .provider_action_url
            .as_deref()
            .is_some_and(|value| !valid_text(value, 2_048))
        || write
            .provider_status
            .as_deref()
            .is_some_and(|value| !valid_text(value, 32))
        || if write.provider == "alipay" {
            !write.materials.is_empty()
        } else {
            !(1..=5).contains(&write.materials.len())
        }
    {
        return Err(AccountVerificationError::Invalid);
    }
    let mut total = 0;
    for item in &write.materials {
        if !valid_text(&item.kind, 64)
            || !valid_text(&item.file_name, 255)
            || item.file_name.contains(['/', '\\'])
            || item.size_bytes != item.content_bytes.len() as i64
            || !validate_verification_material(&item.content_type, &item.content_bytes)
        {
            return Err(AccountVerificationError::Invalid);
        }
        total += item.content_bytes.len();
    }
    if total > 25_000_000 {
        return Err(AccountVerificationError::Invalid);
    }
    Ok(())
}

fn valid_document_number(country: &str, document_type: &str, value: &str) -> bool {
    let value = value.trim();
    if country == "CN" && document_type == "national_id" {
        let bytes = value.as_bytes();
        if bytes.len() != 18 || !bytes[..17].iter().all(u8::is_ascii_digit) {
            return false;
        }
        const WEIGHTS: [u32; 17] = [7, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2];
        const CHECK: &[u8; 11] = b"10X98765432";
        let sum: u32 = bytes[..17]
            .iter()
            .zip(WEIGHTS)
            .map(|(digit, weight)| u32::from(*digit - b'0') * weight)
            .sum();
        return bytes[17].to_ascii_uppercase() == CHECK[(sum % 11) as usize];
    }
    (5..=64).contains(&value.chars().count())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | ' '))
}

fn mask_document_number(value: &str) -> String {
    let compact: Vec<char> = value.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() <= 4 {
        return "****".to_owned();
    }
    let prefix: String = compact.iter().take(2).collect();
    let suffix: String = compact
        .iter()
        .rev()
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}****{suffix}")
}
