use crate::{ChannelType, CredentialKind, Protocol};

/// 渠道在领域层公开的能力契约。
///
/// 这里集中维护通信适配器支持的协议和运行阶段能力。品牌名称、凭据内容以及
/// 适配器内部的认证细节仍由各自模块负责。
impl ChannelType {
    /// 返回当前管理与生产装配接受的原生协议；不代表底层适配器的全部能力。
    #[must_use]
    pub const fn supports_protocol(self, protocol: Protocol) -> bool {
        match self {
            Self::OpenAi => matches!(
                protocol,
                Protocol::OpenAiChat
                    | Protocol::OpenAiResponses
                    | Protocol::OpenAiEmbeddings
                    | Protocol::OpenAiImages
                    | Protocol::OpenAiAudio
                    | Protocol::OpenAiSpeech
            ),
            Self::OpenCode => matches!(
                protocol,
                Protocol::OpenAiChat | Protocol::OpenAiResponses | Protocol::Anthropic
            ),
            Self::Anthropic => matches!(protocol, Protocol::Anthropic),
            Self::Nvidia => matches!(protocol, Protocol::OpenAiChat),
            Self::Gemini => matches!(protocol, Protocol::Gemini),
            Self::Jina => matches!(protocol, Protocol::JinaRerank),
            Self::Cohere => matches!(protocol, Protocol::CohereRerank),
            Self::Xai => matches!(protocol, Protocol::XaiVideo),
            Self::Bedrock | Self::Vertex | Self::Custom => false,
        }
    }

    /// 返回渠道是否已经接入生产调度运行时。
    #[must_use]
    pub const fn supports_runtime(self, protocol: Protocol) -> bool {
        self.supports_protocol(protocol)
    }

    /// 返回渠道是否支持管理员探活。
    #[must_use]
    pub const fn supports_probe(self, protocol: Protocol) -> bool {
        self.supports_protocol(protocol) && !matches!(self, Self::Xai)
    }

    /// 返回渠道是否支持上游模型发现。
    #[must_use]
    pub const fn supports_model_discovery(self, protocol: Protocol) -> bool {
        self.supports_protocol(protocol)
            && matches!(
                self,
                Self::OpenAi | Self::OpenCode | Self::Anthropic | Self::Nvidia | Self::Gemini
            )
    }

    /// 返回凭据种类是否可以挂载到该渠道。
    ///
    /// Codex OAuth 仍需由 provider、protocol 和 OAuth provider 的联合规则校验；
    /// 此方法只处理渠道本身的通用边界。
    #[must_use]
    pub const fn supports_credential_kind(self, kind: CredentialKind) -> bool {
        match self {
            Self::OpenCode => matches!(kind, CredentialKind::Public | CredentialKind::ApiKey),
            Self::Nvidia | Self::Jina | Self::Cohere | Self::Xai => {
                matches!(kind, CredentialKind::ApiKey)
            }
            _ => !matches!(kind, CredentialKind::Public),
        }
    }

