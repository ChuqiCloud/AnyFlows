use af_domain::ClientProfileSnapshot;
use af_httpclient::{HeaderMap, HeaderName, HeaderValue};

use crate::{AdaptorError, AdaptorResult, ClientSimulationContext, UpstreamRequest};

pub fn apply_client_profile_headers(
    profile: &ClientProfileSnapshot,
    context: ClientSimulationContext,
    oauth_provider: Option<&str>,
    headers: &mut HeaderMap,
) -> AdaptorResult<()> {
    profile
        .validate()
        .map_err(|_| AdaptorError::InvalidClientSimulation)?;
    if !profile
        .definition
        .matches_channel(context.channel_type(), context.protocol())
        || !profile
            .definition
            .matches_credential(context.credential_kind(), oauth_provider)
    {
        return Err(AdaptorError::InvalidClientSimulation);
    }
    for (name, value) in &profile.definition.identity_headers {
        headers.insert(
            HeaderName::try_from(name.as_str())
                .map_err(|_| AdaptorError::InvalidClientSimulation)?,
            HeaderValue::try_from(value.as_str())
                .map_err(|_| AdaptorError::InvalidClientSimulation)?,
        );
    }
    Ok(())
}

/// 只应用冻结档案中的身份字段；认证、协议和服务头仍由适配器托管。
pub fn apply_client_profile_identity(
    profile: &ClientProfileSnapshot,
    context: ClientSimulationContext,
    oauth_provider: Option<&str>,
    request: UpstreamRequest,
) -> AdaptorResult<UpstreamRequest> {
    let mut headers = HeaderMap::new();
    apply_client_profile_headers(profile, context, oauth_provider, &mut headers)?;
    request.with_header_overrides(headers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_domain::{ChannelType, CredentialKind, Operation, Protocol, built_in_client_profiles};
    use af_httpclient::{Bytes, Method};

    #[test]
    fn custom_identity_preserves_authentication_and_body() {
        let mut profile = built_in_client_profiles().remove(0);
        profile.binding.key = "custom".to_owned();
        profile
            .definition
            .identity_headers
            .insert("user-agent".to_owned(), "integration/1.0".to_owned());
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer secret-canary"),
        );
        let request = UpstreamRequest::new(
            Method::POST,
            "https://example.com/v1/chat/completions",
            headers,
            Some(Bytes::from_static(b"body-canary")),
        )
        .unwrap();
        let request = apply_client_profile_identity(
            &profile,
            ClientSimulationContext::new(
                ChannelType::OpenAi,
                Protocol::OpenAiChat,
                Operation::Chat,
                CredentialKind::ApiKey,
            ),
            None,
            request,
        )
        .unwrap();
        assert_eq!(request.headers()["authorization"], "Bearer secret-canary");
        assert_eq!(request.headers()["user-agent"], "integration/1.0");
        assert_eq!(request.body().unwrap().as_ref(), b"body-canary");
        profile
            .definition
            .identity_headers
            .insert("authorization".to_owned(), "Bearer replacement".to_owned());
        assert!(profile.validate().is_err());
    }
}
