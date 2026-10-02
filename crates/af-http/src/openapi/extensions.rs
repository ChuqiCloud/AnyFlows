use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(crate::http_extensions::list_extension_catalog),
    components(schemas(crate::http_extensions::HttpExtensionDescriptor))
)]
struct ExtensionCatalogApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    ExtensionCatalogApi::openapi()
}