    /// 管理表单允许新建的凭据种类。历史记录继续按兼容规则读取，避免收紧旧数据。
    #[must_use]
    pub const fn creatable_credential_kinds(self) -> &'static [CredentialKind] {
        match self {
            Self::OpenCode => &[CredentialKind::ApiKey, CredentialKind::Public],
            Self::Nvidia | Self::Jina | Self::Cohere | Self::Xai => &[CredentialKind::ApiKey],
            Self::Bedrock => &[CredentialKind::Bedrock],
            Self::Vertex => &[CredentialKind::Oauth, CredentialKind::ServiceAccount],
            Self::OpenAi | Self::Anthropic | Self::Gemini | Self::Custom => {
                &[CredentialKind::ApiKey, CredentialKind::Oauth]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_capabilities_are_closed() {
        assert!(ChannelType::Nvidia.supports_protocol(Protocol::OpenAiChat));
        assert!(!ChannelType::Nvidia.supports_protocol(Protocol::OpenAiResponses));
        assert!(ChannelType::OpenCode.supports_protocol(Protocol::Anthropic));
        assert!(ChannelType::Xai.supports_runtime(Protocol::XaiVideo));
        assert!(!ChannelType::Xai.supports_probe(Protocol::XaiVideo));
        assert!(!ChannelType::Jina.supports_model_discovery(Protocol::JinaRerank));
    }

    #[test]
    fn credential_capabilities_keep_public_open_code_boundary() {
        assert!(ChannelType::OpenCode.supports_credential_kind(CredentialKind::Public));
        assert!(!ChannelType::OpenAi.supports_credential_kind(CredentialKind::Public));
        assert!(ChannelType::Nvidia.supports_credential_kind(CredentialKind::ApiKey));
        assert!(!ChannelType::Nvidia.supports_credential_kind(CredentialKind::Oauth));
    }

    #[test]
    fn production_matrix_preserves_supported_and_unassembled_pairs() {
        let expected: &[(ChannelType, &[Protocol])] = &[
            (ChannelType::Nvidia, &[Protocol::OpenAiChat]),
            (
                ChannelType::OpenAi,
                &[
                    Protocol::OpenAiChat,
                    Protocol::OpenAiResponses,
                    Protocol::OpenAiEmbeddings,
                    Protocol::OpenAiImages,
                    Protocol::OpenAiAudio,
                    Protocol::OpenAiSpeech,
                ],
            ),
            (
                ChannelType::OpenCode,
                &[
                    Protocol::OpenAiChat,
                    Protocol::OpenAiResponses,
                    Protocol::Anthropic,
                ],
            ),
            (ChannelType::Anthropic, &[Protocol::Anthropic]),
            (ChannelType::Gemini, &[Protocol::Gemini]),
            (ChannelType::Jina, &[Protocol::JinaRerank]),
            (ChannelType::Cohere, &[Protocol::CohereRerank]),
            (ChannelType::Xai, &[Protocol::XaiVideo]),
            (ChannelType::Bedrock, &[]),
            (ChannelType::Vertex, &[]),
            (ChannelType::Custom, &[]),
        ];
        assert_eq!(expected.len(), ChannelType::ALL.len());
        for &(channel, supported) in expected {
            for &protocol in Protocol::ALL {
                let runtime = supported.contains(&protocol);
                assert_eq!(
                    channel.supports_protocol(protocol),
                    runtime,
                    "{channel:?}/{protocol:?}"
                );
                assert_eq!(channel.supports_runtime(protocol), runtime);
                assert_eq!(
                    channel.supports_probe(protocol),
                    runtime && channel != ChannelType::Xai
                );
                let discovery = runtime
                    && !matches!(
                        channel,
                        ChannelType::Xai | ChannelType::Jina | ChannelType::Cohere
                    );
                assert_eq!(channel.supports_model_discovery(protocol), discovery);
            }
        }
    }

    #[test]
    fn new_credentials_remain_a_subset_of_legacy_compatibility() {
        for &channel in ChannelType::ALL {
            assert!(!channel.creatable_credential_kinds().is_empty());
            for &kind in channel.creatable_credential_kinds() {
                assert!(
                    channel.supports_credential_kind(kind),
                    "{channel:?}/{kind:?}"
                );
            }
        }
        // Read compatibility must not expand the choices offered for new credentials.
        assert!(ChannelType::Anthropic.supports_credential_kind(CredentialKind::SetupToken));
        assert!(
            !ChannelType::Anthropic
                .creatable_credential_kinds()
                .contains(&CredentialKind::SetupToken)
        );
        assert_eq!(
            ChannelType::Bedrock.creatable_credential_kinds(),
            &[CredentialKind::Bedrock]
        );
        assert_eq!(
            ChannelType::Vertex.creatable_credential_kinds(),
            &[CredentialKind::Oauth, CredentialKind::ServiceAccount]
        );
    }
}
