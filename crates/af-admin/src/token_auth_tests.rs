use af_domain::{GatewayPrincipal, GroupId, TokenId, TokenModelPolicy, TrustedClientIp, UserId};

use crate::{
    ApiKeyDigest, PresentedApiKey, TokenAuthentication, TokenAuthenticationError,
    TokenAuthenticationFuture, TokenAuthenticator,
};

const TEST_KEY: &str = "sk-af-AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

struct FixedAuthenticator {
    result: Result<TokenAuthentication, TokenAuthenticationError>,
}

impl TokenAuthenticator for FixedAuthenticator {
    fn authenticate<'a>(
        &'a self,
        _digest: &'a ApiKeyDigest,
        _client_ip: TrustedClientIp,
    ) -> TokenAuthenticationFuture<'a> {
        let result = self.result.clone();
        Box::pin(async move { result })
    }
}

fn principal() -> GatewayPrincipal {
    GatewayPrincipal::new(
        TokenId::new(11).unwrap(),
        UserId::new(22).unwrap(),
        GroupId::new(33).unwrap(),
    )
}

fn authentication() -> TokenAuthentication {
    TokenAuthentication::new(principal(), TokenModelPolicy::unrestricted())
}

fn client_ip() -> TrustedClientIp {
    TrustedClientIp::new("192.0.2.10".parse().unwrap())
}

#[tokio::test]
async fn token_authenticator_is_object_safe_and_accepts_digest_with_trusted_ip() {
    let authenticator: &dyn TokenAuthenticator = &FixedAuthenticator {
        result: Ok(authentication()),
    };
    let presented = PresentedApiKey::parse(TEST_KEY).unwrap();
    let digest = presented.digest();
    drop(presented);

    assert_eq!(
        authenticator.authenticate(&digest, client_ip()).await,
        Ok(authentication())
    );
    assert_eq!(
        format!("{:?}", authentication()),
        "TokenAuthentication(<redacted>)"
    );
}

#[tokio::test]
async fn authentication_error_contract_is_closed_and_secret_free() {
    let authenticator: &dyn TokenAuthenticator = &FixedAuthenticator {
        result: Err(TokenAuthenticationError::InvalidApiKey),
    };
    let presented = PresentedApiKey::parse(TEST_KEY).unwrap();
    let digest = presented.digest();
    let secret = presented.expose_secret().to_owned();
    drop(presented);

    let error = authenticator
        .authenticate(&digest, client_ip())
        .await
        .unwrap_err();
    let rendered = format!("{error:?}\n{error}");
    assert_eq!(error, TokenAuthenticationError::InvalidApiKey);
    assert!(!rendered.contains(&secret));
    assert!(!rendered.contains(digest.as_str()));
    assert!(!rendered.contains("192.0.2.10"));
}
