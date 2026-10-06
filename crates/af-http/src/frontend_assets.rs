use std::sync::Arc;

use axum::{
    body::{Body, Bytes},
    extract::State,
    response::{IntoResponse, Response},
    routing::{MethodRouter, any},
};
use http::{
    HeaderMap, HeaderValue, Method, StatusCode, Uri,
    header::{
        CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE, ETAG, IF_NONE_MATCH, X_CONTENT_TYPE_OPTIONS,
    },
};
use sha2::{Digest, Sha256};

const INDEX_PATH: &str = "index.html";
const IMMUTABLE_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";
const REVALIDATE_CACHE_CONTROL: &str = "no-cache";

/// 可由 HTTP 层直接响应的只读前端资源。
#[derive(Clone, Debug)]
pub struct FrontendAsset {
    content: Bytes,
    etag: HeaderValue,
}

impl FrontendAsset {
    /// 从编译期静态字节创建资源，并预计算稳定的强 ETag。
    #[must_use]
    pub fn from_static(content: &'static [u8]) -> Self {
        let digest = Sha256::digest(content);
        let etag = HeaderValue::from_str(&format!("\"{digest:x}\""))
            .expect("SHA-256 摘要必须始终满足 ETag 头约束");
        Self {
            content: Bytes::from_static(content),
            etag,
        }
    }

    /// 从运行时字节创建资源，并预计算稳定的强 ETag。
    ///
    /// 运行时加载的模板资源使用这个构造器；传入的字节会被资源自身持有，
    /// 因此调用方可以在返回后释放原始缓冲区。
    #[must_use]
    pub fn from_bytes(content: impl Into<Bytes>) -> Self {
        let content = content.into();
        let digest = Sha256::digest(&content);
        let etag = HeaderValue::from_str(&format!("\"{digest:x}\""))
            .expect("SHA-256 摘要必须始终满足 ETag 头约束");
        Self { content, etag }
    }

    /// 返回共享的只读资源字节，不复制整个资源。
    #[must_use]
    pub fn content(&self) -> Bytes {
        self.content.clone()
    }

    fn etag(&self) -> &HeaderValue {
        &self.etag
    }
}

/// 向 HTTP 层提供编译期前端资源的只读端口。
pub trait FrontendAssetSource: Send + Sync + 'static {
    /// 按无前导斜杠的相对路径查找资源。
    fn asset(&self, path: &str) -> Option<FrontendAsset>;
}

#[derive(Clone)]
struct FrontendAssetState {
    source: Arc<dyn FrontendAssetSource>,
}

/// 构建仅接受 GET/HEAD 的前端兜底路由。
pub(crate) fn frontend_fallback(source: Arc<dyn FrontendAssetSource>) -> MethodRouter {
    any(serve_frontend_asset).with_state(FrontendAssetState { source })
}

async fn serve_frontend_asset(
    State(state): State<FrontendAssetState>,
    uri: Uri,
    method: Method,
    request_headers: HeaderMap,
) -> Response {
    if !matches!(method, Method::GET | Method::HEAD) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let request_path = uri.path().trim_start_matches('/');
    let Some((asset_path, asset)) = resolve_asset(state.source.as_ref(), request_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let immutable = asset_path.starts_with("assets/");
    asset_response(
        &asset_path,
        asset,
        immutable,
        method == Method::HEAD,
        &request_headers,
    )
}

fn resolve_asset(
    source: &dyn FrontendAssetSource,
    request_path: &str,
) -> Option<(String, FrontendAsset)> {
    if request_path.is_empty() {
        return source
            .asset(INDEX_PATH)
            .map(|asset| (INDEX_PATH.to_owned(), asset));
    }
    if is_reserved_path(request_path) {
        return None;
    }
    if let Some(asset) = source.asset(request_path) {
        return Some((request_path.to_owned(), asset));
    }
    if looks_like_file_path(request_path) {
        return None;
    }
    source
        .asset(INDEX_PATH)
        .map(|asset| (INDEX_PATH.to_owned(), asset))
}

fn is_reserved_path(path: &str) -> bool {
    // /api is the public API explorer page. Keep nested API endpoints reserved
    // so an unknown API request is never answered with the SPA shell.
    if matches!(path, "api" | "api/") {
        return false;
    }
    matches!(
        path.split('/').next(),
        Some("api" | "v1" | "healthz" | "readyz" | "metrics")
    )
}

fn looks_like_file_path(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|segment| segment.starts_with('.') || segment.contains('.'))
}

fn asset_response(
    asset_path: &str,
    asset: FrontendAsset,
    immutable: bool,
    head_only: bool,
    request_headers: &HeaderMap,
) -> Response {
    let cache_control = if immutable {
        IMMUTABLE_CACHE_CONTROL
    } else {
        REVALIDATE_CACHE_CONTROL
    };
    if if_none_match(request_headers, asset.etag()) {
        return Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(CACHE_CONTROL, cache_control)
            .header(ETAG, asset.etag())
            .header(X_CONTENT_TYPE_OPTIONS, "nosniff")
            .body(Body::empty())
            .expect("固定的前端 304 响应头必须有效");
    }

    let content = asset.content();
    let content_length = content.len();
    let content_type = mime_guess::from_path(asset_path).first_or_octet_stream();
    let body = if head_only {
        Body::empty()
    } else {
        Body::from(content)
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(CACHE_CONTROL, cache_control)
        .header(CONTENT_LENGTH, content_length)
        .header(CONTENT_TYPE, content_type.essence_str())
        .header(ETAG, asset.etag())
        .header(X_CONTENT_TYPE_OPTIONS, "nosniff")
        .body(body)
        .expect("固定的前端资源响应头必须有效")
}

fn if_none_match(headers: &HeaderMap, expected: &HeaderValue) -> bool {
    let Ok(expected) = expected.to_str() else {
        return false;
    };
    headers
        .get_all(IF_NONE_MATCH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .any(|candidate| candidate == "*" || weak_etag(candidate) == expected)
}

fn weak_etag(value: &str) -> &str {
    value.strip_prefix("W/").unwrap_or(value)
}
