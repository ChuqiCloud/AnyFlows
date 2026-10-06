use af_admin::{
    AdminCredentialCreateCommand, AdminCredentialExport, AdminCredentialMultiKeyMode,
    AdminCredentialQuotaDimension, AdminCredentialSecret, AdminCredentialUpdateCommand,
    AdminRoutingWriteStatus, PlainOAuthCredential,
};
use af_domain::{ChannelType, CredentialId, CredentialKind, Protocol};
use axum::{
    Json,
    extract::{Extension, Path, State, rejection::JsonRejection},
    response::Response,
};
use base64::Engine as _;
use http::{
    HeaderValue, StatusCode,
    header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE},
};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use utoipa::ToSchema;

use crate::{
    chat_completions::HttpState,
    management_channel_writes::map_write_error,
    management_channels::{
        map_channel_read_error, no_store_empty, no_store_json, parse_channel_id,
        parse_credential_id, status_json,
    },
    management_credentials::AdminCredentialResponse,
    management_error::ManagementError,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
/// 管理端创建或完整更新凭据时使用的配置正文。
pub(crate) struct AdminCredentialWriteRequest {
    kind: CredentialKind,
    #[serde(default)]
    secret: AdminCredentialSecretInput,
    status: AdminRoutingWriteStatus,
    multi_key_mode: Option<AdminCredentialMultiKeyMode>,
    priority: i32,
    weight: i32,
    concurrency: Option<i32>,
    load_factor_micros: Option<i64>,
    rate_multiplier_micros: Option<i64>,
    schedulable: bool,
    parent_id: Option<i64>,
    quota_dimension: AdminCredentialQuotaDimension,
    proxy_id: Option<i64>,
    oauth_provider: Option<String>,
    oauth_account_key: Option<String>,
    oauth_project_id: Option<String>,
}

const MAX_IMPORT_FILES: usize = 64;
const MAX_IMPORT_FILE_BYTES: usize = 2 * 1024 * 1024;
const MAX_IMPORT_ITEMS: usize = 2_048;
const MAX_IMPORT_TOTAL_BYTES: usize = 24 * 1024 * 1024;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdminCredentialImportRequest {
    files: Vec<AdminCredentialImportFile>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct AdminCredentialImportFile {
    name: String,
    content: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
struct AdminCredentialImportItem {
    file: String,
    index: usize,
    action: &'static str,
    credential_id: Option<i64>,
    message: &'static str,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct AdminCredentialImportResponse {
    total: usize,
    created: usize,
    skipped: usize,
    failed: usize,
    items: Vec<AdminCredentialImportItem>,
}

#[derive(Default)]
struct ImportedOAuthToken {
    access_token: String,
    refresh_token: Option<String>,
    expires_at_epoch_seconds: Option<i64>,
    scope: Option<String>,
    account_key: Option<String>,
    project_id: Option<String>,
}

/// 从 sub2api 常见的 JSON、JSON 数组、逐行 JSON 和逐行 token 中批量提取 Codex token。
fn parse_import_file(content: &str) -> Result<Vec<ImportedOAuthToken>, &'static str> {
    if content.len() > MAX_IMPORT_FILE_BYTES || content.trim().is_empty() {
        return Err("文件为空或超过大小限制");
    }
    let mut values = Vec::new();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(content) {
        collect_import_value(&value, &mut values);
    } else {
        for line in content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                collect_import_value(&value, &mut values);
            } else if line.starts_with('{') || line.starts_with('[') {
                return Err("JSON 文件格式无效");
            } else {
                let mut item = ImportedOAuthToken {
                    access_token: line.to_owned(),
                    ..ImportedOAuthToken::default()
                };
                merge_access_token_claims(&mut item);
                values.push(item);
            }
        }
    }
    if values.is_empty() {
        return Err("未找到可用的 access token");
    }
    if values.len() > MAX_IMPORT_ITEMS {
        return Err("单次导入条目过多");
    }
    Ok(values)
}

