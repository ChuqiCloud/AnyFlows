//! 当前用户充值订单创建 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    user_topups::{
        UserTopupOrderCreateRequest, UserTopupOrderResponse, UserTopupOrderStatusResponse,
    },
};

#[utoipa::path(
    post,
    path = "/api/account/wallet/topups",
    operation_id = "createUserTopupOrder",
    tag = "我的钱包",
    summary = "创建当前用户充值订单",
    request_body = UserTopupOrderCreateRequest,
    responses(
        (status = 201, description = "本地 Stripe 充值订单首次创建；尚未向 Provider 提交", body = UserTopupOrderResponse),
        (status = 200, description = "相同幂等键和金额对应的订单已经存在", body = UserTopupOrderResponse),
        (status = 400, description = "幂等键或 Stripe/USD 金额无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 409, description = "幂等键已经绑定不同充值事实", body = ManagementErrorBody),
        (status = 503, description = "Stripe 未配置、数据库暂不可用或提交结果未知", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_user_topup_order() {}

#[derive(OpenApi)]
#[openapi(
    paths(create_user_topup_order),
    components(schemas(
        UserTopupOrderCreateRequest,
        UserTopupOrderStatusResponse,
        UserTopupOrderResponse,
        ManagementErrorBody
    ))
)]
struct UserTopupsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    UserTopupsApi::openapi()
}
