//! 版本化客户端身份配置；认证和正文行为只能引用程序注册的实现。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ChannelType, CredentialKind, Protocol};

/// 渠道引用的不可变修订。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClientProfileBinding {
    pub key: String,
    pub revision: i64,
}

impl ClientProfileBinding {
    pub fn validate(&self) -> Result<(), ClientProfileError> {
        validate_client_profile_key(&self.key)?;
        if self.revision < 1 {
            return Err(ClientProfileError::InvalidRevision);
        }
        Ok(())
    }
}

/// 服务实现决定认证、地址和请求正文，不接受管理员脚本。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientProfileService {
    Standard,
    ClaudeCode,
    Codex,
    OpenCode,
}

/// 注册的正文处理行为。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientProfileBehavior {
    Standard,
    ClaudeUtcDateV1,
    CodexV1,
    OpenCodeV1,
}

/// 会话关联策略，不保存提示词或跨凭据复用私有响应状态。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientProfileSessionStrategy {
    Explicit,
    FirstUserMessage,
}

/// 无密钥材料、可校验和跨节点投影的档案正文。
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClientProfileDefinition {
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub service: ClientProfileService,
    pub protocols: Vec<Protocol>,
    pub credential_kinds: Vec<CredentialKind>,
    pub identity_headers: BTreeMap<String, String>,
    pub session_strategy: ClientProfileSessionStrategy,
    pub behavior: ClientProfileBehavior,
}

impl std::fmt::Debug for ClientProfileDefinition {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ClientProfileDefinition")
            .field("service", &self.service)
            .field("behavior", &self.behavior)
            .field("header_count", &self.identity_headers.len())
            .finish_non_exhaustive()
    }
}

impl ClientProfileDefinition {
    pub fn validate(&self) -> Result<(), ClientProfileError> {
        if !bounded_text(&self.name, 128, false)
            || !bounded_text(&self.description, 2048, true)
            || self.icon.as_deref().is_some_and(|icon| {
                icon.is_empty()
                    || icon.len() > 64
                    || !icon
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            })
        {
            return Err(ClientProfileError::InvalidText);
        }
        if self.protocols.is_empty()
            || self.protocols.len() > Protocol::ALL.len()
            || self.protocols.iter().collect::<BTreeSet<_>>().len() != self.protocols.len()
            || self.credential_kinds.is_empty()
            || self.credential_kinds.len() > CredentialKind::ALL.len()
            || self.credential_kinds.iter().collect::<BTreeSet<_>>().len()
                != self.credential_kinds.len()
        {
            return Err(ClientProfileError::Incompatible);
        }
        let valid_behavior = matches!(
            (self.service, self.behavior),
            (
                ClientProfileService::Standard,
                ClientProfileBehavior::Standard
            ) | (
                ClientProfileService::ClaudeCode,
                ClientProfileBehavior::Standard | ClientProfileBehavior::ClaudeUtcDateV1
            ) | (ClientProfileService::Codex, ClientProfileBehavior::CodexV1)
                | (
                    ClientProfileService::OpenCode,
                    ClientProfileBehavior::OpenCodeV1
                )
        );
        if !valid_behavior
            || self.protocols.iter().any(|p| !self.supports_protocol(*p))
            || self
                .credential_kinds
                .iter()
                .any(|kind| !self.supports_credential(*kind))
        {
            return Err(ClientProfileError::Incompatible);
        }
        if self.identity_headers.len() > 8 {
            return Err(ClientProfileError::InvalidHeader);
        }
        for (name, value) in &self.identity_headers {
            validate_client_identity_header(name, value)?;
        }
        Ok(())
    }

    fn supports_protocol(&self, protocol: Protocol) -> bool {
        match self.service {
            ClientProfileService::Standard => matches!(
                protocol,
                Protocol::OpenAiChat
                    | Protocol::OpenAiResponses
                    | Protocol::Anthropic
                    | Protocol::Gemini
            ),
            ClientProfileService::ClaudeCode => protocol == Protocol::Anthropic,
            ClientProfileService::Codex => protocol == Protocol::OpenAiResponses,
            ClientProfileService::OpenCode => matches!(
                protocol,
                Protocol::OpenAiChat | Protocol::OpenAiResponses | Protocol::Anthropic
            ),
        }
    }