fn collect_import_value(value: &serde_json::Value, output: &mut Vec<ImportedOAuthToken>) {
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .for_each(|item| collect_import_value(item, output)),
        serde_json::Value::String(token) => {
            let mut item = ImportedOAuthToken {
                access_token: token.clone(),
                ..ImportedOAuthToken::default()
            };
            merge_access_token_claims(&mut item);
            output.push(item);
        }
        serde_json::Value::Object(object) => {
            if object
                .get("platform")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|platform| !platform.eq_ignore_ascii_case("openai"))
            {
                return;
            }
            if let Some(accounts) = object.get("accounts").and_then(serde_json::Value::as_array) {
                accounts
                    .iter()
                    .for_each(|item| collect_import_value(item, output));
                return;
            }
            if let Some(tokens) = object.get("tokens")
                && let Some(items) = tokens.as_array()
            {
                items
                    .iter()
                    .for_each(|item| collect_import_value(item, output));
                return;
            }
            let credentials = object
                .get("credentials")
                .and_then(serde_json::Value::as_object);
            let nested = object.get("tokens").and_then(serde_json::Value::as_object);
            let source = nested.or(credentials).unwrap_or(object);
            let access_token = first_string(source, &["access_token", "accessToken", "token"])
                .or_else(|| first_string(object, &["access_token", "accessToken", "token"]));
            let Some(access_token) = access_token else {
                return;
            };
            let account = object.get("account").and_then(serde_json::Value::as_object);
            let account_keys = [
                "chatgpt_account_id",
                "chatgptAccountId",
                "account_id",
                "accountId",
            ];
            let account_key = first_string(source, &account_keys)
                .or_else(|| first_string(object, &account_keys))
                .or_else(|| {
                    account.and_then(|value| {
                        first_string(
                            value,
                            &[
                                "id",
                                "account_id",
                                "accountId",
                                "chatgpt_account_id",
                                "chatgptAccountId",
                            ],
                        )
                    })
                });
            let organization_keys = ["organization_id", "organizationId", "org_id", "orgId"];
            let project_id = first_string(source, &organization_keys)
                .or_else(|| first_string(object, &organization_keys))
                .or_else(|| {
                    account.and_then(|value| {
                        first_string(
                            value,
                            &["organization_id", "organizationId", "org_id", "orgId"],
                        )
                    })
                });
            let mut item = ImportedOAuthToken {
                access_token,
                refresh_token: first_string(source, &["refresh_token", "refreshToken"])
                    .or_else(|| first_string(object, &["refresh_token", "refreshToken"])),
                expires_at_epoch_seconds: parse_expiration(
                    source
                        .get("expires_at")
                        .or_else(|| source.get("expires_at_epoch_seconds"))
                        .or_else(|| source.get("expiresAt"))
                        .or_else(|| {
                            object
                                .get("expires_at")
                                .or_else(|| object.get("expires_at_epoch_seconds"))
                                .or_else(|| object.get("expiresAt"))
                        }),
                ),
                scope: first_string(source, &["scope"])
                    .or_else(|| first_string(object, &["scope"])),
                account_key,
                project_id,
            };
            merge_access_token_claims(&mut item);
            if item.account_key.is_none()
                && let Some(id_token) = first_string(source, &["id_token", "idToken"])
                    .or_else(|| first_string(object, &["id_token", "idToken"]))
            {
                merge_id_token_claims(&mut item, &id_token);
            }
            output.push(item);
        }
        _ => {}
    }
}

