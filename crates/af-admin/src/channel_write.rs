use std::{fmt, future::Future, pin::Pin};

use af_account::{CredentialEncryptor, PlainCredentialSecret};
use af_db::{
    AdminChannelDeleteOutcome, AdminChannelMutationOutcome, AdminChannelRepository,
    AdminChannelWriteRecord, AdminChannelWriteRepositoryError, AdminCredentialCreateOutcome,
    AdminCredentialDeleteOutcome, AdminCredentialMutationOutcome, AdminCredentialWriteRecord,
    MAX_ADMIN_CHANNEL_JSON_BYTES,
};
use af_domain::{ChannelId, ChannelType, CredentialId, CredentialKind, Protocol, Status};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    AdminChannel, AdminCredential, AdminCredentialMultiKeyMode, AdminCredentialQuotaDimension,
    SessionPrincipal, SessionRole,
};

/// 管理写接口允许声明的渠道或凭据状态。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminRoutingWriteStatus {
    /// 启用并允许参与服务或调度。
    Enabled,
    /// 手动禁用或保持待验证状态。
    Disabled,
}

impl AdminRoutingWriteStatus {
    const fn into_domain(self) -> Status {
        match self {
            Self::Enabled => Status::Enabled,
            Self::Disabled => Status::Disabled,
        }
    }
}

/// 管理员创建渠道时使用的完整配置。
pub struct AdminChannelCreateCommand {
    fields: AdminChannelWriteFields,
}

impl AdminChannelCreateCommand {
    /// 校验当前生产运行时支持的渠道类型及全部配置边界。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端渠道写入契约一一对应"
    )]
    pub fn new(
        name: String,
        channel_type: ChannelType,
        protocol: Protocol,
        base_url: Option<String>,
        status: AdminRoutingWriteStatus,
        weight: i32,
        priority: i32,
        auto_ban: bool,
        model_mapping: serde_json::Value,
        param_override: serde_json::Value,
        header_override: serde_json::Value,
        settings: serde_json::Value,
        tag: Option<String>,
    ) -> Result<Self, AdminChannelWriteError> {
        Ok(Self {
            fields: AdminChannelWriteFields::new(
                name,
                channel_type,
                protocol,
                base_url,
                status,
                weight,
                priority,
                auto_ban,
                model_mapping,
                param_override,
                header_override,
                settings,
                tag,
            )?,
        })
    }

    fn into_record(self) -> AdminChannelWriteRecord {
        self.fields.into_record()
    }
}

impl fmt::Debug for AdminChannelCreateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminChannelCreateCommand(<已脱敏>)")
    }
}

/// 管理员完整更新渠道时使用的配置；运行累计字段不属于命令。
pub struct AdminChannelUpdateCommand {
    fields: AdminChannelWriteFields,
}

impl AdminChannelUpdateCommand {
    /// 校验完整更新字段，禁止借管理接口写入未实现的适配器语义。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端渠道写入契约一一对应"
    )]
    pub fn new(
        name: String,
        channel_type: ChannelType,
        protocol: Protocol,
        base_url: Option<String>,
        status: AdminRoutingWriteStatus,
        weight: i32,
        priority: i32,
        auto_ban: bool,
        model_mapping: serde_json::Value,
        param_override: serde_json::Value,
        header_override: serde_json::Value,
        settings: serde_json::Value,
        tag: Option<String>,
    ) -> Result<Self, AdminChannelWriteError> {
        Ok(Self {
            fields: AdminChannelWriteFields::new(
                name,
                channel_type,
                protocol,
                base_url,
                status,
                weight,
                priority,
                auto_ban,
                model_mapping,
                param_override,
                header_override,
                settings,
                tag,
            )?,
        })
    }

    fn into_record(self) -> AdminChannelWriteRecord {
        self.fields.into_record()
    }
}

impl fmt::Debug for AdminChannelUpdateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminChannelUpdateCommand(<已脱敏>)")
    }
}

