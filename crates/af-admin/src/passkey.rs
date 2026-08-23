use std::fmt;

use serde_json::Value;
use zeroize::Zeroizing;

use crate::UserProfileError;

const MAX_CREDENTIAL_JSON_BYTES: usize = 64 * 1_024;
const MAX_DISPLAY_NAME_BYTES: usize = 128;
const MAX_PASSWORD_BYTES: usize = 4_096;
const MAX_TOTP_CODE_BYTES: usize = 128;

/// 账户安全页展示的 Passkey 记录，不包含公钥和注册状态。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserPasskey {
    id: i64,
    display_name: String,
    created_at: i64,
    last_used_at: Option<i64>,
    revoked_at: Option<i64>,
}

impl UserPasskey {
    pub(crate) fn new(
        id: i64,
        display_name: String,
        created_at: i64,
        last_used_at: Option<i64>,
        revoked_at: Option<i64>,
    ) -> Self {
        Self {
            id,
            display_name,
            created_at,
            last_used_at,
            revoked_at,
        }
    }

    /// 返回凭证内部标识。
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }

    /// 返回用户可编辑的展示名称。
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// 返回创建时间的 Unix 秒数。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    /// 返回最近一次使用时间的 Unix 秒数。
    #[must_use]
    pub const fn last_used_at(&self) -> Option<i64> {
        self.last_used_at
    }

    /// 返回撤销时间；非空表示凭证已不可用。
    #[must_use]
    pub const fn revoked_at(&self) -> Option<i64> {
        self.revoked_at
    }
}

/// 注册选项及其短期关联标识。
#[derive(Clone, Debug, PartialEq)]
pub struct PasskeyRegistrationOptions {
    options: Value,
    challenge_digest: String,
}

impl PasskeyRegistrationOptions {
    pub(crate) fn new(options: Value, challenge_digest: String) -> Self {
        Self {
            options,
            challenge_digest,
        }
    }

    /// 返回浏览器需要的 PublicKeyCredentialCreationOptions。
    #[must_use]
    pub fn options(&self) -> &Value {
        &self.options
    }

    /// 返回服务端关联标识；它不是原始挑战，不能单独完成注册。
    #[must_use]
    pub fn challenge_digest(&self) -> &str {
        &self.challenge_digest
    }
}

/// 完成 Passkey 注册的浏览器响应。
pub struct PasskeyRegistrationCommand {
    credential: Value,
    display_name: String,
}

impl PasskeyRegistrationCommand {
    /// 校验浏览器响应和用户展示名称边界。
    pub fn new(credential: Value, display_name: String) -> Result<Self, UserProfileError> {
        if !credential.is_object()
            || serde_json::to_vec(&credential)
                .map_or(true, |bytes| bytes.len() > MAX_CREDENTIAL_JSON_BYTES)
            || !valid_display_name(&display_name)
        {
            return Err(UserProfileError::InvalidInput);
        }
        Ok(Self {
            credential,
            display_name,
        })
    }

    pub(crate) fn into_parts(self) -> (Value, String) {
        (self.credential, self.display_name)
    }
}

impl fmt::Debug for PasskeyRegistrationCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PasskeyRegistrationCommand(<redacted>)")
    }
}

/// 修改 Passkey 展示名称的命令。
pub struct PasskeyRenameCommand {
    display_name: String,
}

impl PasskeyRenameCommand {
    /// 校验展示名称长度、空白和控制字符。
    pub fn new(display_name: String) -> Result<Self, UserProfileError> {
        if !valid_display_name(&display_name) {
            return Err(UserProfileError::InvalidInput);
        }
        Ok(Self { display_name })
    }

    pub(crate) fn into_display_name(self) -> String {
        self.display_name
    }
}

impl fmt::Debug for PasskeyRenameCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PasskeyRenameCommand(<redacted>)")
    }
}

/// 撤销 Passkey 的密码和二次验证命令。
pub struct PasskeyRevokeCommand {
    current_password: Zeroizing<String>,
    totp_code: Option<Zeroizing<String>>,
}

impl PasskeyRevokeCommand {
    /// 校验密码和可选 TOTP/备份码边界，并在内存中使用可清零容器保存。
    pub fn new(
        current_password: String,
        totp_code: Option<String>,
    ) -> Result<Self, UserProfileError> {
        if current_password.is_empty()
            || current_password.len() > MAX_PASSWORD_BYTES
            || current_password.chars().any(char::is_control)
            || totp_code.as_deref().is_some_and(|code| {
                code.is_empty()
                    || code.len() > MAX_TOTP_CODE_BYTES
                    || code.chars().any(char::is_control)
            })
        {
            return Err(UserProfileError::InvalidInput);
        }
        Ok(Self {
            current_password: Zeroizing::new(current_password),
            totp_code: totp_code.map(Zeroizing::new),
        })
    }

    pub(crate) fn into_parts(self) -> (Zeroizing<String>, Option<Zeroizing<String>>) {
        (self.current_password, self.totp_code)
    }
}

impl fmt::Debug for PasskeyRevokeCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PasskeyRevokeCommand(<redacted>)")
    }
}

fn valid_display_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_DISPLAY_NAME_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
