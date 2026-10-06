use std::fmt;

use af_domain::ModelId;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseTransaction, DbErr, EntityTrait, QueryFilter, Set,
    TransactionTrait, entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AdminModelLifecycleRecord, AdminModelModalitiesRecord, AdminModelRecord, AdminModelRepository,
    AdminModelRepositoryError, AdminModelVisibilityRecord, MAX_ADMIN_MODEL_DESCRIPTION_BYTES,
    MAX_ADMIN_MODEL_DISPLAY_NAME_BYTES, MAX_ADMIN_MODEL_PROVIDER_BYTES,
    admin_model::{
        record_internal_error, valid_canonical_model, valid_context_window,
        valid_optional_icon_url, valid_optional_text, valid_tags, valid_text,
    },
    entity::models,
};

/// 管理端创建模型商品元数据时写入的完整记录。
pub struct AdminModelCreateRecord {
    model: String,
    fields: AdminModelWriteRecord,
}

impl AdminModelCreateRecord {
    /// 组装已经由应用层完成公开边界校验的创建记录。
    #[must_use]
    pub fn new(model: String, fields: AdminModelWriteRecord) -> Self {
        Self { model, fields }
    }
}

impl fmt::Debug for AdminModelCreateRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminModelCreateRecord(<redacted>)")
    }
}

/// 管理端完整更新模型商品元数据时允许覆盖的字段。
pub struct AdminModelWriteRecord {
    pub(super) display_name: String,
    pub(super) provider: String,
    pub(super) description: Option<String>,
    pub(super) icon_url: Option<String>,
    pub(super) tags: Vec<String>,
    pub(super) context_window: Option<i64>,
    pub(super) input_modalities: AdminModelModalitiesRecord,
    pub(super) output_modalities: AdminModelModalitiesRecord,
    pub(super) supports_reasoning: bool,
    pub(super) supports_tool_calls: bool,
    pub(super) visibility: AdminModelVisibilityRecord,
    pub(super) lifecycle: AdminModelLifecycleRecord,
}

impl AdminModelWriteRecord {
    /// 组装已经由应用层校验的可变模型元数据字段。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与模型元数据写入契约一一对应"
    )]
    #[must_use]
    pub fn new(
        display_name: String,
        provider: String,
        description: Option<String>,
        icon_url: Option<String>,
        tags: Vec<String>,
        context_window: Option<i64>,
        input_modalities: AdminModelModalitiesRecord,
        output_modalities: AdminModelModalitiesRecord,
        supports_reasoning: bool,
        supports_tool_calls: bool,
        visibility: AdminModelVisibilityRecord,
        lifecycle: AdminModelLifecycleRecord,
    ) -> Self {
        Self {
            display_name,
            provider,
            description,
            icon_url,
            tags,
            context_window,
            input_modalities,
            output_modalities,
            supports_reasoning,
            supports_tool_calls,
            visibility,
            lifecycle,
        }
    }
}

impl fmt::Debug for AdminModelWriteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminModelWriteRecord(<redacted>)")
    }
}

/// 模型元数据更新结果；不存在和已软删除统一视为未找到。
pub enum AdminModelMutationOutcome {
    /// 元数据已更新，并返回最新管理快照。
    Mutated(AdminModelRecord),
    /// 元数据不存在或已经软删除。
    NotFound,
}

impl fmt::Debug for AdminModelMutationOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mutated(_) => {
                formatter.write_str("AdminModelMutationOutcome::Mutated(<redacted>)")
            }
            Self::NotFound => formatter.write_str("AdminModelMutationOutcome::NotFound"),
        }
    }
}

/// 模型元数据软删除结果；重复删除不会伪装成成功。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminModelDeleteOutcome {
    /// 仅模型商品元数据已写入墓碑，其他模型域数据保持不变。
    Deleted,
    /// 元数据不存在或已经软删除。
    NotFound,
}

