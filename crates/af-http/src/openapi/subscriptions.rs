//! 订阅计划管理与当前用户订阅读取 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    subscriptions::{
        AdminSubscriptionPlanCreateRequest, AdminSubscriptionPlanDisableRequest,
        AdminSubscriptionPlanListResponse, AdminSubscriptionPlanResponse,
        AdminUserSubscriptionBindRequest, AdminUserSubscriptionLifecycleActionDto,
        AdminUserSubscriptionLifecycleRequest, AdminUserSubscriptionLifecycleResponse,
        SubscriptionCycleDto, SubscriptionPlanStatusDto, UserSubscriptionListResponse,
        UserSubscriptionResponse, UserSubscriptionStatusDto,
    },
};

#[utoipa::path(
    get,
    path = "/api/admin/subscription-plans",
    operation_id = "listAdminSubscriptionPlans",
    tag = "订阅管理",
    summary = "读取订阅计划",
    params(
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾的计划主键游标"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 25")
    ),
    responses(
        (status = 200, description = "订阅计划列表", body = AdminSubscriptionPlanListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_subscription_plans() {}

#[utoipa::path(
    post,
    path = "/api/admin/subscription-plans",
    operation_id = "createAdminSubscriptionPlan",
    tag = "订阅管理",
    summary = "创建订阅计划",
    request_body = AdminSubscriptionPlanCreateRequest,
    responses(
        (status = 201, description = "订阅计划已创建", body = AdminSubscriptionPlanResponse),
        (status = 400, description = "计划名称、额度或周期无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 409, description = "计划业务事实发生冲突", body = ManagementErrorBody),
        (status = 503, description = "提交结果未知", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_admin_subscription_plan() {}

#[utoipa::path(
    post,
    path = "/api/admin/subscription-plans/{plan_id}/disable",
    operation_id = "disableAdminSubscriptionPlan",
    tag = "订阅管理",
    summary = "停用订阅计划",
    params(("plan_id" = String, Path, min_length = 32, max_length = 32, description = "计划标识")),
    request_body = AdminSubscriptionPlanDisableRequest,
    responses(
        (status = 200, description = "计划已停用或相同迁移已存在", body = AdminSubscriptionPlanResponse),
        (status = 400, description = "计划标识或版本无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "计划不存在", body = ManagementErrorBody),
        (status = 409, description = "计划版本冲突", body = ManagementErrorBody),
        (status = 503, description = "提交结果未知", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn disable_admin_subscription_plan() {}

#[utoipa::path(
    get,
    path = "/api/admin/users/{user_id}/subscriptions",
    operation_id = "listAdminUserSubscriptions",
    tag = "订阅管理",
    summary = "读取指定用户订阅",
    params(
        ("user_id" = i64, Path, minimum = 1, description = "用户 ID"),
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾的订阅主键游标"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 25")
    ),
    responses(
        (status = 200, description = "指定用户订阅列表", body = UserSubscriptionListResponse),
        (status = 400, description = "用户 ID 或分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_user_subscriptions() {}

#[utoipa::path(
    post,
    path = "/api/admin/users/{user_id}/subscriptions",
    operation_id = "bindAdminUserSubscription",
    tag = "订阅管理",
    summary = "给指定用户绑定订阅计划",
    params(("user_id" = i64, Path, minimum = 1, description = "用户 ID")),
    request_body = AdminUserSubscriptionBindRequest,
    responses(
        (status = 201, description = "订阅已绑定或相同绑定已恢复", body = UserSubscriptionResponse),
        (status = 400, description = "用户 ID、计划标识或请求正文无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "用户或计划不存在", body = ManagementErrorBody),
        (status = 409, description = "计划已停用或绑定事实冲突", body = ManagementErrorBody),
        (status = 503, description = "提交结果未知", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn bind_admin_user_subscription() {}

#[utoipa::path(
    post,
    path = "/api/admin/users/{user_id}/subscriptions/{subscription_id}/lifecycle",
    operation_id = "transitionAdminUserSubscriptionLifecycle",
    tag = "订阅管理",
    summary = "迁移指定用户订阅生命周期",
    params(
        ("user_id" = i64, Path, minimum = 1, description = "用户 ID"),
        ("subscription_id" = String, Path, min_length = 32, max_length = 32, description = "用户订阅标识")
    ),
    request_body = AdminUserSubscriptionLifecycleRequest,
    responses(
        (status = 200, description = "生命周期迁移已提交或相同迁移已存在", body = AdminUserSubscriptionLifecycleResponse),
        (status = 400, description = "用户 ID、订阅标识、动作或版本无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "需要管理员权限", body = ManagementErrorBody),
        (status = 404, description = "订阅不存在或不属于指定用户", body = ManagementErrorBody),
        (status = 409, description = "状态动作无效、版本冲突或存在在途计费预留", body = ManagementErrorBody),
        (status = 503, description = "提交结果未知", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn transition_admin_user_subscription_lifecycle() {}

#[utoipa::path(
    get,
    path = "/api/account/subscriptions",
    operation_id = "listCurrentUserSubscriptions",
    tag = "订阅管理",
    summary = "读取当前用户订阅",
    params(
        ("before" = Option<i64>, Query, minimum = 1, description = "上一页末尾的订阅主键游标"),
        ("limit" = Option<i32>, Query, minimum = 1, maximum = 100, description = "每页数量，默认 25")
    ),
    responses(
        (status = 200, description = "当前用户订阅列表", body = UserSubscriptionListResponse),
        (status = 400, description = "分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_current_user_subscriptions() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_subscription_plans,
        create_admin_subscription_plan,
        disable_admin_subscription_plan,
        list_admin_user_subscriptions,
        bind_admin_user_subscription,
        transition_admin_user_subscription_lifecycle,
        list_current_user_subscriptions
    ),
    components(schemas(
        SubscriptionCycleDto,
        SubscriptionPlanStatusDto,
        UserSubscriptionStatusDto,
        AdminSubscriptionPlanResponse,
        AdminSubscriptionPlanListResponse,
        AdminSubscriptionPlanCreateRequest,
        AdminSubscriptionPlanDisableRequest,
        UserSubscriptionResponse,
        UserSubscriptionListResponse,
        AdminUserSubscriptionBindRequest,
        AdminUserSubscriptionLifecycleActionDto,
        AdminUserSubscriptionLifecycleRequest,
        AdminUserSubscriptionLifecycleResponse,
        ManagementErrorBody
    ))
)]
struct SubscriptionsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    SubscriptionsApi::openapi()
}