fn first_string(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter().find_map(|key| {
        object
            .get(*key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    })
}

fn parse_expiration(value: Option<&serde_json::Value>) -> Option<i64> {
    match value {
        Some(serde_json::Value::Number(number)) => number.as_i64().map(normalize_epoch),
        Some(serde_json::Value::String(value)) => {
            value.parse::<i64>().ok().map(normalize_epoch).or_else(|| {
                OffsetDateTime::parse(value, &Rfc3339)
                    .ok()
                    .map(|date| date.unix_timestamp())
            })
        }
        _ => None,
    }
}

fn normalize_epoch(value: i64) -> i64 {
    if value > 1_000_000_000_000 {
        value / 1_000
    } else {
        value
    }
}

fn merge_access_token_claims(item: &mut ImportedOAuthToken) {
    let access_token = item.access_token.clone();
    merge_id_token_claims(item, &access_token);
    if item.expires_at_epoch_seconds.is_none() {
        item.expires_at_epoch_seconds = decode_jwt_claims(&access_token)
            .and_then(|claims| claims.get("exp").and_then(serde_json::Value::as_i64));
    }
}

fn merge_id_token_claims(item: &mut ImportedOAuthToken, id_token: &str) {
    let Some(value) = decode_jwt_claims(id_token) else {
        return;
    };
    if item.account_key.is_none() {
        item.account_key = value
            .get("https://api.openai.com/auth")
            .and_then(serde_json::Value::as_object)
            .and_then(|auth| first_string(auth, &["chatgpt_account_id", "chatgptAccountId"]))
            .or_else(|| {
                value.as_object().and_then(|claims| {
                    first_string(claims, &["https://api.openai.com/auth.chatgpt_account_id"])
                })
            });
    }
    if item.project_id.is_none() {
        item.project_id = value
            .get("https://api.openai.com/auth")
            .and_then(serde_json::Value::as_object)
            .and_then(|auth| {
                first_string(
                    auth,
                    &["organization_id", "organizationId", "org_id", "orgId"],
                )
            });
    }
}

fn decode_jwt_claims(token: &str) -> Option<serde_json::Value> {
    let mut parts = token.split('.');
    let (Some(_header), Some(payload), Some(_signature)) =
        (parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    if parts.next().is_some() || payload.len() > 12 * 1_024 {
        return None;
    }
    let Ok(bytes) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload) else {
        return None;
    };
    serde_json::from_slice(&bytes).ok()
}

/// 区分字段缺失、显式 null 与有效明文，避免 Serde 将缺失字段折叠为 None。
#[derive(Default)]
enum AdminCredentialSecretInput {
    #[default]
    Missing,
    Present(Option<AdminCredentialSecretValue>),
}

impl<'de> Deserialize<'de> for AdminCredentialSecretInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<AdminCredentialSecretValue>::deserialize(deserializer).map(Self::Present)
    }
}

/// 管理端允许写入的闭合凭据明文对象；未知字段由 Serde 直接拒绝。
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum AdminCredentialSecretValue {
    ApiKey {
        api_key: String,
    },
    Oauth {
        access_token: String,
        #[serde(default)]
        refresh_token: Option<String>,
        #[serde(default)]
        expires_at_epoch_seconds: Option<i64>,
        #[serde(default)]
        scope: Option<String>,
    },
    Bedrock {
        access_key_id: String,
        secret_access_key: String,
        #[serde(default)]
        session_token: Option<String>,
    },
    ServiceAccount {
        client_email: String,
        #[serde(default)]
        private_key_id: Option<String>,
        private_key: String,
    },
}

impl AdminCredentialSecretValue {
    fn into_secret(self) -> Result<AdminCredentialSecret, ManagementError> {
        let result = match self {
            Self::ApiKey { api_key } => AdminCredentialSecret::api_key(api_key),
            Self::Oauth {
                access_token,
                refresh_token,
                expires_at_epoch_seconds,
                scope,
            } => PlainOAuthCredential::new(
                access_token,
                refresh_token,
                expires_at_epoch_seconds,
                scope,
            )
            .map(AdminCredentialSecret::oauth_bundle),
            Self::Bedrock {
                access_key_id,
                secret_access_key,
                session_token,
            } => AdminCredentialSecret::bedrock(access_key_id, secret_access_key, session_token),
            Self::ServiceAccount {
                client_email,
                private_key_id,
                private_key,
            } => AdminCredentialSecret::service_account(client_email, private_key_id, private_key),
        };
        result.map_err(|_| ManagementError::InvalidRequest)
    }
}

