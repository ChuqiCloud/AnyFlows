use std::{fmt, future::Future, pin::Pin};

use af_db::{
    BalanceDisplayModeRecord, BalanceDisplayPolicyRecord, BalanceSymbolPositionRecord,
    SiteSettingsRecord, SiteSettingsRepository, SiteSettingsRepositoryError,
    SiteSettingsWriteRecord,
};
pub use af_db::{
    SiteNavigationGroupRecord, SiteNavigationLinkRecord, SiteNavigationRecord,
    SiteSidebarLinkRecord,
};
use thiserror::Error;
use url::Url;

use crate::{SessionPrincipal, SessionRole};

const MAX_SITE_NAME_BYTES: usize = 80;
const MAX_URL_BYTES: usize = 2_048;
const MAX_BRAND_TAGLINE_BYTES: usize = 160;
const MAX_BRAND_DESCRIPTION_BYTES: usize = 500;
const MAX_BALANCE_UNIT_NAME_BYTES: usize = 64;
const MAX_BALANCE_UNIT_SYMBOL_BYTES: usize = 24;
const MAX_BALANCE_FRACTION_DIGITS: u8 = 4;

/// 余额数值的公开展示模式。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BalanceDisplayMode {
    Quota,
    CustomUnit,
}

/// 自定义单位符号相对数值的位置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BalanceSymbolPosition {
    Prefix,
    Suffix,
}

/// 余额展示策略仅负责界面换算，不进入内部计费或支付金额计算。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BalanceDisplayPolicy {
    mode: BalanceDisplayMode,
    unit_name: String,
    unit_symbol: String,
    quota_units_per_display_unit: i64,
    symbol_position: BalanceSymbolPosition,
    fraction_digits: u8,
}

impl BalanceDisplayPolicy {
    /// 规范化并校验管理员提交的完整展示策略。
    pub fn new(
        mode: BalanceDisplayMode,
        unit_name: String,
        unit_symbol: String,
        quota_units_per_display_unit: i64,
        symbol_position: BalanceSymbolPosition,
        fraction_digits: u8,
    ) -> Result<Self, SiteSettingsError> {
        let policy = Self {
            mode,
            unit_name: unit_name.trim().to_owned(),
            unit_symbol: unit_symbol.trim().to_owned(),
            quota_units_per_display_unit,
            symbol_position,
            fraction_digits,
        };
        if !valid_balance_unit(&policy.unit_name, MAX_BALANCE_UNIT_NAME_BYTES)
            || !valid_balance_unit(&policy.unit_symbol, MAX_BALANCE_UNIT_SYMBOL_BYTES)
            || policy.quota_units_per_display_unit <= 0
            || policy.fraction_digits > MAX_BALANCE_FRACTION_DIGITS
        {
            return Err(SiteSettingsError::InvalidInput);
        }
        Ok(policy)
    }

    /// 返回保持历史界面行为的原始额度展示策略。
    pub fn quota_default() -> Self {
        Self {
            mode: BalanceDisplayMode::Quota,
            unit_name: "算力积分".to_owned(),
            unit_symbol: "积分".to_owned(),
            quota_units_per_display_unit: 10_000,
            symbol_position: BalanceSymbolPosition::Suffix,
            fraction_digits: 0,
        }
    }

