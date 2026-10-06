//! 当前用户充值订单创建 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    user_topups::{
        UserTopupConfigurationResponse, UserTopupMethodResponse, UserTopupOrderCreateRequest,
        UserTopupOrderResponse, UserTopupOrderStatusResponse, UserTopupPaymentSessionResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/account/wallet/topups/config",
    operation_id = "getUserTopupConfiguration",
    tag = "我的钱包",
    summary = "读取当前用户充值配置",
    responses(
        (status = 200, description = "可用支付方式、客户端材料与金额边界；未配置在线支付时 methods 为空", body = UserTopupConfigurationResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_user_topup_configuration() {}

#[utoipa::path(
    post,
    path = "/api/account/wallet/topups",
    operation_id = "createUserTopupOrder",
    tag = "我的钱包",
    summary = "创建当前用户充值订单",
    request_body = UserTopupOrderCreateRequest,
    responses(
        (status = 201, description = "本地订单首次创建并已绑定支付动作", body = UserTopupOrderResponse),
        (status = 200, description = "相同幂等事实的订单已恢复并返回同一支付动作", body = UserTopupOrderResponse),
        (status = 400, description = "幂等键、支付方式或金额无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 409, description = "幂等键已经绑定不同充值事实", body = ManagementErrorBody),
        (status = 502, description = "支付 Provider 确定拒绝服务端请求", body = ManagementErrorBody),
        (status = 503, description = "支付方式未配置、数据库暂不可用或提交结果未知", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_user_topup_order() {}

#[derive(OpenApi)]
#[openapi(
    paths(get_user_topup_configuration, create_user_topup_order),
    components(schemas(
        UserTopupConfigurationResponse,
        UserTopupMethodResponse,
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