struct AdminChannelWriteFields {
    name: String,
    channel_type: ChannelType,
    protocol: Protocol,
    base_url: Option<String>,
    status: AdminRoutingWriteStatus,
    weight: i32,
    priority: i32,
    auto_ban: bool,
    model_mapping: serde_json::Value,
    param_override: serde_json::Value,
    header_override: serde_json::Value,
    settings: serde_json::Value,
    tag: Option<String>,
}

impl AdminChannelWriteFields {
    #[allow(
        clippy::too_many_arguments,
        reason = "仅在统一校验入口组装完整渠道字段"
    )]
    fn new(
        name: String,
        channel_type: ChannelType,
        protocol: Protocol,
        base_url: Option<String>,
        status: AdminRoutingWriteStatus,
        weight: i32,
        priority: i32,
        auto_ban: bool,
        model_mapping: serde_json::Value,
        param_override: serde_json::Value,
        header_override: serde_json::Value,
        settings: serde_json::Value,
        tag: Option<String>,
    ) -> Result<Self, AdminChannelWriteError> {
        if !valid_text(&name, 128)
            || weight < 0
            || channel_type != ChannelType::OpenAi
            || protocol != Protocol::OpenAiChat
            || base_url
                .as_deref()
                .is_some_and(|value| value.is_empty() || value.len() > 2_048)
            || tag.as_deref().is_some_and(|value| !valid_text(value, 64))
        {
            return Err(AdminChannelWriteError::InvalidInput);
        }
        validate_json_object(&model_mapping, true)?;
        validate_json_object(&param_override, false)?;
        validate_json_object(&header_override, false)?;
        validate_json_object(&settings, false)?;
        Ok(Self {
            name,
            channel_type,
            protocol,
            base_url,
            status,
            weight,
            priority,
            auto_ban,
            model_mapping,
            param_override,
            header_override,
            settings,
            tag,
        })
    }

    fn into_record(self) -> AdminChannelWriteRecord {
        AdminChannelWriteRecord::new(
            self.name,
            self.channel_type,
            self.protocol,
            self.base_url,
            self.status.into_domain(),
            self.weight,
            self.priority,
            self.auto_ban,
            self.model_mapping,
            self.param_override,
            self.header_override,
            self.settings,
            self.tag,
        )
    }
}

/// 管理员创建凭据时使用的完整配置和一次性明文。
pub struct AdminCredentialCreateCommand {
    fields: AdminCredentialWriteFields,
    secret: PlainCredentialSecret,
}

impl AdminCredentialCreateCommand {
    /// 校验类型化 secret 及非敏感调度配置。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端凭据写入契约一一对应"
    )]
    pub fn new(
        kind: CredentialKind,
        secret: String,
        status: AdminRoutingWriteStatus,
        multi_key_mode: Option<AdminCredentialMultiKeyMode>,
        priority: i32,
        weight: i32,
        concurrency: Option<i32>,
        load_factor_micros: Option<i64>,
        rate_multiplier_micros: Option<i64>,
        schedulable: bool,
        parent_id: Option<CredentialId>,
        quota_dimension: AdminCredentialQuotaDimension,
        proxy_id: Option<i64>,
        oauth_provider: Option<String>,
        oauth_account_key: Option<String>,
        oauth_project_id: Option<String>,
    ) -> Result<Self, AdminChannelWriteError> {
        let fields = AdminCredentialWriteFields::new(
            kind,
            status,
            multi_key_mode,
            priority,
            weight,
            concurrency,
            load_factor_micros,
            rate_multiplier_micros,
            schedulable,
            parent_id,
            quota_dimension,
            proxy_id,
            oauth_provider,
            oauth_account_key,
            oauth_project_id,
        )?;
        let secret = PlainCredentialSecret::new(kind, secret)
            .map_err(|_| AdminChannelWriteError::InvalidInput)?;
        Ok(Self { fields, secret })
    }

    fn into_parts(self) -> (AdminCredentialWriteRecord, PlainCredentialSecret) {
        (self.fields.into_record(), self.secret)
    }
}

