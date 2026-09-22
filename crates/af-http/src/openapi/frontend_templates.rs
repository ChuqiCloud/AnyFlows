//! 运行目录外部前端模板管理 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    frontend_templates::{
        FrontendTemplateActivationRequest, FrontendTemplateCatalog, FrontendTemplateSummary,
    },
    management_error::ManagementErrorBody,
};

#[utoipa::path(
    get,
    path = "/api/admin/frontend-templates",
    operation_id = "listAdminFrontendTemplates",
    tag = "前端模板",
    summary = "读取内置与外部前端模板目录",
    responses(
        (status = 200, description = "模板扫描快照", body = FrontendTemplateCatalog),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_frontend_templates() {}

#[utoipa::path(
    post,
    path = "/api/admin/frontend-templates/scan",
    operation_id = "scanAdminFrontendTemplates",
    tag = "前端模板",
    summary = "重新扫描外部前端模板目录",
    responses(
        (status = 200, description = "更新后的模板扫描快照", body = FrontendTemplateCatalog),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn scan_admin_frontend_templates() {}

#[utoipa::path(
    put,
    path = "/api/admin/frontend-templates/active",
    operation_id = "activateAdminFrontendTemplate",
    tag = "前端模板",
    summary = "切换当前前端模板",
    request_body = FrontendTemplateActivationRequest,
    responses(
        (status = 200, description = "切换后的模板扫描快照", body = FrontendTemplateCatalog),
        (status = 400, description = "模板标识无效或不存在", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 409, description = "模板选择持久化冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn activate_admin_frontend_template() {}

#[utoipa::path(
    get,
    path = "/api/admin/frontend-templates/{template_id}/preview",
    operation_id = "getAdminFrontendTemplatePreview",
    tag = "前端模板",
    summary = "读取管理员模板预览图片",
    params(("template_id" = String, Path, description = "模板稳定标识")),
    responses(
        (status = 200, description = "静态预览图片，不执行模板代码", body = Vec<u8>, content_type = ["image/png", "image/jpeg", "image/webp", "image/svg+xml"]),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 404, description = "模板不存在或没有预览图片"),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_admin_frontend_template_preview() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_frontend_templates,
        scan_admin_frontend_templates,
        activate_admin_frontend_template,
        get_admin_frontend_template_preview
    ),
    components(schemas(
        FrontendTemplateSummary,
        FrontendTemplateCatalog,
        FrontendTemplateActivationRequest,
        ManagementErrorBody
    ))
)]
struct FrontendTemplatesApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    FrontendTemplatesApi::openapi()
}