    fn supports_credential(&self, kind: CredentialKind) -> bool {
        match self.service {
            ClientProfileService::Standard => true,
            ClientProfileService::ClaudeCode | ClientProfileService::Codex => {
                kind == CredentialKind::Oauth
            }
            ClientProfileService::OpenCode => kind == CredentialKind::ApiKey,
        }
    }

    pub fn matches_channel(&self, channel_type: ChannelType, protocol: Protocol) -> bool {
        self.protocols.contains(&protocol)
            && match self.service {
                ClientProfileService::Standard => true,
                ClientProfileService::ClaudeCode => channel_type == ChannelType::Anthropic,
                ClientProfileService::Codex => channel_type == ChannelType::OpenAi,
                ClientProfileService::OpenCode => channel_type == ChannelType::OpenCode,
            }
    }

    /// 服务身份除凭据种类外，还须匹配实际的 OAuth 提供商。
    pub fn matches_credential(&self, kind: CredentialKind, oauth_provider: Option<&str>) -> bool {
        self.credential_kinds.contains(&kind)
            && (self.service != ClientProfileService::Codex
                || (kind == CredentialKind::Oauth
                    && oauth_provider
                        .is_some_and(|provider| provider.eq_ignore_ascii_case("codex"))))
    }
}

/// 运行时只持有已验证的不可变配置，不在发送过程中读取数据库。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClientProfileSnapshot {
    pub binding: ClientProfileBinding,
    pub definition: ClientProfileDefinition,
}

