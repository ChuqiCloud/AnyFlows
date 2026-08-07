//! 当前用户充值订单创建 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    user_topups::{
        UserTopupOrderCreateRequest, UserTopupOrderResponse, UserTopupOrderStatusResponse,
        UserTopupPaymentSessionResponse,
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
        (status = 201, description = "本地订单首次创建并已绑定 Stripe PaymentIntent", body = UserTopupOrderResponse),
        (status = 200, description = "相同幂等键和金额对应的订单已恢复；待支付时携带同一 PaymentIntent 会话", body = UserTopupOrderResponse),
        (status = 400, description = "幂等键或 Stripe/USD 金额无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 409, description = "幂等键已经绑定不同充值事实", body = ManagementErrorBody),
        (status = 502, description = "Stripe 确定拒绝服务端支付请求", body = ManagementErrorBody),
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
        UserTopupPaymentSessionResponse,
        ManagementErrorBody
    ))
)]
struct UserTopupsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    UserTopupsApi::openapi()
}