impl AdminModelRepository {
    /// 创建独立模型元数据并返回管理快照。
    pub async fn create(
        &self,
        record: AdminModelCreateRecord,
    ) -> Result<AdminModelRecord, AdminModelRepositoryError> {
        match timeout(self.lookup_timeout, self.create_inner(record)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(AdminModelRepositoryError::Timeout)),
        }
    }

    /// 完整更新可变元数据；Canonical 模型标识保持不可变。
    pub async fn update(
        &self,
        model_id: ModelId,
        record: AdminModelWriteRecord,
    ) -> Result<AdminModelMutationOutcome, AdminModelRepositoryError> {
        match timeout(self.lookup_timeout, self.update_inner(model_id, record)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(AdminModelRepositoryError::Timeout)),
        }
    }

    /// 软删除模型元数据，不触碰价格、渠道能力或映射记录。
    pub async fn delete(
        &self,
        model_id: ModelId,
    ) -> Result<AdminModelDeleteOutcome, AdminModelRepositoryError> {
        match timeout(self.lookup_timeout, self.delete_inner(model_id)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(AdminModelRepositoryError::Timeout)),
        }
    }

    async fn create_inner(
        &self,
        record: AdminModelCreateRecord,
    ) -> Result<AdminModelRecord, AdminModelRepositoryError> {
        if !valid_canonical_model(&record.model) || !valid_write_fields(&record.fields) {
            return Err(record_internal_error(AdminModelRepositoryError::Invariant));
        }
        let transaction = begin_transaction(self).await?;
        ensure_model_available(&transaction, &record.model).await?;
        let inserted = active_model_from_write(record.model, record.fields)?
            .insert(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?;
        let snapshot = AdminModelRecord::try_from_model(inserted)?;
        commit_transaction(transaction).await?;
        Ok(snapshot)
    }

    async fn update_inner(
        &self,
        model_id: ModelId,
        fields: AdminModelWriteRecord,
    ) -> Result<AdminModelMutationOutcome, AdminModelRepositoryError> {
        if !valid_write_fields(&fields) {
            return Err(record_internal_error(AdminModelRepositoryError::Invariant));
        }
        let transaction = begin_transaction(self).await?;
        if !active_model_exists(&transaction, model_id).await? {
            return Ok(AdminModelMutationOutcome::NotFound);
        }
        let now = TimeDateTimeWithTimeZone::now_utc();
        let result = models::Entity::update_many()
            .filter(models::Column::Id.eq(model_id.get()))
            .filter(models::Column::DeletedAt.is_null())
            .col_expr(
                models::Column::DisplayName,
                Expr::value(fields.display_name),
            )
            .col_expr(models::Column::Provider, Expr::value(fields.provider))
            .col_expr(models::Column::Description, Expr::value(fields.description))
            .col_expr(models::Column::IconUrl, Expr::value(fields.icon_url))
            .col_expr(
                models::Column::Tags,
                Expr::value(serde_json::to_value(fields.tags).map_err(|_| internal_invariant())?),
            )
            .col_expr(
                models::Column::ContextWindow,
                Expr::value(fields.context_window),
            )
            .col_expr(
                models::Column::SupportsTextInput,
                Expr::value(fields.input_modalities.text()),
            )
            .col_expr(
                models::Column::SupportsImageInput,
                Expr::value(fields.input_modalities.image()),
            )
            .col_expr(
                models::Column::SupportsAudioInput,
                Expr::value(fields.input_modalities.audio()),
            )
            .col_expr(
                models::Column::SupportsVideoInput,
                Expr::value(fields.input_modalities.video()),
            )
            .col_expr(
                models::Column::SupportsTextOutput,
                Expr::value(fields.output_modalities.text()),
            )
            .col_expr(
                models::Column::SupportsImageOutput,
                Expr::value(fields.output_modalities.image()),
            )
            .col_expr(
                models::Column::SupportsAudioOutput,
                Expr::value(fields.output_modalities.audio()),
            )
            .col_expr(
                models::Column::SupportsVideoOutput,
                Expr::value(fields.output_modalities.video()),
            )
            .col_expr(
                models::Column::SupportsReasoning,
                Expr::value(fields.supports_reasoning),
            )
            .col_expr(
                models::Column::SupportsToolCalls,
                Expr::value(fields.supports_tool_calls),
            )
            .col_expr(
                models::Column::Visibility,
                Expr::value(fields.visibility.database_value()),
            )
            .col_expr(
                models::Column::Lifecycle,
                Expr::value(fields.lifecycle.database_value()),
            )
            .col_expr(models::Column::UpdatedAt, Expr::value(now))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?;
        if result.rows_affected != 1 {
            return Err(internal_invariant());
        }
        let snapshot = fetch_model_snapshot(&transaction, model_id).await?;
        commit_transaction(transaction).await?;
        Ok(AdminModelMutationOutcome::Mutated(snapshot))
    }

    async fn delete_inner(
        &self,
        model_id: ModelId,
    ) -> Result<AdminModelDeleteOutcome, AdminModelRepositoryError> {
        let transaction = begin_transaction(self).await?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let result = models::Entity::update_many()
            .filter(models::Column::Id.eq(model_id.get()))
            .filter(models::Column::DeletedAt.is_null())
            .col_expr(models::Column::DeletedAt, Expr::value(now))
            .col_expr(models::Column::UpdatedAt, Expr::value(now))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?;
        match result.rows_affected {
            0 => Ok(AdminModelDeleteOutcome::NotFound),
            1 => {
                commit_transaction(transaction).await?;
                Ok(AdminModelDeleteOutcome::Deleted)
            }
            _ => Err(internal_invariant()),
        }
    }
}