impl fmt::Debug for AdminCredentialCreateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminCredentialCreateCommand(<已脱敏>)")
    }
}

/// 管理员完整更新凭据时使用的配置；空 secret 表示保留原密文。
pub struct AdminCredentialUpdateCommand {
    fields: AdminCredentialWriteFields,
    secret: Option<PlainCredentialSecret>,
}

impl AdminCredentialUpdateCommand {
    /// 校验更新字段；所属渠道和凭据类型只用于确认归属，不允许转移。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端凭据写入契约一一对应"
    )]
    pub fn new(
        kind: CredentialKind,
        secret: Option<String>,
        status: AdminRoutingWriteStatus,
        multi_key_mode: Option<AdminCredentialMultiKeyMode>,
        priority: i32,
        weight: i32,
        concurrency: Option<i32>,
        load_factor_micros: Option<i64>,
        rate_multiplier_micros: Option<i64>,
        schedulable: bool,
        parent_id: Option<CredentialId>,
        quota_dimension: AdminCredentialQuotaDimension,
        proxy_id: Option<i64>,
        oauth_provider: Option<String>,
        oauth_account_key: Option<String>,
        oauth_project_id: Option<String>,
    ) -> Result<Self, AdminChannelWriteError> {
        let fields = AdminCredentialWriteFields::new(
            kind,
            status,
            multi_key_mode,
            priority,
            weight,
            concurrency,
            load_factor_micros,
            rate_multiplier_micros,
            schedulable,
            parent_id,
            quota_dimension,
            proxy_id,
            oauth_provider,
            oauth_account_key,
            oauth_project_id,
        )?;
        let secret = secret
            .map(|secret| PlainCredentialSecret::new(kind, secret))
            .transpose()
            .map_err(|_| AdminChannelWriteError::InvalidInput)?;
        Ok(Self { fields, secret })
    }

    fn into_parts(self) -> (AdminCredentialWriteRecord, Option<PlainCredentialSecret>) {
        (self.fields.into_record(), self.secret)
    }
}

impl fmt::Debug for AdminCredentialUpdateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminCredentialUpdateCommand(<已脱敏>)")
    }
}

struct AdminCredentialWriteFields {
    kind: CredentialKind,
    status: AdminRoutingWriteStatus,
    multi_key_mode: Option<AdminCredentialMultiKeyMode>,
    priority: i32,
    weight: i32,
    concurrency: Option<i32>,
    load_factor_micros: Option<i64>,
    rate_multiplier_micros: Option<i64>,
    schedulable: bool,
    parent_id: Option<CredentialId>,
    quota_dimension: AdminCredentialQuotaDimension,
    proxy_id: Option<i64>,
    oauth_provider: Option<String>,
    oauth_account_key: Option<String>,
    oauth_project_id: Option<String>,
}

