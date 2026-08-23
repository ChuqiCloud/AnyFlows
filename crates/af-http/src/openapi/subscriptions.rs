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
        SubscriptionCatalogPlanResponse, SubscriptionCatalogResponse, SubscriptionCycleDto,
        SubscriptionOrderCreateRequest, SubscriptionOrderPaymentRequest,
        SubscriptionOrderPaymentResponse, SubscriptionOrderResponse, SubscriptionOrderStatusDto,
        SubscriptionPaymentResponse, SubscriptionPlanStatusDto, UserSubscriptionListResponse,
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

#[utoipa::path(
    get,
    path = "/api/account/subscription-catalog",
    operation_id = "listCurrentSubscriptionCatalog",
    tag = "订阅管理",
    summary = "读取当前用户可售订阅目录",
    responses(
        (status = 200, description = "当前用户可购买的 Active 计划与价格快照", body = SubscriptionCatalogResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_current_subscription_catalog() {}

#[utoipa::path(
    post,
    path = "/api/account/subscription-orders",
    operation_id = "createCurrentSubscriptionOrder",
    tag = "订阅管理",
    summary = "幂等创建当前用户订阅订单",
    request_body = SubscriptionOrderCreateRequest,
    responses(
        (status = 201, description = "已创建待支付订阅订单", body = SubscriptionOrderResponse),
        (status = 200, description = "已重放同一幂等订单", body = SubscriptionOrderResponse),
        (status = 400, description = "订单请求或幂等键无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 404, description = "订阅计划不存在", body = ManagementErrorBody),
        (status = 409, description = "价格快照、销售状态或幂等事实冲突", body = ManagementErrorBody),
        (status = 503, description = "提交结果未知", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_current_subscription_order() {}

#[utoipa::path(
    get,
    path = "/api/account/subscription-orders/{order_id}",
    operation_id = "getCurrentSubscriptionOrder",
    tag = "订阅管理",
    summary = "查询当前用户订阅订单结果",
    params(("order_id" = String, Path, min_length = 32, max_length = 32, description = "订阅订单标识")),
    responses(
        (status = 200, description = "订阅订单服务端结果", body = SubscriptionOrderResponse),
        (status = 400, description = "订单标识无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 404, description = "订单不存在或不属于当前用户", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_current_subscription_order() {}

#[utoipa::path(
    post,
    path = "/api/account/subscription-orders/{order_id}/payment",
    operation_id = "submitCurrentSubscriptionOrderPayment",
    tag = "订阅管理",
    summary = "提交或恢复当前用户订阅订单支付",
    params(("order_id" = String, Path, min_length = 32, max_length = 32, description = "订阅订单标识")),
    request_body = SubscriptionOrderPaymentRequest,
    responses(
        (status = 200, description = "支付会话已创建或恢复", body = SubscriptionOrderPaymentResponse),
        (status = 400, description = "订单标识或支付方式无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 404, description = "订单不存在或不属于当前用户", body = ManagementErrorBody),
        (status = 409, description = "订单状态、Provider 或支付事实冲突", body = ManagementErrorBody),
        (status = 503, description = "支付 Provider 或提交结果暂不可用", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn submit_current_subscription_order_payment() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_subscription_plans,
        create_admin_subscription_plan,
        disable_admin_subscription_plan,
        list_admin_user_subscriptions,
        bind_admin_user_subscription,
        transition_admin_user_subscription_lifecycle,
        list_current_user_subscriptions,
        list_current_subscription_catalog,
        create_current_subscription_order,
        get_current_subscription_order,
        submit_current_subscription_order_payment
    ),
    components(schemas(
        SubscriptionCycleDto,
        SubscriptionPlanStatusDto,
        UserSubscriptionStatusDto,
        AdminSubscriptionPlanResponse,
        AdminSubscriptionPlanListResponse,
        AdminSubscriptionPlanCreateRequest,
        SubscriptionCatalogResponse,
        SubscriptionCatalogPlanResponse,
        SubscriptionOrderCreateRequest,
        SubscriptionOrderPaymentRequest,
        SubscriptionOrderPaymentResponse,
        SubscriptionPaymentResponse,
        SubscriptionOrderResponse,
        SubscriptionOrderStatusDto,
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
