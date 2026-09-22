use std::{fmt, time::Duration};

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QuerySelect, Set, TransactionTrait,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};
use url::Url;

use crate::{DatabasePool, entity::site_settings};

const SITE_SETTINGS_ID: i16 = 1;
const MAX_SITE_NAME_BYTES: usize = 80;
const MAX_URL_BYTES: usize = 2_048;
const MAX_BRAND_TAGLINE_BYTES: usize = 160;
const MAX_BRAND_DESCRIPTION_BYTES: usize = 500;
const MAX_FRONTEND_TEMPLATE_ID_BYTES: usize = 128;
const MAX_BALANCE_UNIT_NAME_BYTES: usize = 64;
const MAX_BALANCE_UNIT_SYMBOL_BYTES: usize = 24;
const MAX_BALANCE_FRACTION_DIGITS: u8 = 4;

/// 余额数值采用原始额度或自定义面额展示。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum BalanceDisplayModeRecord {
    Quota = 1,
    CustomUnit = 2,
}

impl TryFrom<i16> for BalanceDisplayModeRecord {
    type Error = SiteSettingsRepositoryError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Quota),
            2 => Ok(Self::CustomUnit),
            _ => Err(SiteSettingsRepositoryError::Invariant),
        }
    }
}

/// 自定义单位符号相对数值的展示位置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum BalanceSymbolPositionRecord {
    Prefix = 1,
    Suffix = 2,
}

impl TryFrom<i16> for BalanceSymbolPositionRecord {
    type Error = SiteSettingsRepositoryError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Prefix),
            2 => Ok(Self::Suffix),
            _ => Err(SiteSettingsRepositoryError::Invariant),
        }
    }
}

/// 只改变界面格式化结果的余额展示策略，不参与任何计费运算。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BalanceDisplayPolicyRecord {
    mode: BalanceDisplayModeRecord,
    unit_name: String,
    unit_symbol: String,
    quota_units_per_display_unit: i64,
    symbol_position: BalanceSymbolPositionRecord,
    fraction_digits: u8,
}

impl BalanceDisplayPolicyRecord {
    /// 创建并校验完整展示策略。
    pub fn new(
        mode: BalanceDisplayModeRecord,
        unit_name: String,
        unit_symbol: String,
        quota_units_per_display_unit: i64,
        symbol_position: BalanceSymbolPositionRecord,
        fraction_digits: u8,
    ) -> Result<Self, SiteSettingsRepositoryError> {
        let policy = Self {
            mode,
            unit_name,
            unit_symbol,
            quota_units_per_display_unit,
            symbol_position,
            fraction_digits,
        };
        policy.validate()?;
        Ok(policy)
    }

    #[must_use]
    pub const fn mode(&self) -> BalanceDisplayModeRecord {
        self.mode
    }

    #[must_use]
    pub fn unit_name(&self) -> &str {
        &self.unit_name
    }

    #[must_use]
    pub fn unit_symbol(&self) -> &str {
        &self.unit_symbol
    }

    #[must_use]
    pub const fn quota_units_per_display_unit(&self) -> i64 {
        self.quota_units_per_display_unit
    }

    #[must_use]
    pub const fn symbol_position(&self) -> BalanceSymbolPositionRecord {
        self.symbol_position
    }

    #[must_use]
    pub const fn fraction_digits(&self) -> u8 {
        self.fraction_digits
    }

    fn validate(&self) -> Result<(), SiteSettingsRepositoryError> {
        if !valid_balance_unit(&self.unit_name, MAX_BALANCE_UNIT_NAME_BYTES)
            || !valid_balance_unit(&self.unit_symbol, MAX_BALANCE_UNIT_SYMBOL_BYTES)
            || self.quota_units_per_display_unit <= 0
            || self.fraction_digits > MAX_BALANCE_FRACTION_DIGITS
        {
            return Err(SiteSettingsRepositoryError::InvalidSettings);
        }
        Ok(())
    }
}

/// 已完成持久化校验的站点身份与公开品牌设置。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SiteSettingsRecord {
    site_name: String,
    public_base_url: Option<String>,
    brand_logo_url: Option<String>,
    brand_tagline: Option<String>,
    brand_description: Option<String>,
    frontend_template_id: Option<String>,
    balance_display: BalanceDisplayPolicyRecord,
    version: i64,
}

impl SiteSettingsRecord {
    #[must_use]
    pub fn site_name(&self) -> &str {
        &self.site_name
    }

    #[must_use]
    pub fn public_base_url(&self) -> Option<&str> {
        self.public_base_url.as_deref()
    }