impl AdminCredentialWriteFields {
    #[allow(
        clippy::too_many_arguments,
        reason = "仅在统一校验入口组装完整凭据字段"
    )]
    fn new(
        kind: CredentialKind,
        status: AdminRoutingWriteStatus,
        multi_key_mode: Option<AdminCredentialMultiKeyMode>,
        priority: i32,
        weight: i32,
        concurrency: Option<i32>,
        load_factor_micros: Option<i64>,
        rate_multiplier_micros: Option<i64>,
        schedulable: bool,
        parent_id: Option<CredentialId>,
        quota_dimension: AdminCredentialQuotaDimension,
        proxy_id: Option<i64>,
        oauth_provider: Option<String>,
        oauth_account_key: Option<String>,
        oauth_project_id: Option<String>,
    ) -> Result<Self, AdminChannelWriteError> {
        let oauth_fields_present =
            oauth_provider.is_some() || oauth_account_key.is_some() || oauth_project_id.is_some();
        if !matches!(kind, CredentialKind::ApiKey | CredentialKind::Oauth)
            || weight < 0
            || concurrency.is_some_and(|value| value < 0)
            || load_factor_micros.is_some_and(|value| value < 0)
            || rate_multiplier_micros.is_some_and(|value| value < 0)
            || proxy_id.is_some_and(|value| value <= 0)
            || !valid_optional_text(oauth_provider.as_deref(), 64)
            || !valid_optional_text(oauth_account_key.as_deref(), 255)
            || !valid_optional_text(oauth_project_id.as_deref(), 255)
            || (kind == CredentialKind::ApiKey && oauth_fields_present)
        {
            return Err(AdminChannelWriteError::InvalidInput);
        }
        Ok(Self {
            kind,
            status,
            multi_key_mode,
            priority,
            weight,
            concurrency,
            load_factor_micros,
            rate_multiplier_micros,
            schedulable,
            parent_id,
            quota_dimension,
            proxy_id,
            oauth_provider,
            oauth_account_key,
            oauth_project_id,
        })
    }

    fn into_record(self) -> AdminCredentialWriteRecord {
        AdminCredentialWriteRecord::new(
            self.kind,
            self.status.into_domain(),
            self.multi_key_mode.map(|mode| match mode {
                AdminCredentialMultiKeyMode::Random => 1,
                AdminCredentialMultiKeyMode::RoundRobin => 2,
            }),
            self.priority,
            self.weight,
            self.concurrency,
            self.load_factor_micros,
            self.rate_multiplier_micros,
            self.schedulable,
            self.parent_id,
            match self.quota_dimension {
                AdminCredentialQuotaDimension::Global => "global",
                AdminCredentialQuotaDimension::Spark => "spark",
            },
            self.proxy_id,
            self.oauth_provider,
            self.oauth_account_key,
            self.oauth_project_id,
        )
    }
}

/// 管理渠道或凭据写入失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminChannelWriteError {
    /// 请求字段、类型或父凭据引用无效。
    #[error("管理渠道写入参数无效")]
    InvalidInput,
    /// 当前会话不是管理员。
    #[error("管理渠道写入权限不足")]
    Forbidden,
    /// 渠道不存在或已经软删除。
    #[error("管理渠道不存在")]
    ChannelNotFound,
    /// 凭据不存在、已删除或不属于父渠道。
    #[error("管理凭据不存在")]
    CredentialNotFound,
    /// 加密、数据库或持久化状态发生内部故障。
    #[error("管理渠道写入内部失败")]
    Internal,
}

pub type AdminChannelCreateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminChannel, AdminChannelWriteError>> + Send + 'a>>;
pub type AdminChannelUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminChannel, AdminChannelWriteError>> + Send + 'a>>;
pub type AdminChannelDeleteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), AdminChannelWriteError>> + Send + 'a>>;
pub type AdminCredentialCreateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminCredential, AdminChannelWriteError>> + Send + 'a>>;
pub type AdminCredentialUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminCredential, AdminChannelWriteError>> + Send + 'a>>;
pub type AdminCredentialDeleteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), AdminChannelWriteError>> + Send + 'a>>;

/// 管理渠道与凭据写入应用端口；角色校验必须先于加密和仓储访问。
pub trait AdminChannelWriter: Send + Sync {
    /// 创建渠道并返回非敏感管理快照。
    fn create_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminChannelCreateCommand,
    ) -> AdminChannelCreateFuture<'a>;

    /// 完整更新渠道配置，同时保留运行累计字段。
    fn update_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        command: AdminChannelUpdateCommand,
    ) -> AdminChannelUpdateFuture<'a>;

    /// 安全软删除渠道、凭据和直接运行时关系。
    fn delete_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
    ) -> AdminChannelDeleteFuture<'a>;

    /// 创建凭据；明文只在生成绑定真实 ID 的密文时短暂暴露。
    fn create_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        command: AdminCredentialCreateCommand,
    ) -> AdminCredentialCreateFuture<'a>;

    /// 完整更新凭据配置，可选轮换 secret。
    fn update_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
        command: AdminCredentialUpdateCommand,
    ) -> AdminCredentialUpdateFuture<'a>;

    /// 安全软删除凭据及其影子后代。
    fn delete_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
    ) -> AdminCredentialDeleteFuture<'a>;
}