    #[must_use]
    pub const fn mode(&self) -> BalanceDisplayMode {
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
    pub const fn symbol_position(&self) -> BalanceSymbolPosition {
        self.symbol_position
    }

    #[must_use]
    pub const fn fraction_digits(&self) -> u8 {
        self.fraction_digits
    }

    fn from_record(record: &BalanceDisplayPolicyRecord) -> Self {
        Self {
            mode: match record.mode() {
                BalanceDisplayModeRecord::Quota => BalanceDisplayMode::Quota,
                BalanceDisplayModeRecord::CustomUnit => BalanceDisplayMode::CustomUnit,
            },
            unit_name: record.unit_name().to_owned(),
            unit_symbol: record.unit_symbol().to_owned(),
            quota_units_per_display_unit: record.quota_units_per_display_unit(),
            symbol_position: match record.symbol_position() {
                BalanceSymbolPositionRecord::Prefix => BalanceSymbolPosition::Prefix,
                BalanceSymbolPositionRecord::Suffix => BalanceSymbolPosition::Suffix,
            },
            fraction_digits: record.fraction_digits(),
        }
    }

    fn into_record(self) -> Result<BalanceDisplayPolicyRecord, SiteSettingsError> {
        BalanceDisplayPolicyRecord::new(
            match self.mode {
                BalanceDisplayMode::Quota => BalanceDisplayModeRecord::Quota,
                BalanceDisplayMode::CustomUnit => BalanceDisplayModeRecord::CustomUnit,
            },
            self.unit_name,
            self.unit_symbol,
            self.quota_units_per_display_unit,
            match self.symbol_position {
                BalanceSymbolPosition::Prefix => BalanceSymbolPositionRecord::Prefix,
                BalanceSymbolPosition::Suffix => BalanceSymbolPositionRecord::Suffix,
            },
            self.fraction_digits,
        )
        .map_err(|_| SiteSettingsError::InvalidInput)
    }
}

/// 游客可读取的站点身份与品牌投影，不包含内部版本或管理元数据。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicSiteSettings {
    site_name: String,
    public_base_url: Option<String>,
    brand_logo_url: Option<String>,
    brand_tagline: Option<String>,
    brand_description: Option<String>,
    navigation: SiteNavigationRecord,
    balance_display: BalanceDisplayPolicy,
}

impl PublicSiteSettings {
    /// 为替代服务实现构造经过同一边界校验的公开投影。
    pub fn new(
        site_name: String,
        public_base_url: Option<String>,
        brand_logo_url: Option<String>,
        brand_tagline: Option<String>,
        brand_description: Option<String>,
    ) -> Result<Self, SiteSettingsError> {
        let command = SiteSettingsCommand::new(
            site_name,
            public_base_url,
            brand_logo_url,
            brand_tagline,
            brand_description,
            BalanceDisplayPolicy::quota_default(),
            1,
        )?;
        Ok(Self {
            site_name: command.site_name,
            public_base_url: command.public_base_url,
            brand_logo_url: command.brand_logo_url,
            brand_tagline: command.brand_tagline,
            brand_description: command.brand_description,
            navigation: SiteNavigationRecord::default(),
            balance_display: command.balance_display,
        })
    }

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

    #[must_use]
    pub const fn navigation(&self) -> &SiteNavigationRecord {
        &self.navigation
    }

    #[must_use]
    pub const fn balance_display(&self) -> &BalanceDisplayPolicy {
        &self.balance_display
    }

    fn from_record(record: &SiteSettingsRecord) -> Self {
        Self {
            site_name: record.site_name().to_owned(),
            public_base_url: record.public_base_url().map(str::to_owned),
            brand_logo_url: record.brand_logo_url().map(str::to_owned),
            brand_tagline: record.brand_tagline().map(str::to_owned),
            brand_description: record.brand_description().map(str::to_owned),
            navigation: record.navigation().clone(),
            balance_display: BalanceDisplayPolicy::from_record(record.balance_display()),
        }
    }
}

/// 管理员可读取的完整站点设置投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminSiteSettings {
    public: PublicSiteSettings,
    version: i64,
}

impl AdminSiteSettings {
    /// 为替代服务实现构造完整管理员投影。
    pub fn new(public: PublicSiteSettings, version: i64) -> Result<Self, SiteSettingsError> {
        if version < 1 {
            return Err(SiteSettingsError::InvalidInput);
        }
        Ok(Self { public, version })
    }

    #[must_use]
    pub fn site_name(&self) -> &str {
        self.public.site_name()
    }

