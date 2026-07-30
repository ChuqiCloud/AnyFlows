//! 上游凭据与订阅账号生命周期管理。

mod credential_decryption;
mod credential_encryption;
mod system_secret;

pub use credential_decryption::{
    CredentialDecryptionError, CredentialDecryptor, DecryptedCredential,
    MAX_DECRYPTED_CREDENTIAL_SECRET_BYTES, credential_plaintext_aad,
};
pub use credential_encryption::{
    CredentialEncryptionError, CredentialEncryptor, PlainCredentialSecret,
};
pub use system_secret::{
    DecryptedSystemSecret, PlainSystemSecret, SystemSecretCipher, SystemSecretError,
    SystemSecretKind,
};
