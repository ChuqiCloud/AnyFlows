use std::{fmt, future::Future, pin::Pin};

use af_db::{
    SiteSettingsRecord, SiteSettingsRepository, SiteSettingsRepositoryError,
    SiteSettingsWriteRecord,
};
use thiserror::Error;
use url::Url;

use crate::{SessionPrincipal, SessionRole};

const MAX_SITE_NAME_BYTES: usize = 80;
const MAX_URL_BYTES: usize = 2_048;
const MAX_BRAND_TAGLINE_BYTES: usize = 160;
const MAX_BRAND_DESCRIPTION_BYTES: usize = 500;

/// 游客可读取的站点身份与品牌投影，不包含内部版本或管理元数据。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicSiteSettings {
    site_name: String,
    public_base_url: Option<String>,
    brand_logo_url: Option<String>,
    brand_tagline: Option<String>,
    brand_description: Option<String>,
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
        )?;
        Ok(Self {
            site_name: command.site_name,
            public_base_url: command.public_base_url,
            brand_logo_url: command.brand_logo_url,
            brand_tagline: command.brand_tagline,
            brand_description: command.brand_description,
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

    fn from_record(record: &SiteSettingsRecord) -> Self {
        Self {
            site_name: record.site_name().to_owned(),
            public_base_url: record.public_base_url().map(str::to_owned),
            brand_logo_url: record.brand_logo_url().map(str::to_owned),
            brand_tagline: record.brand_tagline().map(str::to_owned),
            brand_description: record.brand_description().map(str::to_owned),
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
}

impl SiteSettingsCommand {
    /// 规范化文本与 URL，并拒绝脚本协议、凭据 URL 和不可见控制字符。
    pub fn new(
        site_name: String,
        public_base_url: Option<String>,
        brand_logo_url: Option<String>,
        brand_tagline: Option<String>,
        brand_description: Option<String>,
    ) -> Result<Self, SiteSettingsError> {
        let site_name = site_name.trim().to_owned();
        if !valid_site_name(&site_name) {
            return Err(SiteSettingsError::InvalidInput);
        }
        Ok(Self {
            site_name,
            public_base_url: normalize_public_base_url(public_base_url)?,
            brand_logo_url: normalize_brand_asset_url(brand_logo_url)?,
            brand_tagline: normalize_optional_single_line(brand_tagline, MAX_BRAND_TAGLINE_BYTES)?,
            brand_description: normalize_optional_description(brand_description)?,
        })
    }

    fn into_record(self) -> SiteSettingsWriteRecord {
        SiteSettingsWriteRecord::new(
            self.site_name,
            self.public_base_url,
            self.brand_logo_url,
            self.brand_tagline,
            self.brand_description,
        )
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
            self.repository
                .update(command.into_record())
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
        SiteSettingsRepositoryError::Query
        | SiteSettingsRepositoryError::Timeout
        | SiteSettingsRepositoryError::Invariant => SiteSettingsError::Internal,
    }
}

fn valid_site_name(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_SITE_NAME_BYTES && !value.chars().any(char::is_control)
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
            )
            .is_err()
        );
    }
}