    #[must_use]
    pub fn public_base_url(&self) -> Option<&str> {
        self.public.public_base_url()
    }

    #[must_use]
    pub fn brand_logo_url(&self) -> Option<&str> {
        self.public.brand_logo_url()
    }

    #[must_use]
    pub fn brand_tagline(&self) -> Option<&str> {
        self.public.brand_tagline()
    }

    #[must_use]
    pub fn brand_description(&self) -> Option<&str> {
        self.public.brand_description()
    }

    #[must_use]
    pub const fn navigation(&self) -> &SiteNavigationRecord {
        self.public.navigation()
    }

    #[must_use]
    pub const fn balance_display(&self) -> &BalanceDisplayPolicy {
        self.public.balance_display()
    }

    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }

    fn from_record(record: &SiteSettingsRecord) -> Self {
        Self {
            public: PublicSiteSettings::from_record(record),
            version: record.version(),
        }
    }
}

/// 管理员完整保存站点身份与品牌信息的命令。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SiteSettingsCommand {
    site_name: String,
    public_base_url: Option<String>,
    brand_logo_url: Option<String>,
    brand_tagline: Option<String>,
    brand_description: Option<String>,
    balance_display: BalanceDisplayPolicy,
    expected_version: i64,
}

impl SiteSettingsCommand {
    /// 规范化文本与 URL，并拒绝脚本协议、凭据 URL 和不可见控制字符。
    pub fn new(
        site_name: String,
        public_base_url: Option<String>,
        brand_logo_url: Option<String>,
        brand_tagline: Option<String>,
        brand_description: Option<String>,
        balance_display: BalanceDisplayPolicy,
        expected_version: i64,
    ) -> Result<Self, SiteSettingsError> {
        let site_name = site_name.trim().to_owned();
        if !valid_site_name(&site_name) {
            return Err(SiteSettingsError::InvalidInput);
        }
        if expected_version < 1 {
            return Err(SiteSettingsError::InvalidInput);
        }
        Ok(Self {
            site_name,
            public_base_url: normalize_public_base_url(public_base_url)?,
            brand_logo_url: normalize_brand_asset_url(brand_logo_url)?,
            brand_tagline: normalize_optional_single_line(brand_tagline, MAX_BRAND_TAGLINE_BYTES)?,
            brand_description: normalize_optional_description(brand_description)?,
            balance_display,
            expected_version,
        })
    }

    fn into_record(self) -> Result<SiteSettingsWriteRecord, SiteSettingsError> {
        Ok(SiteSettingsWriteRecord::new(
            self.site_name,
            self.public_base_url,
            self.brand_logo_url,
            self.brand_tagline,
            self.brand_description,
            self.balance_display.into_record()?,
            self.expected_version,
        ))
    }
}

/// 站点设置应用服务的闭合错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SiteSettingsError {
    #[error("站点设置输入无效")]
    InvalidInput,
    #[error("站点设置权限不足")]
    Forbidden,
    #[error("站点设置内部失败")]
    Internal,
    #[error("站点设置版本冲突")]
    Conflict,
}

pub type PublicSiteSettingsFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PublicSiteSettings, SiteSettingsError>> + Send + 'a>>;
pub type AdminSiteSettingsReadFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminSiteSettings, SiteSettingsError>> + Send + 'a>>;
pub type AdminSiteSettingsUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminSiteSettings, SiteSettingsError>> + Send + 'a>>;

/// 公开投影与管理员完整投影分离的站点设置端口。
pub trait SiteSettingsService: Send + Sync {
    /// 读取游客可见的站点身份与品牌信息。
    fn public_settings(&self) -> PublicSiteSettingsFuture<'_>;

    /// 读取管理员完整设置与单调版本。
    fn admin_settings(&self, principal: SessionPrincipal) -> AdminSiteSettingsReadFuture<'_>;