    #[must_use]
    pub fn brand_logo_url(&self) -> Option<&str> {
        self.brand_logo_url.as_deref()
    }

    #[must_use]
    pub fn brand_tagline(&self) -> Option<&str> {
        self.brand_tagline.as_deref()
    }

    #[must_use]
    pub fn brand_description(&self) -> Option<&str> {
        self.brand_description.as_deref()
    }

    /// 返回当前前端模板 ID；None 表示 Classic，embedded-next 表示 Next。
    #[must_use]
    pub fn frontend_template_id(&self) -> Option<&str> {
        self.frontend_template_id.as_deref()
    }

    #[must_use]
    pub const fn balance_display(&self) -> &BalanceDisplayPolicyRecord {
        &self.balance_display
    }

    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

/// 管理员完整覆盖站点设置时使用的强类型写入记录。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SiteSettingsWriteRecord {
    site_name: String,
    public_base_url: Option<String>,
    brand_logo_url: Option<String>,
    brand_tagline: Option<String>,
    brand_description: Option<String>,
    frontend_template_id: Option<String>,
    balance_display: BalanceDisplayPolicyRecord,
    expected_version: i64,
}

impl SiteSettingsWriteRecord {
    /// 组合已由应用层规范化的完整站点设置。
    #[must_use]
    pub fn new(
        site_name: String,
        public_base_url: Option<String>,
        brand_logo_url: Option<String>,
        brand_tagline: Option<String>,
        brand_description: Option<String>,
        balance_display: BalanceDisplayPolicyRecord,
        expected_version: i64,
    ) -> Self {
        Self {
            site_name,
            public_base_url,
            brand_logo_url,
            brand_tagline,
            brand_description,
            frontend_template_id: None,
            balance_display,
            expected_version,
        }
    }

    /// 设置前端模板 ID；None 表示恢复 Classic，embedded-next 表示 Next。
    #[must_use]
    pub fn with_frontend_template_id(mut self, template_id: Option<String>) -> Self {
        self.frontend_template_id = template_id;
        self
    }

    fn validate(&self) -> Result<(), SiteSettingsRepositoryError> {
        if !valid_site_name(&self.site_name)
            || self
                .public_base_url
                .as_deref()
                .is_some_and(|value| !valid_public_base_url(value))
            || self
                .brand_logo_url
                .as_deref()
                .is_some_and(|value| !valid_brand_asset_url(value))
            || !valid_optional_single_line(self.brand_tagline.as_deref(), MAX_BRAND_TAGLINE_BYTES)
            || !valid_optional_description(self.brand_description.as_deref())
            || !valid_frontend_template_id(self.frontend_template_id.as_deref())
            || self.expected_version < 1
        {
            return Err(SiteSettingsRepositoryError::InvalidSettings);
        }
        self.balance_display.validate()?;
        Ok(())
    }
}

/// 站点设置仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SiteSettingsRepositoryConfigError {
    #[error("站点设置数据库操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 站点设置仓储错误；不透传数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SiteSettingsRepositoryError {
    #[error("站点设置数据库操作失败")]
    Query,
    #[error("站点设置数据库操作超时")]
    Timeout,
    #[error("站点设置持久化状态损坏")]
    Invariant,
    #[error("站点设置字段无效")]
    InvalidSettings,
    #[error("站点设置版本冲突")]
    Conflict,
}

/// 站点身份与公开品牌配置的固定记录仓储。
#[derive(Clone)]
pub struct SiteSettingsRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl SiteSettingsRepository {
    /// 使用共享连接池和单次操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, SiteSettingsRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(SiteSettingsRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 读取固定站点设置记录；固定行缺失视为持久化状态损坏。
    pub async fn settings(&self) -> Result<SiteSettingsRecord, SiteSettingsRepositoryError> {
        match timeout(self.operation_timeout, self.settings_inner()).await {
            Ok(result) => result,
            Err(_) => Err(internal_error(SiteSettingsRepositoryError::Timeout)),
        }
    }

    /// 原子覆盖完整站点设置，并单调递增配置版本。
    pub async fn update(
        &self,
        record: SiteSettingsWriteRecord,
    ) -> Result<SiteSettingsRecord, SiteSettingsRepositoryError> {
        match timeout(self.operation_timeout, self.update_inner(record)).await {
            Ok(result) => result,
            Err(_) => Err(internal_error(SiteSettingsRepositoryError::Timeout)),
        }
    }