/// 使用数据库仓储和启动密钥实现管理员渠道写入。
pub struct DatabaseAdminChannelWriter {
    repository: AdminChannelRepository,
    encryptor: CredentialEncryptor,
}

impl DatabaseAdminChannelWriter {
    /// 绑定渠道仓储和已校验的凭据加密器。
    #[must_use]
    pub const fn new(repository: AdminChannelRepository, encryptor: CredentialEncryptor) -> Self {
        Self {
            repository,
            encryptor,
        }
    }
}

impl AdminChannelWriter for DatabaseAdminChannelWriter {
    fn create_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminChannelCreateCommand,
    ) -> AdminChannelCreateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .create_channel(command.into_record())
                .await
                .map(AdminChannel::from_record)
                .map_err(map_repository_error)
        })
    }

    fn update_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        command: AdminChannelUpdateCommand,
    ) -> AdminChannelUpdateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .update_channel(channel_id, command.into_record())
                .await
                .map_err(map_repository_error)?
            {
                AdminChannelMutationOutcome::Mutated(record) => {
                    Ok(AdminChannel::from_record(*record))
                }
                AdminChannelMutationOutcome::NotFound => {
                    Err(AdminChannelWriteError::ChannelNotFound)
                }
            }
        })
    }

    fn delete_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
    ) -> AdminChannelDeleteFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .delete_channel(channel_id)
                .await
                .map_err(map_repository_error)?
            {
                AdminChannelDeleteOutcome::Deleted => Ok(()),
                AdminChannelDeleteOutcome::NotFound => Err(AdminChannelWriteError::ChannelNotFound),
            }
        })
    }

    fn create_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        command: AdminCredentialCreateCommand,
    ) -> AdminCredentialCreateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let (record, secret) = command.into_parts();
            match self
                .repository
                .create_credential(channel_id, record, |credential_id| {
                    self.encryptor
                        .encrypt(channel_id, credential_id.get(), &secret)
                        .map_err(|_| ())
                })
                .await
                .map_err(map_repository_error)?
            {
                AdminCredentialCreateOutcome::Created(record) => map_credential(*record),
                AdminCredentialCreateOutcome::ChannelNotFound => {
                    Err(AdminChannelWriteError::ChannelNotFound)
                }
            }
        })
    }

    fn update_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
        command: AdminCredentialUpdateCommand,
    ) -> AdminCredentialUpdateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let (record, secret) = command.into_parts();
            let replacement = secret
                .as_ref()
                .map(|secret| {
                    self.encryptor
                        .encrypt(channel_id, credential_id.get(), secret)
                })
                .transpose()
                .map_err(|_| AdminChannelWriteError::Internal)?;
            match self
                .repository
                .update_credential(channel_id, credential_id, record, replacement)
                .await
                .map_err(map_repository_error)?
            {
                AdminCredentialMutationOutcome::Mutated(record) => map_credential(*record),
                AdminCredentialMutationOutcome::ChannelNotFound => {
                    Err(AdminChannelWriteError::ChannelNotFound)
                }
                AdminCredentialMutationOutcome::CredentialNotFound => {
                    Err(AdminChannelWriteError::CredentialNotFound)
                }
            }
        })
    }

    fn delete_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
    ) -> AdminCredentialDeleteFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .delete_credential(channel_id, credential_id)
                .await
                .map_err(map_repository_error)?
            {
                AdminCredentialDeleteOutcome::Deleted => Ok(()),
                AdminCredentialDeleteOutcome::ChannelNotFound => {
                    Err(AdminChannelWriteError::ChannelNotFound)
                }
                AdminCredentialDeleteOutcome::CredentialNotFound => {
                    Err(AdminChannelWriteError::CredentialNotFound)
                }
            }
        })
    }
}