/// 创建凭据；OAuth 可不带明文进入待授权态，其余明文仅用于生成密文。
pub(crate) async fn create_admin_credential(
    State(state): State<HttpState>,
    Path(channel_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminCredentialWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let channel_id = parse_channel_id(&channel_id)?;
    let Json(mut request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    normalize_openai_oauth_provider(&state, authentication.principal(), channel_id, &mut request)
        .await?;
    let writer = state
        .admin_channel_writer
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let credential = writer
        .create_credential(
            authentication.principal(),
            channel_id,
            request.into_create_command()?,
        )
        .await
        .map_err(map_write_error)?;
    Ok(status_json(
        StatusCode::CREATED,
        AdminCredentialResponse::from_credential(&credential),
    ))
}

/// 批量导入 sub2api/Codex 本地令牌文件；每个条目单独返回结果，失败不影响同批次其他条目。
pub(crate) async fn import_admin_credentials(
    State(state): State<HttpState>,
    Path(channel_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminCredentialImportRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let channel_id = parse_channel_id(&channel_id)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    if request.files.is_empty()
        || request.files.len() > MAX_IMPORT_FILES
        || request
            .files
            .iter()
            .any(|file| file.content.len() > MAX_IMPORT_FILE_BYTES || file.name.len() > 255)
        || request
            .files
            .iter()
            .map(|file| file.content.len())
            .sum::<usize>()
            > MAX_IMPORT_TOTAL_BYTES
    {
        return Err(ManagementError::InvalidRequest);
    }
    let reader = state
        .admin_channel_reader
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let channel = reader
        .get_channel(authentication.principal(), channel_id)
        .await
        .map_err(map_channel_read_error)?;
    if channel.channel_type() != ChannelType::OpenAi
        || channel.protocol() != Protocol::OpenAiResponses
    {
        return Err(ManagementError::InvalidRequest);
    }
    let writer = state
        .admin_channel_writer
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let mut seen = std::collections::HashSet::<[u8; 32]>::new();
    let mut response = AdminCredentialImportResponse {
        total: 0,
        created: 0,
        skipped: 0,
        failed: 0,
        items: Vec::new(),
    };
    for file in request.files {
        let tokens = match parse_import_file(&file.content) {
            Ok(tokens) => tokens,
            Err(message) => {
                response.failed += 1;
                response.items.push(AdminCredentialImportItem {
                    file: file.name,
                    index: 0,
                    action: "failed",
                    credential_id: None,
                    message,
                });
                continue;
            }
        };
        for (index, token) in tokens.into_iter().enumerate() {
            response.total += 1;
            if response.total > MAX_IMPORT_ITEMS {
                response.failed += 1;
                response.items.push(AdminCredentialImportItem {
                    file: file.name.clone(),
                    index,
                    action: "failed",
                    credential_id: None,
                    message: "单次导入条目过多",
                });
                continue;
            }
            let digest = Sha256::digest(token.access_token.as_bytes());
            let fingerprint: [u8; 32] = digest.into();
            if !seen.insert(fingerprint) {
                response.skipped += 1;
                response.items.push(AdminCredentialImportItem {
                    file: file.name.clone(),
                    index,
                    action: "skipped",
                    credential_id: None,
                    message: "批次内重复令牌",
                });
                continue;
            }
            let secret = match PlainOAuthCredential::new(
                token.access_token,
                token.refresh_token,
                token.expires_at_epoch_seconds,
                token.scope,
            ) {
                Ok(secret) => AdminCredentialSecret::oauth_bundle(secret),
                Err(_) => {
                    response.failed += 1;
                    response.items.push(AdminCredentialImportItem {
                        file: file.name.clone(),
                        index,
                        action: "failed",
                        credential_id: None,
                        message: "令牌格式无效",
                    });
                    continue;
                }
            };
            let command = match AdminCredentialCreateCommand::new(
                CredentialKind::Oauth,
                secret,
                AdminRoutingWriteStatus::Enabled,
                None,
                0,
                10,
                None,
                None,
                None,
                true,
                None,
                AdminCredentialQuotaDimension::Global,
                None,
                Some("codex".to_owned()),
                token.account_key,
                token.project_id,
            ) {
                Ok(command) => command,
                Err(_) => {
                    response.failed += 1;
                    response.items.push(AdminCredentialImportItem {
                        file: file.name.clone(),
                        index,
                        action: "failed",
                        credential_id: None,
                        message: "令牌配置无效",
                    });
                    continue;
                }
            };
            match writer
                .create_credential(authentication.principal(), channel_id, command)
                .await
            {
                Ok(credential) => {
                    response.created += 1;
                    response.items.push(AdminCredentialImportItem {
                        file: file.name.clone(),
                        index,
                        action: "created",
                        credential_id: Some(credential.credential_id().get()),
                        message: "导入成功",
                    });
                }
                Err(_) => {
                    response.failed += 1;
                    response.items.push(AdminCredentialImportItem {
                        file: file.name.clone(),
                        index,
                        action: "failed",
                        credential_id: None,
                        message: "保存令牌失败",
                    });
                }
            }
        }
    }
    Ok(no_store_json(response))
}

async fn normalize_openai_oauth_provider(
    state: &HttpState,
    principal: af_admin::SessionPrincipal,
    channel_id: af_domain::ChannelId,
    request: &mut AdminCredentialWriteRequest,
) -> Result<(), ManagementError> {
    if request.kind != CredentialKind::Oauth {
        return Ok(());
    }
    let reader = state
        .admin_channel_reader
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let channel = reader
        .get_channel(principal, channel_id)
        .await
        .map_err(map_channel_read_error)?;
    if channel.channel_type() == ChannelType::OpenAi {
        if channel.protocol() != Protocol::OpenAiResponses {
            return Err(ManagementError::InvalidRequest);
        }
        // OpenAI OAuth 使用固定 Codex 协议；忽略旧客户端传入的可配置 provider，
        // 这样新旧管理端都不会再要求部署者维护 OAuth Provider 配置。
        request.oauth_provider = Some("codex".to_owned());
    }
    Ok(())
}

#[derive(Serialize)]
struct Sub2ApiExportTokens<'a> {
    access_token: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    refresh_token: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<&'a str>,
}

#[derive(Serialize)]
struct Sub2ApiExportRecord<'a> {
    tokens: Sub2ApiExportTokens<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chatgpt_account_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    organization_id: Option<&'a str>,
}

/// 导出兼容 sub2api 的 OAuth 令牌数组；响应禁止缓存并仅对管理员开放。
pub(crate) async fn export_admin_credentials(
    State(state): State<HttpState>,
    Path(channel_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let channel_id = parse_channel_id(&channel_id)?;
    let reader = state
        .admin_channel_reader
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let channel = reader
        .get_channel(authentication.principal(), channel_id)
        .await
        .map_err(map_channel_read_error)?;
    if channel.channel_type() != ChannelType::OpenAi {
        return Err(ManagementError::InvalidRequest);
    }
    let writer = state
        .admin_channel_writer
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let credentials = writer
        .export_oauth_credentials(authentication.principal(), channel_id)
        .await
        .map_err(map_write_error)?;
    let exports: Vec<Sub2ApiExportRecord<'_>> = credentials
        .iter()
        .map(|item: &AdminCredentialExport| Sub2ApiExportRecord {
            tokens: Sub2ApiExportTokens {
                access_token: item.secret().access_token(),
                refresh_token: item.secret().refresh_token(),
                expires_at: item.secret().expires_at_epoch_seconds(),
                scope: item.secret().scope(),
            },
            chatgpt_account_id: item.oauth_account_key(),
            organization_id: item.oauth_project_id(),
        })
        .collect();
    let body = serde_json::to_vec(&exports).map_err(|_| ManagementError::Internal)?;
    let mut response = Response::new(axum::body::Body::from(body));
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=anyflows-codex-credentials.json"),
    );
    Ok(response)
}

/// 完整更新凭据配置；secret 为空时保留已有密文。
pub(crate) async fn update_admin_credential(
    State(state): State<HttpState>,
    Path((channel_id, credential_id)): Path<(String, String)>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminCredentialWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let channel_id = parse_channel_id(&channel_id)?;
    let credential_id = parse_credential_id(&credential_id)?;
    let Json(mut request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    normalize_openai_oauth_provider(&state, authentication.principal(), channel_id, &mut request)
        .await?;
    let writer = state
        .admin_channel_writer
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let credential = writer
        .update_credential(
            authentication.principal(),
            channel_id,
            credential_id,
            request.into_update_command()?,
        )
        .await
        .map_err(map_write_error)?;
    Ok(no_store_json(AdminCredentialResponse::from_credential(
        &credential,
    )))
}

/// 安全软删除凭据及其影子后代。
pub(crate) async fn delete_admin_credential(
    State(state): State<HttpState>,
    Path((channel_id, credential_id)): Path<(String, String)>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let channel_id = parse_channel_id(&channel_id)?;
    let credential_id = parse_credential_id(&credential_id)?;
    let writer = state
        .admin_channel_writer
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    writer
        .delete_credential(authentication.principal(), channel_id, credential_id)
        .await
        .map_err(map_write_error)?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
}

impl AdminCredentialWriteRequest {
    fn into_create_command(self) -> Result<AdminCredentialCreateCommand, ManagementError> {
        let secret = match self.secret {
            AdminCredentialSecretInput::Present(Some(secret)) => Some(secret.into_secret()?),
            AdminCredentialSecretInput::Present(None) => None,
            AdminCredentialSecretInput::Missing => return Err(ManagementError::InvalidRequest),
        };
        if self.quota_dimension == AdminCredentialQuotaDimension::Spark {
            if self.kind != CredentialKind::Oauth
                || secret.is_some()
                || self.proxy_id.is_some()
                || self.oauth_provider.is_some()
                || self.oauth_account_key.is_some()
                || self.oauth_project_id.is_some()
            {
                return Err(ManagementError::InvalidRequest);
            }
            let parent_id =
                parse_parent_id(self.parent_id)?.ok_or(ManagementError::InvalidRequest)?;
            return AdminCredentialCreateCommand::new_spark_shadow(
                self.status,
                self.multi_key_mode,
                self.priority,
                self.weight,
                self.concurrency,
                self.load_factor_micros,
                self.rate_multiplier_micros,
                self.schedulable,
                parent_id,
            )
            .map_err(map_write_error);
        }
        let args = (
            self.status,
            self.multi_key_mode,
            self.priority,
            self.weight,
            self.concurrency,
            self.load_factor_micros,
            self.rate_multiplier_micros,
            self.schedulable,
            parse_parent_id(self.parent_id)?,
            self.quota_dimension,
            self.proxy_id,
            self.oauth_provider,
            self.oauth_account_key,
            self.oauth_project_id,
        );
        match secret {
            Some(secret) => AdminCredentialCreateCommand::new(
                self.kind, secret, args.0, args.1, args.2, args.3, args.4, args.5, args.6, args.7,
                args.8, args.9, args.10, args.11, args.12, args.13,
            )
            .map_err(map_write_error),
            None => AdminCredentialCreateCommand::new_pending_oauth(
                self.kind, args.0, args.1, args.2, args.3, args.4, args.5, args.6, args.7, args.8,
                args.9, args.10, args.11, args.12, args.13,
            )
            .map_err(map_write_error),
        }
    }

    fn into_update_command(self) -> Result<AdminCredentialUpdateCommand, ManagementError> {
        let secret = match self.secret {
            AdminCredentialSecretInput::Missing => return Err(ManagementError::InvalidRequest),
            AdminCredentialSecretInput::Present(secret) => secret
                .map(AdminCredentialSecretValue::into_secret)
                .transpose()?,
        };
        AdminCredentialUpdateCommand::new(
            self.kind,
            secret,
            self.status,
            self.multi_key_mode,
            self.priority,
            self.weight,
            self.concurrency,
            self.load_factor_micros,
            self.rate_multiplier_micros,
            self.schedulable,
            parse_parent_id(self.parent_id)?,
            self.quota_dimension,
            self.proxy_id,
            self.oauth_provider,
            self.oauth_account_key,
            self.oauth_project_id,
        )
        .map_err(map_write_error)
    }
}

fn parse_parent_id(value: Option<i64>) -> Result<Option<CredentialId>, ManagementError> {
    value
        .map(|value| CredentialId::new(value).map_err(|_| ManagementError::InvalidRequest))
        .transpose()
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const SERVICE_ACCOUNT_PRIVATE_KEY: &str =
        include_str!("../../af-adapter/src/vertex/fixtures/service_account_private.pem");

    #[test]
    fn typed_secret_requests_accept_all_supported_shapes() {
        for (kind, secret) in [
            (
                "api_key",
                json!({"kind": "api_key", "api_key": "sk-private"}),
            ),
            (
                "oauth",
                json!({"kind": "oauth", "access_token": "oauth-private"}),
            ),
            (
                "bedrock",
                json!({
                    "kind": "bedrock",
                    "access_key_id": "AKIDEXAMPLE00000001",
                    "secret_access_key": "aws-secret-access-key",
                    "session_token": "aws-session-token"
                }),
            ),
            (
                "service_account",
                json!({
                    "kind": "service_account",
                    "client_email": "runtime@example.iam.gserviceaccount.com",
                    "private_key_id": "private-key-id",
                    "private_key": SERVICE_ACCOUNT_PRIVATE_KEY
                }),
            ),
        ] {
            let request =
                serde_json::from_value::<AdminCredentialWriteRequest>(request(kind, secret))
                    .expect("类型化凭据请求应可解析");
            assert!(request.into_create_command().is_ok());
        }
    }

    #[test]
    fn typed_secret_requests_cover_pending_oauth_and_missing_field_boundaries() {
        let mismatch = serde_json::from_value::<AdminCredentialWriteRequest>(request(
            "bedrock",
            json!({"kind": "api_key", "api_key": "sk-private"}),
        ))
        .unwrap();
        assert!(mismatch.into_create_command().is_err());

        let pending =
            serde_json::from_value::<AdminCredentialWriteRequest>(request("oauth", Value::Null))
                .unwrap();
        assert!(pending.into_create_command().is_ok());

        let non_oauth_pending =
            serde_json::from_value::<AdminCredentialWriteRequest>(request("api_key", Value::Null))
                .unwrap();
        assert!(non_oauth_pending.into_create_command().is_err());

        let service_account_with_token_uri = request(
            "service_account",
            json!({
                "kind": "service_account",
                "client_email": "runtime@example.iam.gserviceaccount.com",
                "private_key": SERVICE_ACCOUNT_PRIVATE_KEY,
                "token_uri": "https://untrusted.example/token"
            }),
        );
        assert!(
            serde_json::from_value::<AdminCredentialWriteRequest>(service_account_with_token_uri)
                .is_err()
        );

        let mut missing = request(
            "api_key",
            json!({"kind": "api_key", "api_key": "sk-private"}),
        );
        missing.as_object_mut().unwrap().remove("secret");
        let missing = serde_json::from_value::<AdminCredentialWriteRequest>(missing).unwrap();
        assert!(missing.into_update_command().is_err());
    }

    #[test]
    fn imports_codex_sessions_and_sub2api_account_backups() {
        let tokens = parse_import_file(
            &json!({
                "accounts": [
                    {
                        "platform": "openai",
                        "type": "oauth",
                        "credentials": {
                            "access_token": "access-one",
                            "refresh_token": "refresh-one",
                            "expires_at": "2026-09-25T12:00:00Z"
                        },
                        "chatgpt_account_id": "account-one"
                    },
                    {
                        "tokens": {
                            "access_token": "access-two",
                            "refresh_token": "refresh-two",
                            "expires_at": 1_800_000_000
                        },
                        "account": { "id": "account-two" }
                    }
                ]
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].account_key.as_deref(), Some("account-one"));
        assert_eq!(tokens[0].refresh_token.as_deref(), Some("refresh-one"));
        assert!(tokens[0].expires_at_epoch_seconds.is_some());
        assert_eq!(tokens[1].account_key.as_deref(), Some("account-two"));
        assert_eq!(tokens[1].expires_at_epoch_seconds, Some(1_800_000_000));
    }

    #[test]
    fn imports_line_delimited_tokens_and_rejects_empty_payloads() {
        let tokens = parse_import_file("access-one\n{\"accessToken\":\"access-two\"}\n").unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].access_token, "access-one");
        assert_eq!(tokens[1].access_token, "access-two");
        assert!(parse_import_file("[]").is_err());
        assert!(parse_import_file("{broken-json").is_err());
    }

    #[test]
    fn import_uses_chatgpt_account_id_and_normalizes_millisecond_expiry() {
        let payload = json!({
            "https://api.openai.com/auth": {"chatgpt_account_id": "account-from-jwt"},
            "exp": 1_800_000_000
        });
        let token = format!(
            "header.{}.signature",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string())
        );
        let tokens = parse_import_file(
            &json!({
                "tokens": {"access_token": token, "expires_at": 1_800_000_000_000_i64},
                "user_id": "different-user"
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(tokens[0].account_key.as_deref(), Some("account-from-jwt"));
        assert_eq!(tokens[0].expires_at_epoch_seconds, Some(1_800_000_000));
    }

    #[test]
    fn import_reads_sub2api_credentials_and_skips_other_platforms() {
        let tokens = parse_import_file(
            &json!({"accounts": [
                {"platform": "openai", "credentials": {
                    "access_token": "codex-access", "refresh_token": "codex-refresh",
                    "chatgpt_account_id": "codex-account"
                }},
                {"platform": "claude", "credentials": {"access_token": "other-access"}}
            ]})
            .to_string(),
        )
        .unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].account_key.as_deref(), Some("codex-account"));
        assert_eq!(tokens[0].refresh_token.as_deref(), Some("codex-refresh"));
    }

    fn request(kind: &str, secret: Value) -> Value {
        json!({
            "kind": kind,
            "secret": secret,
            "status": "enabled",
            "multi_key_mode": null,
            "priority": 0,
            "weight": 10,
            "concurrency": null,
            "load_factor_micros": null,
            "rate_multiplier_micros": null,
            "schedulable": true,
            "parent_id": null,
            "quota_dimension": "global",
            "proxy_id": null,
            "oauth_provider": if kind == "oauth" { Value::String("codex".to_owned()) } else { Value::Null },
            "oauth_account_key": null,
            "oauth_project_id": null
        })
    }
}