    /// 原子切换当前前端模板，并单调递增站点设置版本。
    ///
    /// 该方法只修改模板选择，适合运行时扫描/切换接口，其他站点设置会在同一
    /// 行锁内完整保留；传入 None 可恢复编译进二进制的内嵌前端。
    pub async fn set_frontend_template_id(
        &self,
        template_id: Option<String>,
    ) -> Result<SiteSettingsRecord, SiteSettingsRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.set_frontend_template_id_inner(template_id),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(internal_error(SiteSettingsRepositoryError::Timeout)),
        }
    }

    async fn settings_inner(&self) -> Result<SiteSettingsRecord, SiteSettingsRepositoryError> {
        let Some(model) = site_settings::Entity::find_by_id(SITE_SETTINGS_ID)
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("site_settings_read"))?
        else {
            return Err(internal_error(SiteSettingsRepositoryError::Invariant));
        };
        record_from_model(model)
    }

    async fn update_inner(
        &self,
        record: SiteSettingsWriteRecord,
    ) -> Result<SiteSettingsRecord, SiteSettingsRepositoryError> {
        record.validate()?;
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("site_settings_begin"))?;
        let existing = lock_settings(&transaction).await?;
        if existing.version != record.expected_version {
            return Err(SiteSettingsRepositoryError::Conflict);
        }
        let version = existing
            .version
            .checked_add(1)
            .ok_or_else(|| internal_error(SiteSettingsRepositoryError::Invariant))?;
        let saved = site_settings::ActiveModel {
            id: Set(SITE_SETTINGS_ID),
            site_name: Set(record.site_name),
            public_base_url: Set(record.public_base_url),
            brand_logo_url: Set(record.brand_logo_url),
            brand_tagline: Set(record.brand_tagline),
            brand_description: Set(record.brand_description),
            frontend_template_id: Set(record.frontend_template_id),
            balance_display_mode: Set(record.balance_display.mode() as i16),
            balance_unit_name: Set(record.balance_display.unit_name().to_owned()),
            balance_unit_symbol: Set(record.balance_display.unit_symbol().to_owned()),
            quota_units_per_display_unit: Set(record
                .balance_display
                .quota_units_per_display_unit()),
            balance_symbol_position: Set(record.balance_display.symbol_position() as i16),
            balance_fraction_digits: Set(i16::from(record.balance_display.fraction_digits())),
            version: Set(version),
            created_at: Set(existing.created_at),
            updated_at: Set(sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc()),
        }
        .update(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query_error("site_settings_write"))?;
        let saved = record_from_model(saved)?;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("site_settings_commit"))?;
        Ok(saved)
    }

    async fn set_frontend_template_id_inner(
        &self,
        template_id: Option<String>,
    ) -> Result<SiteSettingsRecord, SiteSettingsRepositoryError> {
        if !valid_frontend_template_id(template_id.as_deref()) {
            return Err(SiteSettingsRepositoryError::InvalidSettings);
        }
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("site_settings_begin"))?;
        let existing = lock_settings(&transaction).await?;
        let version = existing
            .version
            .checked_add(1)
            .ok_or_else(|| internal_error(SiteSettingsRepositoryError::Invariant))?;
        let saved = site_settings::ActiveModel {
            id: Set(SITE_SETTINGS_ID),
            site_name: Set(existing.site_name),
            public_base_url: Set(existing.public_base_url),
            brand_logo_url: Set(existing.brand_logo_url),
            brand_tagline: Set(existing.brand_tagline),
            brand_description: Set(existing.brand_description),
            frontend_template_id: Set(template_id),
            balance_display_mode: Set(existing.balance_display_mode),
            balance_unit_name: Set(existing.balance_unit_name),
            balance_unit_symbol: Set(existing.balance_unit_symbol),
            quota_units_per_display_unit: Set(existing.quota_units_per_display_unit),
            balance_symbol_position: Set(existing.balance_symbol_position),
            balance_fraction_digits: Set(existing.balance_fraction_digits),
            version: Set(version),
            created_at: Set(existing.created_at),
            updated_at: Set(sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc()),
        }
        .update(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query_error("site_settings_write"))?;
        let saved = record_from_model(saved)?;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("site_settings_commit"))?;
        Ok(saved)
    }
}

async fn lock_settings(
    transaction: &DatabaseTransaction,
) -> Result<site_settings::Model, SiteSettingsRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 没有 FOR UPDATE，通过无变化写入先取得数据库写锁。
        let result = site_settings::Entity::update_many()
            .filter(site_settings::Column::Id.eq(SITE_SETTINGS_ID))
            .col_expr(
                site_settings::Column::Version,
                Expr::col(site_settings::Column::Version).into(),
            )
            .exec(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("site_settings_lock"))?;
        if result.rows_affected != 1 {
            return Err(internal_error(SiteSettingsRepositoryError::Invariant));
        }
    }

    let mut query =
        site_settings::Entity::find().filter(site_settings::Column::Id.eq(SITE_SETTINGS_ID));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query_error("site_settings_read_for_update"))?
        .ok_or_else(|| internal_error(SiteSettingsRepositoryError::Invariant))
}

