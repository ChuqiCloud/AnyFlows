//! HTTP 路由、中间件、提取器与响应映射。

mod error_response;
mod middleware;
mod router;

pub use error_response::OpenAiHttpError;
pub use middleware::REQUEST_ID_HEADER_NAME;
pub use router::{DEFAULT_REQUEST_BODY_LIMIT_BYTES, build_router};

#[cfg(test)]
mod error_response_tests;
#[cfg(test)]
mod tests;
