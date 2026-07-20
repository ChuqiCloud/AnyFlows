use af_domain::{GatewayPrincipal, GroupId, TokenId, UserId};

use crate::{
    ApiKeyDigest, PresentedApiKey, TokenAuthenticationError, TokenAuthenticationFuture,
    TokenAuthenticator,
};

const TEST_KEY: &str = "sk-af-AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

struct FixedAuthenticator {
    result: Result<GatewayPrincipal, TokenAuthenticationError>,
}

impl TokenAuthenticator for FixedAuthenticator {
    fn authenticate<'a>(&'a self, _digest: &'a ApiKeyDigest) -> TokenAuthenticationFuture<'a> {
        let result = self.result;
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

#[tokio::test]
async fn token_authenticator_is_object_safe_and_accepts_only_digest() {
    let authenticator: &dyn TokenAuthenticator = &FixedAuthenticator {
        result: Ok(principal()),
    };
    let presented = PresentedApiKey::parse(TEST_KEY).unwrap();
    let digest = presented.digest();
    drop(presented);

    assert_eq!(authenticator.authenticate(&digest).await, Ok(principal()));
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

    let error = authenticator.authenticate(&digest).await.unwrap_err();
    let rendered = format!("{error:?}\n{error}");
    assert_eq!(error, TokenAuthenticationError::InvalidApiKey);
    assert!(!rendered.contains(&secret));
    assert!(!rendered.contains(digest.as_str()));
}
