use std::{fmt, sync::Arc};

use af_adapter::{
    Adaptor, Bytes, Credential, HeaderMap, Method, RelayContext, ResponseMode, TransportDispatcher,
    UpstreamRequest, UpstreamResponse,
};
use af_domain::UpstreamError;

use crate::{
    RelayAttemptDiagnostic, RelayDiagnosticPolicy,
    error::{classify_upstream_status, map_adaptor_error, parse_retry_after},
};

/// 仅从已发送成功的请求冻结；消费一次，不持有候选目录、调度器或新的并发入口。
/// 身份头、地址、连接池和凭据归属在两轮间保持一致。
pub(crate) struct FrozenCandidate {
    adaptor: Arc<dyn Adaptor>,
    context: RelayContext,
    credential: Credential,
    transport: TransportDispatcher,
    method: Method,
    target: String,
    headers: HeaderMap,
    response_mode: ResponseMode,
    body_limit: usize,
    diagnostic_policy: Option<RelayDiagnosticPolicy>,
}

impl FrozenCandidate {
    pub(crate) fn new(
        adaptor: Arc<dyn Adaptor>,
        context: RelayContext,
        credential: Credential,
        transport: TransportDispatcher,
        request: &UpstreamRequest,
        diagnostic_policy: Option<RelayDiagnosticPolicy>,
    ) -> Self {
        Self {
            adaptor,
            context,
            credential,
            transport,
            method: request.method().clone(),
            target: request.target().to_owned(),
            headers: request.headers().clone(),
            response_mode: request.response_mode(),
            body_limit: request.response_body_limit(),
            diagnostic_policy,
        }
    }

    pub(crate) async fn send(
        self,
        body: Bytes,
    ) -> (
        Result<UpstreamResponse, UpstreamError>,
        Option<RelayAttemptDiagnostic>,
    ) {
        let request = UpstreamRequest::new(self.method, self.target, self.headers, Some(body))
            .and_then(|request| request.with_response_body_limit(self.body_limit))
            .map(|request| request.with_response_mode(self.response_mode));
        let request = match request {
            Ok(request) => request,
            Err(error) => return (Err(map_adaptor_error(error)), None),
        };
        // 续答仍经过服务的认证和正文约束；不重新查档案、选凭据或取得并发许可。
        let request = match self
            .adaptor
            .finalize_request(request, &self.credential, &self.context)
            .await
        {
            Ok(request) => request,
            Err(error) => return (Err(map_adaptor_error(error)), None),
        };
        let mut diagnostic = self
            .diagnostic_policy
            .map(|policy| RelayAttemptDiagnostic::capture_request(&request, policy));
        let result = async {
            let response = self
                .transport
                .send_with_report(request, &self.context)
                .await
                .map_err(map_adaptor_error)?
                .into_response();
            if let Some(diagnostic) = diagnostic.as_mut() {
                diagnostic.capture_response_head(&response);
            }
            if response.status() == af_adapter::StatusCode::OK {
                return Ok(response);
            }
            let status = response.status();
            let retry_after = parse_retry_after(response.headers());
            let body = response
                .into_body()
                .into_bytes()
                .await
                .map_err(map_adaptor_error)?;
            if let Some(diagnostic) = diagnostic.as_mut() {
                diagnostic.capture_buffered_response_body(&body);
            }
            Err(classify_upstream_status(status, retry_after, &body))
        }
        .await;
        (result, diagnostic)
    }
}

impl fmt::Debug for FrozenCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FrozenCandidate")
            .field("identity", &"<已脱敏>")
            .finish_non_exhaustive()
    }
}