    /// 原子覆盖完整站点设置。
    fn update(
        &self,
        principal: SessionPrincipal,
        command: SiteSettingsCommand,
    ) -> AdminSiteSettingsUpdateFuture<'_>;

    /// 独立更新站点公开导航，避免品牌设置表单覆盖菜单。
    fn update_navigation(
        &self,
        principal: SessionPrincipal,
        navigation: SiteNavigationRecord,
        expected_version: i64,
    ) -> AdminSiteSettingsUpdateFuture<'_>;
}

/// 使用强类型固定记录仓储实现站点设置用例。
pub struct DatabaseSiteSettingsService {
    repository: SiteSettingsRepository,
}

impl DatabaseSiteSettingsService {
    #[must_use]
    pub const fn new(repository: SiteSettingsRepository) -> Self {
        Self { repository }
    }
}

impl SiteSettingsService for DatabaseSiteSettingsService {
    fn public_settings(&self) -> PublicSiteSettingsFuture<'_> {
        Box::pin(async move {
            self.repository
                .settings()
                .await
                .map(|record| PublicSiteSettings::from_record(&record))
                .map_err(map_repository_error)
        })
    }

    fn admin_settings(&self, principal: SessionPrincipal) -> AdminSiteSettingsReadFuture<'_> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .settings()
                .await
                .map(|record| AdminSiteSettings::from_record(&record))
                .map_err(map_repository_error)
        })
    }

    fn update(
        &self,
        principal: SessionPrincipal,
        command: SiteSettingsCommand,
    ) -> AdminSiteSettingsUpdateFuture<'_> {
        Box::pin(async move {
            require_admin(principal)?;
            // 站点品牌表单不管理模板选择；读取并带回当前值，避免保存品牌时
            // 意外清空管理员刚选择的外部模板。
            let existing = self
                .repository
                .settings()
                .await
                .map_err(map_repository_error)?;
            let record = command
                .into_record()?
                .with_frontend_template_id(existing.frontend_template_id().map(str::to_owned));
            self.repository
                .update(record)
                .await
                .map(|record| AdminSiteSettings::from_record(&record))
                .map_err(map_repository_error)
        })
    }

    fn update_navigation(
        &self,
        principal: SessionPrincipal,
        navigation: SiteNavigationRecord,
        expected_version: i64,
    ) -> AdminSiteSettingsUpdateFuture<'_> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .update_navigation(navigation, expected_version)
                .await
                .map(|record| AdminSiteSettings::from_record(&record))
                .map_err(map_repository_error)
        })
    }
}

impl fmt::Debug for DatabaseSiteSettingsService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseSiteSettingsService")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), SiteSettingsError> {
    if principal.role() != SessionRole::Admin {
        return Err(SiteSettingsError::Forbidden);
    }
    Ok(())
}

fn map_repository_error(error: SiteSettingsRepositoryError) -> SiteSettingsError {
    match error {
        SiteSettingsRepositoryError::InvalidSettings => SiteSettingsError::InvalidInput,
        SiteSettingsRepositoryError::Conflict => SiteSettingsError::Conflict,
        SiteSettingsRepositoryError::Query
        | SiteSettingsRepositoryError::Timeout
        | SiteSettingsRepositoryError::Invariant => SiteSettingsError::Internal,
    }
}

fn valid_site_name(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_SITE_NAME_BYTES && !value.chars().any(char::is_control)
}

fn valid_balance_unit(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= maximum_bytes && !value.chars().any(char::is_control)
}

fn normalize_public_base_url(value: Option<String>) -> Result<Option<String>, SiteSettingsError> {
    let Some(value) = normalize_optional(value) else {
        return Ok(None);
    };
    if value.len() > MAX_URL_BYTES {
        return Err(SiteSettingsError::InvalidInput);
    }
    let url = Url::parse(&value).map_err(|_| SiteSettingsError::InvalidInput)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(SiteSettingsError::InvalidInput);
    }
    Ok(Some(trim_url_trailing_slash(url.to_string())))
}