impl fmt::Debug for DatabaseAdminChannelWriter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminChannelWriter(<已脱敏>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminChannelWriteError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AdminChannelWriteError::Forbidden)
    }
}

fn map_repository_error(error: AdminChannelWriteRepositoryError) -> AdminChannelWriteError {
    match error {
        AdminChannelWriteRepositoryError::InvalidInput
        | AdminChannelWriteRepositoryError::InvalidReference => {
            AdminChannelWriteError::InvalidInput
        }
        AdminChannelWriteRepositoryError::SecretPreparation
        | AdminChannelWriteRepositoryError::Query
        | AdminChannelWriteRepositoryError::Timeout
        | AdminChannelWriteRepositoryError::Invariant => AdminChannelWriteError::Internal,
    }
}

fn map_credential(
    record: af_db::AdminCredentialRecord,
) -> Result<AdminCredential, AdminChannelWriteError> {
    AdminCredential::from_record(record).map_err(|_| AdminChannelWriteError::Internal)
}

fn validate_json_object(
    value: &serde_json::Value,
    string_values_only: bool,
) -> Result<(), AdminChannelWriteError> {
    let serde_json::Value::Object(object) = value else {
        return Err(AdminChannelWriteError::InvalidInput);
    };
    if serde_json::to_vec(value)
        .ok()
        .is_none_or(|encoded| encoded.len() > MAX_ADMIN_CHANNEL_JSON_BYTES as usize)
        || (string_values_only
            && object.iter().any(|(key, value)| {
                !valid_text(key, 256) || value.as_str().is_none_or(|value| !valid_text(value, 256))
            }))
    {
        return Err(AdminChannelWriteError::InvalidInput);
    }
    Ok(())
}

fn valid_optional_text(value: Option<&str>, maximum_bytes: usize) -> bool {
    value.is_none_or(|value| valid_text(value, maximum_bytes))
}

fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_reject_unsupported_types_and_redact_sensitive_fields() {
        let channel = AdminChannelCreateCommand::new(
            "private".to_owned(),
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
            Some("https://private.example.com".to_owned()),
            AdminRoutingWriteStatus::Disabled,
            10,
            20,
            true,
            serde_json::json!({"model": "upstream"}),
            serde_json::json!({}),
            serde_json::json!({"x-private": "header-secret"}),
            serde_json::json!({"private": "settings-secret"}),
            None,
        )
        .unwrap();
        let credential = AdminCredentialCreateCommand::new(
            CredentialKind::ApiKey,
            "sk-private".to_owned(),
            AdminRoutingWriteStatus::Disabled,
            None,
            0,
            0,
            None,
            None,
            None,
            false,
            None,
            AdminCredentialQuotaDimension::Global,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let rendered = format!("{channel:?}{credential:?}");
        for canary in [
            "private.example.com",
            "header-secret",
            "settings-secret",
            "sk-private",
        ] {
            assert!(!rendered.contains(canary));
        }

        assert!(
            AdminChannelCreateCommand::new(
                "unsupported".to_owned(),
                ChannelType::Anthropic,
                Protocol::Anthropic,
                None,
                AdminRoutingWriteStatus::Disabled,
                0,
                0,
                true,
                serde_json::json!({}),
                serde_json::json!({}),
                serde_json::json!({}),
                serde_json::json!({}),
                None,
            )
            .is_err()
        );
        assert!(
            AdminCredentialCreateCommand::new(
                CredentialKind::Bedrock,
                "private".to_owned(),
                AdminRoutingWriteStatus::Disabled,
                None,
                0,
                0,
                None,
                None,
                None,
                false,
                None,
                AdminCredentialQuotaDimension::Global,
                None,
                None,
                None,
                None,
            )
            .is_err()
        );
    }
}
