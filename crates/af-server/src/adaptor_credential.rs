use af_account::DecryptedCredential;
use af_adapter::{AdaptorError, Credential};
use af_domain::CredentialKind;

/// 将账号层脱敏凭据转换为适配器凭据，复杂类型始终保持结构化字段。
pub(crate) fn build_adaptor_credential(
    credential: &DecryptedCredential,
) -> Result<Credential, AdaptorError> {
    match credential.kind() {
        CredentialKind::ApiKey => Credential::api_key(credential.expose_secret()),
        CredentialKind::Oauth => Credential::oauth(credential.expose_secret()),
        CredentialKind::Bedrock => {
            let Some((access_key_id, secret_access_key, session_token)) =
                credential.bedrock_parts()
            else {
                return Err(AdaptorError::InvalidCredential);
            };
            Credential::bedrock(
                access_key_id,
                secret_access_key,
                session_token.map(str::to_owned),
            )
        }
        CredentialKind::ServiceAccount => {
            let Some((client_email, private_key_id, private_key)) =
                credential.service_account_parts()
            else {
                return Err(AdaptorError::InvalidCredential);
            };
            Credential::service_account(
                client_email,
                private_key_id.map(str::to_owned),
                private_key,
            )
        }
        CredentialKind::SetupToken | CredentialKind::Upstream => {
            Err(AdaptorError::UnsupportedCredential {
                kind: credential.kind(),
            })
        }
    }
}
