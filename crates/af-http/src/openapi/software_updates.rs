use crate::software_updates::{
    InstallSoftwareUpdate, SoftwareRelease, SoftwareStatus, SoftwareUpdateErrorBody,
};
use utoipa::OpenApi;

#[utoipa::path(get, path = "/api/admin/software", operation_id = "getAdminSoftware", tag = "版本与更新", summary = "读取当前节点的软件版本与更新能力", responses((status = 200, body = SoftwareStatus)), security(("bearerAuth" = [])))]
fn status() {}

#[utoipa::path(post, path = "/api/admin/software/check", operation_id = "checkAdminSoftwareUpdate", tag = "版本与更新", summary = "手动检查公开版本，结果缓存一分钟", responses((status = 200, body = SoftwareStatus), (status = 403, body = SoftwareUpdateErrorBody), (status = 409, body = SoftwareUpdateErrorBody), (status = 502, body = SoftwareUpdateErrorBody)), security(("bearerAuth" = [])))]
fn check() {}

#[utoipa::path(post, path = "/api/admin/software/install", operation_id = "installAdminSoftwareUpdate", tag = "版本与更新", summary = "校验并替换当前节点二进制，完成后需要管理员重启", request_body = InstallSoftwareUpdate, responses((status = 200, body = SoftwareStatus), (status = 403, body = SoftwareUpdateErrorBody), (status = 409, body = SoftwareUpdateErrorBody), (status = 422, body = SoftwareUpdateErrorBody), (status = 500, body = SoftwareUpdateErrorBody), (status = 502, body = SoftwareUpdateErrorBody)), security(("bearerAuth" = [])))]
fn install() {}

#[derive(OpenApi)]
#[openapi(
    paths(status, check, install),
    components(schemas(
        SoftwareStatus,
        SoftwareRelease,
        InstallSoftwareUpdate,
        SoftwareUpdateErrorBody
    ))
)]
struct SoftwareUpdatesApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    SoftwareUpdatesApi::openapi()
}
