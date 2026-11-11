use utoipa::OpenApi;

use crate::public_service_status::{PublicServiceStatusPoint, PublicServiceStatusResponse};

#[utoipa::path(
    get,
    path = "/api/public/service-status",
    operation_id = "getPublicServiceStatus",
    tag = "运行状态",
    summary = "读取脱敏的平台级请求状态",
    responses(
        (status = 200, description = "最近24小时的聚合状态；低于样本门槛时省略计数", body = PublicServiceStatusResponse),
        (status = 503, description = "状态数据暂不可用，响应不包含内部错误详情", body = PublicServiceStatusResponse)
    )
)]
fn get_public_service_status() {}

#[derive(OpenApi)]
#[openapi(
    paths(get_public_service_status),
    components(schemas(PublicServiceStatusResponse, PublicServiceStatusPoint))
)]
struct PublicServiceStatusApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    PublicServiceStatusApi::openapi()
}