fn record_from_model(
    model: site_settings::Model,
) -> Result<SiteSettingsRecord, SiteSettingsRepositoryError> {
    let balance_display = BalanceDisplayPolicyRecord::new(
        BalanceDisplayModeRecord::try_from(model.balance_display_mode).map_err(invariant_error)?,
        model.balance_unit_name.clone(),
        model.balance_unit_symbol.clone(),
        model.quota_units_per_display_unit,
        BalanceSymbolPositionRecord::try_from(model.balance_symbol_position)
            .map_err(invariant_error)?,
        u8::try_from(model.balance_fraction_digits)
            .map_err(|_| internal_error(SiteSettingsRepositoryError::Invariant))?,
    )
    .map_err(invariant_error)?;
    if model.id != SITE_SETTINGS_ID
        || model.version < 1
        || !valid_site_name(&model.site_name)
        || model
            .public_base_url
            .as_deref()
            .is_some_and(|value| !valid_public_base_url(value))
        || model
            .brand_logo_url
            .as_deref()
            .is_some_and(|value| !valid_brand_asset_url(value))
        || !valid_optional_single_line(model.brand_tagline.as_deref(), MAX_BRAND_TAGLINE_BYTES)
        || !valid_optional_description(model.brand_description.as_deref())
        || !valid_frontend_template_id(model.frontend_template_id.as_deref())
    {
        return Err(internal_error(SiteSettingsRepositoryError::Invariant));
    }
    Ok(SiteSettingsRecord {
        site_name: model.site_name,
        public_base_url: model.public_base_url,
        brand_logo_url: model.brand_logo_url,
        brand_tagline: model.brand_tagline,
        brand_description: model.brand_description,
        frontend_template_id: model.frontend_template_id,
        balance_display,
        version: model.version,
    })
}

fn valid_site_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SITE_NAME_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_public_base_url(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_URL_BYTES || value.trim() != value {
        return false;
    }
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}

fn valid_brand_asset_url(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_URL_BYTES
        || value.trim() != value
        || value.chars().any(char::is_control)
        || value.contains('\\')
    {
        return false;
    }
    if value.starts_with('/') {
        return !value.starts_with("//");
    }
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
}

fn valid_optional_single_line(value: Option<&str>, maximum_bytes: usize) -> bool {
    value.is_none_or(|value| {
        !value.is_empty()
            && value.len() <= maximum_bytes
            && value.trim() == value
            && !value.chars().any(char::is_control)
    })
}

fn valid_optional_description(value: Option<&str>) -> bool {
    value.is_none_or(|value| {
        !value.is_empty()
            && value.len() <= MAX_BRAND_DESCRIPTION_BYTES
            && value.trim() == value
            && !value
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    })
}

fn valid_frontend_template_id(value: Option<&str>) -> bool {
    value.is_none_or(|value| {
        !value.is_empty()
            && value.len() <= MAX_FRONTEND_TEMPLATE_ID_BYTES
            && value.trim() == value
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
            })
    })
}

fn valid_balance_unit(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn invariant_error(_: SiteSettingsRepositoryError) -> SiteSettingsRepositoryError {
    internal_error(SiteSettingsRepositoryError::Invariant)
}

fn query_error(operation: &'static str) -> SiteSettingsRepositoryError {
    tracing::error!(
        target: "af_db::site_settings",
        error_kind = operation,
        "站点设置数据库操作失败"
    );
    SiteSettingsRepositoryError::Query
}

fn internal_error(error: SiteSettingsRepositoryError) -> SiteSettingsRepositoryError {
    tracing::error!(
        target: "af_db::site_settings",
        error_kind = ?error,
        "站点设置内部状态无效"
    );
    error
}

impl fmt::Debug for SiteSettingsRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SiteSettingsRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_public_http_urls_and_rejects_credential_or_script_urls() {
        assert!(valid_public_base_url("https://example.com/gateway"));
        assert!(valid_brand_asset_url("/favicon.svg"));
        assert!(valid_brand_asset_url(
            "https://cdn.example.com/logo.svg?v=2"
        ));
        assert!(!valid_public_base_url("https://user@example.com"));
        assert!(!valid_public_base_url("https://example.com/?token=secret"));
        assert!(!valid_brand_asset_url("javascript:alert(1)"));
        assert!(!valid_brand_asset_url("//untrusted.example/logo.svg"));
    }
}