fn normalize_brand_asset_url(value: Option<String>) -> Result<Option<String>, SiteSettingsError> {
    let Some(value) = normalize_optional(value) else {
        return Ok(None);
    };
    if value.len() > MAX_URL_BYTES || value.chars().any(char::is_control) || value.contains('\\') {
        return Err(SiteSettingsError::InvalidInput);
    }
    if value.starts_with('/') {
        if value.starts_with("//") {
            return Err(SiteSettingsError::InvalidInput);
        }
        return Ok(Some(value));
    }
    let url = Url::parse(&value).map_err(|_| SiteSettingsError::InvalidInput)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(SiteSettingsError::InvalidInput);
    }
    Ok(Some(url.to_string()))
}

fn normalize_optional_single_line(
    value: Option<String>,
    maximum_bytes: usize,
) -> Result<Option<String>, SiteSettingsError> {
    let value = normalize_optional(value);
    if value
        .as_deref()
        .is_some_and(|value| value.len() > maximum_bytes || value.chars().any(char::is_control))
    {
        return Err(SiteSettingsError::InvalidInput);
    }
    Ok(value)
}

fn normalize_optional_description(
    value: Option<String>,
) -> Result<Option<String>, SiteSettingsError> {
    let value = normalize_optional(value);
    if value.as_deref().is_some_and(|value| {
        value.len() > MAX_BRAND_DESCRIPTION_BYTES
            || value
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    }) {
        return Err(SiteSettingsError::InvalidInput);
    }
    Ok(value)
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn trim_url_trailing_slash(mut value: String) -> String {
    while value.ends_with('/') {
        value.pop();
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_normalizes_optional_brand_fields_and_public_base() {
        let command = SiteSettingsCommand::new(
            " AnyFlows Cloud ".to_owned(),
            Some("https://example.com/gateway/".to_owned()),
            Some(" /brand.svg ".to_owned()),
            Some(" 统一访问模型 ".to_owned()),
            Some(" 一套面向团队的模型 API 工作台。 ".to_owned()),
            BalanceDisplayPolicy::quota_default(),
            3,
        )
        .unwrap();
        assert_eq!(command.site_name, "AnyFlows Cloud");
        assert_eq!(
            command.public_base_url.as_deref(),
            Some("https://example.com/gateway")
        );
        assert_eq!(command.brand_logo_url.as_deref(), Some("/brand.svg"));
    }

    #[test]
    fn command_rejects_script_and_credential_urls() {
        assert!(
            SiteSettingsCommand::new(
                "AnyFlows".to_owned(),
                Some("https://admin:secret@example.com".to_owned()),
                None,
                None,
                None,
                BalanceDisplayPolicy::quota_default(),
                1,
            )
            .is_err()
        );
        assert!(
            SiteSettingsCommand::new(
                "AnyFlows".to_owned(),
                None,
                Some("javascript:alert(1)".to_owned()),
                None,
                None,
                BalanceDisplayPolicy::quota_default(),
                1,
            )
            .is_err()
        );
    }

    #[test]
    fn balance_display_policy_normalizes_units_and_rejects_invalid_scale() {
        let policy = BalanceDisplayPolicy::new(
            BalanceDisplayMode::CustomUnit,
            " 算力积分 ".to_owned(),
            " 积分 ".to_owned(),
            10_000,
            BalanceSymbolPosition::Suffix,
            2,
        )
        .unwrap();
        assert_eq!(policy.unit_name(), "算力积分");
        assert_eq!(policy.unit_symbol(), "积分");
        assert_eq!(policy.quota_units_per_display_unit(), 10_000);
        assert!(
            BalanceDisplayPolicy::new(
                BalanceDisplayMode::CustomUnit,
                "算力积分".to_owned(),
                "积分".to_owned(),
                0,
                BalanceSymbolPosition::Suffix,
                2,
            )
            .is_err()
        );
    }
}