impl ClientProfileSnapshot {
    pub fn validate(&self) -> Result<(), ClientProfileError> {
        self.binding.validate()?;
        self.definition.validate()
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ClientProfileError {
    #[error("客户端档案标识无效")]
    InvalidKey,
    #[error("客户端档案修订无效")]
    InvalidRevision,
    #[error("客户端档案文本无效")]
    InvalidText,
    #[error("客户端身份请求头无效或由服务托管")]
    InvalidHeader,
    #[error("客户端档案与服务、协议或凭据不兼容")]
    Incompatible,
}

pub fn validate_client_profile_key(key: &str) -> Result<(), ClientProfileError> {
    if key.is_empty()
        || key.len() > 64
        || !key.bytes().enumerate().all(|(index, b)| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || (index > 0 && matches!(b, b'-' | b'_'))
        })
    {
        return Err(ClientProfileError::InvalidKey);
    }
    Ok(())
}

/// 身份白名单与认证/协议/会话字段明确分离，拒绝大小写绕过及 CRLF。
pub fn validate_client_identity_header(name: &str, value: &str) -> Result<(), ClientProfileError> {
    if !matches!(
        name,
        "user-agent" | "x-app" | "x-client-name" | "x-client-version" | "x-opencode-client"
    ) || value.is_empty()
        || value.len() > 512
        || value.trim() != value
        || !value.bytes().all(|b| matches!(b, b' '..=b'~'))
    {
        return Err(ClientProfileError::InvalidHeader);
    }
    Ok(())
}

fn bounded_text(value: &str, max: usize, empty: bool) -> bool {
    (empty || !value.is_empty())
        && value.len() <= max
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

/// 受保护的初始修订；后续升级发布新修订，不覆写既有版本。
pub fn built_in_client_profiles() -> Vec<ClientProfileSnapshot> {
    let definitions = [
        (
            "standard",
            "标准请求",
            ClientProfileService::Standard,
            ClientProfileBehavior::Standard,
            vec![
                Protocol::OpenAiChat,
                Protocol::OpenAiResponses,
                Protocol::Anthropic,
                Protocol::Gemini,
            ],
            CredentialKind::ALL.to_vec(),
            vec![],
        ),
        (
            "claude-code",
            "Claude Code",
            ClientProfileService::ClaudeCode,
            ClientProfileBehavior::Standard,
            vec![Protocol::Anthropic],
            vec![CredentialKind::Oauth],
            vec![
                ("user-agent", "claude-cli/2.1.114 (external, sdk-cli)"),
                ("x-app", "cli"),
            ],
        ),
        (
            "claude-code-date",
            "Claude Code（UTC 日期）",
            ClientProfileService::ClaudeCode,
            ClientProfileBehavior::ClaudeUtcDateV1,
            vec![Protocol::Anthropic],
            vec![CredentialKind::Oauth],
            vec![
                ("user-agent", "claude-cli/2.1.114 (external, sdk-cli)"),
                ("x-app", "cli"),
            ],
        ),
        (
            "codex",
            "Codex",
            ClientProfileService::Codex,
            ClientProfileBehavior::CodexV1,
            vec![Protocol::OpenAiResponses],
            vec![CredentialKind::Oauth],
            vec![],
        ),
        (
            "opencode",
            "OpenCode",
            ClientProfileService::OpenCode,
            ClientProfileBehavior::OpenCodeV1,
            vec![
                Protocol::OpenAiChat,
                Protocol::OpenAiResponses,
                Protocol::Anthropic,
            ],
            vec![CredentialKind::ApiKey],
            vec![
                ("user-agent", "opencode/latest/2.0.29/cli"),
                ("x-opencode-client", "cli"),
            ],
        ),
    ];
    definitions
        .into_iter()
        .map(
            |(key, name, service, behavior, protocols, credential_kinds, headers)| {
                ClientProfileSnapshot {
                    binding: ClientProfileBinding {
                        key: key.to_owned(),
                        revision: 1,
                    },
                    definition: ClientProfileDefinition {
                        name: name.to_owned(),
                        description: String::new(),
                        icon: None,
                        service,
                        protocols,
                        credential_kinds,
                        identity_headers: headers
                            .into_iter()
                            .map(|(k, v)| (k.to_owned(), v.to_owned()))
                            .collect(),
                        session_strategy: ClientProfileSessionStrategy::Explicit,
                        behavior,
                    },
                }
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_ins_are_valid_and_authentication_headers_are_never_editable() {
        for profile in built_in_client_profiles() {
            profile.validate().unwrap();
        }
        for name in [
            "authorization",
            "Authorization",
            "cookie",
            "x-api-key",
            "host",
            "accept",
            "content-type",
            "x-opencode-session",
            "originator",
        ] {
            assert_eq!(
                validate_client_identity_header(name, "value"),
                Err(ClientProfileError::InvalidHeader)
            );
        }
        assert!(
            validate_client_identity_header("user-agent", "cli\r\nauthorization: secret").is_err()
        );
    }

    #[test]
    fn behavior_service_and_protocol_cannot_be_mixed() {
        let mut profile = built_in_client_profiles().remove(1);
        profile.definition.behavior = ClientProfileBehavior::OpenCodeV1;
        assert!(profile.validate().is_err());
        profile.definition.behavior = ClientProfileBehavior::Standard;
        profile.definition.protocols.push(Protocol::OpenAiChat);
        assert!(profile.validate().is_err());
    }

    #[test]
    fn deserialization_rejects_scripts_and_secret_fields() {
        let value = serde_json::to_value(&built_in_client_profiles()[0].definition).unwrap();
        for field in ["script", "authorization", "body_patch", "base_url"] {
            let mut copy = value.clone();
            copy[field] = serde_json::json!("untrusted");
            assert!(serde_json::from_value::<ClientProfileDefinition>(copy).is_err());
        }
    }

    #[test]
    fn codex_service_requires_its_registered_oauth_provider() {
        let codex = built_in_client_profiles().remove(3).definition;
        assert!(codex.matches_credential(CredentialKind::Oauth, Some("codex")));
        assert!(codex.matches_credential(CredentialKind::Oauth, Some("Codex")));
        assert!(!codex.matches_credential(CredentialKind::Oauth, None));
        assert!(!codex.matches_credential(CredentialKind::Oauth, Some("openai")));
        assert!(!codex.matches_credential(CredentialKind::ApiKey, Some("codex")));
    }
}
