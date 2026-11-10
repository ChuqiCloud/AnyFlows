use utoipa::{OpenApi, ToSchema};

use crate::management_error::ManagementErrorBody;
use crate::playground_image_history::{ImageHistoryItemDto, ImageHistoryListResponse};

#[derive(ToSchema)]
#[schema(as = PlaygroundImageHistoryUploadRequest)]
struct ImageHistoryUploadRequest {
    model: String,
    prompt: String,
    revised_prompt: Option<String>,
    request_id: Option<String>,
    #[schema(value_type = Option<String>, format = Binary)]
    image: Option<String>,
    image_url: Option<String>,
    #[schema(value_type = Option<String>, format = Binary)]
    thumbnail: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/playground/image-history",
    operation_id = "listPlaygroundImageHistory",
    tag = "生图广场",
    summary = "列出当前用户的图片历史",
    responses(
        (status = 200, description = "当前用户最近的图片历史", body = ImageHistoryListResponse),
        (status = 401, description = "登录状态无效", body = ManagementErrorBody),
    ),
    security(("bearerAuth" = []))
)]
fn list_playground_image_history() {}

#[utoipa::path(
    post,
    path = "/api/playground/image-history",
    operation_id = "savePlaygroundImageHistory",
    tag = "生图广场",
    summary = "保存一张图片到当前用户的历史",
    request_body(content = ImageHistoryUploadRequest, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "已保存的图片历史", body = ImageHistoryItemDto),
        (status = 400, description = "图片或元数据无效", body = ManagementErrorBody),
        (status = 401, description = "登录状态无效", body = ManagementErrorBody),
        (status = 409, description = "图片历史空间不足", body = ManagementErrorBody),
    ),
    security(("bearerAuth" = []))
)]
fn save_playground_image_history() {}

#[utoipa::path(
    get,
    path = "/api/playground/image-history/{image_id}/image",
    operation_id = "readPlaygroundHistoryImage",
    tag = "生图广场",
    summary = "读取当前用户历史中的图片",
    params(("image_id" = String, Path, description = "图片历史标识")),
    responses(
        (status = 200, description = "PNG、JPEG 或 WebP 图片字节"),
        (status = 404, description = "图片不存在或不属于当前用户", body = ManagementErrorBody),
    ),
    security(("bearerAuth" = []))
)]
fn read_playground_history_image() {}

#[utoipa::path(
    get,
    path = "/api/playground/image-history/{image_id}/thumbnail",
    operation_id = "readPlaygroundHistoryImageThumbnail",
    tag = "生图广场",
    summary = "读取当前用户历史图片的缩略图",
    params(("image_id" = String, Path, description = "图片历史标识")),
    responses(
        (status = 200, description = "PNG、JPEG 或 WebP 缩略图字节"),
        (status = 404, description = "图片不存在或不属于当前用户", body = ManagementErrorBody),
    ),
    security(("bearerAuth" = []))
)]
fn read_playground_history_image_thumbnail() {}

#[utoipa::path(
    delete,
    path = "/api/playground/image-history/{image_id}",
    operation_id = "deletePlaygroundImageHistory",
    tag = "生图广场",
    summary = "删除当前用户的一张图片历史",
    params(("image_id" = String, Path, description = "图片历史标识")),
    responses(
        (status = 204, description = "已删除"),
        (status = 404, description = "图片不存在或不属于当前用户", body = ManagementErrorBody),
    ),
    security(("bearerAuth" = []))
)]
fn delete_playground_image_history() {}

#[utoipa::path(
    delete,
    path = "/api/playground/image-history",
    operation_id = "clearPlaygroundImageHistory",
    tag = "生图广场",
    summary = "清空当前用户的图片历史",
    responses((status = 204, description = "已清空")),
    security(("bearerAuth" = []))
)]
fn clear_playground_image_history() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_playground_image_history,
        save_playground_image_history,
        read_playground_history_image,
        read_playground_history_image_thumbnail,
        delete_playground_image_history,
        clear_playground_image_history,
    ),
    components(schemas(
        ImageHistoryItemDto,
        ImageHistoryListResponse,
        ImageHistoryUploadRequest,
        ManagementErrorBody,
    ))
)]
struct PlaygroundImageHistoryApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    PlaygroundImageHistoryApi::openapi()
}
