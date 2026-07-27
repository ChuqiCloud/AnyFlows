use af_admin::{InitialSetupCommand, InitialSetupError, InitialSetupStatus};
use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    response::Response,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    chat_completions::HttpState,
    management_error::ManagementError,
    management_session::{login_with_credentials, no_store_json},
};

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SetupStatusResponse)]
pub(crate) struct SetupStatusResponse {
    setup_required: bool,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SetupRequest)]
pub(crate) struct SetupRequest {
    #[schema(min_length = 1, max_length = 64)]
    username: String,
    #[schema(min_length = 12, max_length = 128, format = Password, write_only)]
    password: String,
}

/// 返回失败关闭的首次安装状态。
pub(crate) async fn setup_status(
    State(state): State<HttpState>,
) -> Result<Response, ManagementError> {
    let setup = state
        .initial_setup
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let status = setup.status().await.map_err(map_setup_error)?;
    Ok(no_store_json(SetupStatusResponse {
        setup_required: status == InitialSetupStatus::Required,
    }))
}

/// 一次性创建首个管理员，并在成功后直接签发管理会话。
pub(crate) async fn initialize_setup(
    State(state): State<HttpState>,
    request: Result<Json<SetupRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = InitialSetupCommand::new(request.username.clone(), request.password.clone())
        .map_err(map_setup_error)?;
    let setup = state
        .initial_setup
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    setup.initialize(command).await.map_err(map_setup_error)?;

    // 安装成功必须立即形成可用会话；认证失败表示持久化不变量破坏，不对外伪装成密码错误。
    login_with_credentials(&state, request.username, request.password)
        .await
        .map_err(|error| match error {
            ManagementError::InvalidCredentials => ManagementError::Internal,
            other => other,
        })
}

fn map_setup_error(error: InitialSetupError) -> ManagementError {
    match error {
        InitialSetupError::InvalidInput => ManagementError::InvalidRequest,
        InitialSetupError::Conflict => ManagementError::SetupConflict,
        InitialSetupError::Internal => ManagementError::Internal,
    }
}