pub(super) fn valid_write_fields(fields: &AdminModelWriteRecord) -> bool {
    valid_text(&fields.display_name, MAX_ADMIN_MODEL_DISPLAY_NAME_BYTES)
        && valid_text(&fields.provider, MAX_ADMIN_MODEL_PROVIDER_BYTES)
        && valid_optional_text(
            fields.description.as_deref(),
            MAX_ADMIN_MODEL_DESCRIPTION_BYTES,
        )
        && valid_optional_icon_url(fields.icon_url.as_deref())
        && valid_tags(&fields.tags)
        && valid_context_window(fields.context_window)
        && !fields.input_modalities.is_empty()
        && !fields.output_modalities.is_empty()
}

/// 将已验证的元数据字段转换为唯一的模型实体写入形状。
pub(super) fn active_model_from_write(
    model: String,
    fields: AdminModelWriteRecord,
) -> Result<models::ActiveModel, AdminModelRepositoryError> {
    if !valid_canonical_model(&model) || !valid_write_fields(&fields) {
        return Err(internal_invariant());
    }
    Ok(models::ActiveModel {
        model: Set(model),
        display_name: Set(fields.display_name),
        provider: Set(fields.provider),
        description: Set(fields.description),
        icon_url: Set(fields.icon_url),
        tags: Set(serde_json::to_value(fields.tags).map_err(|_| internal_invariant())?),
        context_window: Set(fields.context_window),
        supports_text_input: Set(fields.input_modalities.text()),
        supports_image_input: Set(fields.input_modalities.image()),
        supports_audio_input: Set(fields.input_modalities.audio()),
        supports_video_input: Set(fields.input_modalities.video()),
        supports_text_output: Set(fields.output_modalities.text()),
        supports_image_output: Set(fields.output_modalities.image()),
        supports_audio_output: Set(fields.output_modalities.audio()),
        supports_video_output: Set(fields.output_modalities.video()),
        supports_reasoning: Set(fields.supports_reasoning),
        supports_tool_calls: Set(fields.supports_tool_calls),
        visibility: Set(fields.visibility.database_value()),
        lifecycle: Set(fields.lifecycle.database_value()),
        ..Default::default()
    })
}

async fn begin_transaction(
    repository: &AdminModelRepository,
) -> Result<DatabaseTransaction, AdminModelRepositoryError> {
    repository
        .pool
        .connection()
        .begin()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminModelRepositoryError::Query))
}

async fn commit_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), AdminModelRepositoryError> {
    transaction
        .commit()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminModelRepositoryError::Query))
}

async fn active_model_exists(
    transaction: &DatabaseTransaction,
    model_id: ModelId,
) -> Result<bool, AdminModelRepositoryError> {
    models::Entity::find_by_id(model_id.get())
        .filter(models::Column::DeletedAt.is_null())
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map(|row| row.is_some())
        .map_err(|_| record_internal_error(AdminModelRepositoryError::Query))
}

async fn ensure_model_available(
    transaction: &DatabaseTransaction,
    model: &str,
) -> Result<(), AdminModelRepositoryError> {
    let exists = models::Entity::find()
        .filter(models::Column::Model.eq(model))
        .filter(models::Column::DeletedAt.is_null())
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminModelRepositoryError::Query))?
        .is_some();
    if exists {
        Err(AdminModelRepositoryError::Conflict)
    } else {
        Ok(())
    }
}

async fn fetch_model_snapshot(
    transaction: &DatabaseTransaction,
    model_id: ModelId,
) -> Result<AdminModelRecord, AdminModelRepositoryError> {
    let model = models::Entity::find_by_id(model_id.get())
        .filter(models::Column::DeletedAt.is_null())
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminModelRepositoryError::Query))?
        .ok_or_else(internal_invariant)?;
    AdminModelRecord::try_from_model(model)
}

pub(super) fn map_write_db_error(error: DbErr) -> AdminModelRepositoryError {
    let rendered = error.to_string();
    if rendered.contains("uq_models_active_model")
        || rendered.contains("_active_model_hash")
        || rendered.contains("models.model")
    {
        return AdminModelRepositoryError::Conflict;
    }
    record_internal_error(AdminModelRepositoryError::Query)
}

fn internal_invariant() -> AdminModelRepositoryError {
    record_internal_error(AdminModelRepositoryError::Invariant)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_records_hide_model_business_fields_from_debug() {
        let modalities = AdminModelModalitiesRecord::new(true, true, false, false);
        let fields = AdminModelWriteRecord::new(
            "Private Model".to_owned(),
            "private-provider".to_owned(),
            Some("private-description".to_owned()),
            None,
            vec!["private-tag".to_owned()],
            Some(128_000),
            modalities,
            AdminModelModalitiesRecord::new(true, false, false, false),
            true,
            true,
            AdminModelVisibilityRecord::Hidden,
            AdminModelLifecycleRecord::Draft,
        );
        let create = AdminModelCreateRecord::new("private-model".to_owned(), fields);
        assert_eq!(format!("{create:?}"), "AdminModelCreateRecord(<redacted>)");
    }
}
