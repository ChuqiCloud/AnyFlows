//! 首次安装管理员 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_session::{LoginResponse, SessionUser},
    management_setup::{SetupRequest, SetupStatusResponse},
};

#[utoipa::path(
    get,
    path = "/api/setup/status",
    operation_id = "getInitialSetupStatus",
    tag = "首次安装",
    summary = "读取首次安装状态",
    responses(
        (status = 200, description = "首次安装状态", body = SetupStatusResponse),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn get_initial_setup_status() {}

#[utoipa::path(
    post,
    path = "/api/setup",
    operation_id = "initializeAdminSetup",
    tag = "首次安装",
    summary = "创建首个管理员并签发管理会话",
    request_body = SetupRequest,
    responses(
        (status = 200, description = "安装及登录成功", body = LoginResponse),
        (status = 400, description = "请求正文或字段无效", body = ManagementErrorBody),
        (status = 409, description = "系统已经完成首次安装", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    )
)]
fn initialize_admin_setup() {}

#[derive(OpenApi)]
#[openapi(
    paths(get_initial_setup_status, initialize_admin_setup),
    components(schemas(
        SetupStatusResponse,
        SetupRequest,
        SessionUser,
        LoginResponse,
        ManagementErrorBody
    ))
)]
struct SetupApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    SetupApi::openapi()
}
