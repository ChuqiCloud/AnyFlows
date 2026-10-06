use std::fmt;

use af_config::CredentialEncryptionSettings;
use af_db::{
    CREDENTIAL_ENVELOPE_NONCE_BYTES, DebugTraceSnapshotCipher, DebugTraceSnapshotCipherError,
    DebugTraceSnapshotContext, DebugTraceSnapshotKind, DebugTraceSnapshotPlaintext,
    EncryptedCredentialEnvelope,
};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead as _, KeyInit as _, Payload},
};
use zeroize::Zeroize as _;

use crate::credential_decryption::{CredentialKey, is_valid_key_id};

/// 复用启动主密钥的诊断快照加解密器，使用独立位置绑定 AAD 域。
#[derive(Clone)]
pub struct DiagnosticSnapshotCipher {
    key_id: String,
    key: CredentialKey,
}

impl DiagnosticSnapshotCipher {
    /// 从已校验的凭据主密钥配置构造诊断快照加解密器。
    pub fn new(
        settings: &CredentialEncryptionSettings,
    ) -> Result<Self, DebugTraceSnapshotCipherError> {
        let key_id = settings.key_id().ok_or(DebugTraceSnapshotCipherError)?;
        let encoded_key = settings.key().ok_or(DebugTraceSnapshotCipherError)?;
        if !is_valid_key_id(key_id) {
            return Err(DebugTraceSnapshotCipherError);
        }
        let key = CredentialKey::from_encoded(encoded_key.expose())
            .map_err(|_| DebugTraceSnapshotCipherError)?;
        Ok(Self {
            key_id: key_id.to_owned(),
            key,
        })
    }
}

impl DebugTraceSnapshotCipher for DiagnosticSnapshotCipher {
    fn encrypt(
        &self,
        context: DebugTraceSnapshotContext,
        plaintext: &str,
    ) -> Result<EncryptedCredentialEnvelope, DebugTraceSnapshotCipherError> {
        // 构造临时受保护值可统一复用数据库边界的 JSON 与大小校验。
        let plaintext = DebugTraceSnapshotPlaintext::new(plaintext.to_owned())?;
        let mut nonce = [0_u8; CREDENTIAL_ENVELOPE_NONCE_BYTES];
        getrandom::fill(&mut nonce).map_err(|_| DebugTraceSnapshotCipherError)?;
        let cipher = XChaCha20Poly1305::new_from_slice(self.key.as_slice())
            .map_err(|_| DebugTraceSnapshotCipherError)?;
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext.expose().as_bytes(),
                    aad: &diagnostic_snapshot_aad(context),
                },
            )
            .map_err(|_| DebugTraceSnapshotCipherError)?;
        EncryptedCredentialEnvelope::new(self.key_id.clone(), nonce, ciphertext)
            .map_err(|_| DebugTraceSnapshotCipherError)
    }

    fn decrypt(
        &self,
        context: DebugTraceSnapshotContext,
        envelope: &EncryptedCredentialEnvelope,
    ) -> Result<DebugTraceSnapshotPlaintext, DebugTraceSnapshotCipherError> {
        if envelope.key_id() != self.key_id {
            return Err(DebugTraceSnapshotCipherError);
        }
        let cipher = XChaCha20Poly1305::new_from_slice(self.key.as_slice())
            .map_err(|_| DebugTraceSnapshotCipherError)?;
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(envelope.nonce()),
                Payload {
                    msg: envelope.ciphertext(),
                    aad: &diagnostic_snapshot_aad(context),
                },
            )
            .map_err(|_| DebugTraceSnapshotCipherError)?;
        let value = String::from_utf8(plaintext).map_err(|error| {
            let mut plaintext = error.into_bytes();
            plaintext.zeroize();
            DebugTraceSnapshotCipherError
        })?;
        DebugTraceSnapshotPlaintext::new(value)
    }
}

impl fmt::Debug for DiagnosticSnapshotCipher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiagnosticSnapshotCipher")
            .field("key_id", &"<已脱敏>")
            .finish_non_exhaustive()
    }
}

fn diagnostic_snapshot_aad(context: DebugTraceSnapshotContext) -> Vec<u8> {
    let attempt = context
        .attempt_id()
        .map_or_else(|| "downstream".to_owned(), |id| id.to_string());
    let kind = match context.kind() {
        DebugTraceSnapshotKind::DownstreamHeaders => "downstream_headers",
        DebugTraceSnapshotKind::DownstreamBody => "downstream_body",
        DebugTraceSnapshotKind::AttemptRequestHeaders => "attempt_request_headers",
        DebugTraceSnapshotKind::AttemptRequestBody => "attempt_request_body",
        DebugTraceSnapshotKind::AttemptResponseHeaders => "attempt_response_headers",
        DebugTraceSnapshotKind::AttemptResponseBody => "attempt_response_body",
    };
    format!(
        "anyflows:debug-trace-snapshot:v1:trace={}:attempt={attempt}:kind={kind}",
        context.trace_id()
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use serde_json::json;

    use super::*;

    #[test]
    fn round_trip_is_bound_to_trace_attempt_and_field() {
        let cipher = DiagnosticSnapshotCipher::new(&settings()).unwrap();
        let context = DebugTraceSnapshotContext::new(
            7,
            Some(11),
            DebugTraceSnapshotKind::AttemptRequestHeaders,
        )
        .unwrap();
        let envelope = cipher
            .encrypt(
                context,
                r#"[{"name":"authorization","value":"<redacted>"}]"#,
            )
            .unwrap();
        assert_eq!(
            cipher.decrypt(context, &envelope).unwrap().expose(),
            r#"[{"name":"authorization","value":"<redacted>"}]"#
        );

        for wrong_context in [
            DebugTraceSnapshotContext::new(
                8,
                Some(11),
                DebugTraceSnapshotKind::AttemptRequestHeaders,
            )
            .unwrap(),
            DebugTraceSnapshotContext::new(
                7,
                Some(12),
                DebugTraceSnapshotKind::AttemptRequestHeaders,
            )
            .unwrap(),
            DebugTraceSnapshotContext::new(
                7,
                Some(11),
                DebugTraceSnapshotKind::AttemptResponseHeaders,
            )
            .unwrap(),
        ] {
            assert!(cipher.decrypt(wrong_context, &envelope).is_err());
        }
    }

    #[test]
    fn rejects_non_json_plaintext_and_hides_debug_material() {
        let cipher = DiagnosticSnapshotCipher::new(&settings()).unwrap();
        let context =
            DebugTraceSnapshotContext::new(7, None, DebugTraceSnapshotKind::DownstreamBody)
                .unwrap();
        assert!(cipher.encrypt(context, "not-json").is_err());
        assert!(!format!("{cipher:?}").contains("snapshot-test-key"));
    }

    fn settings() -> CredentialEncryptionSettings {
        serde_json::from_value(json!({
            "key_id": "snapshot-test-key",
            "key": URL_SAFE_NO_PAD.encode([0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES])
        }))
        .unwrap()
    }
}
