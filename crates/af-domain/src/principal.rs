use std::fmt;

use thiserror::Error;

/// 网关主体或渠道标识构造错误；不保留外部提供的原始数值。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PrincipalIdError {
    /// 数据库实体标识必须是正整数。
    #[error("领域实体标识必须大于零")]
    NonPositive,
}

macro_rules! principal_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(i64);

        impl $name {
            /// 校验并构造正整数标识。
            pub const fn new(value: i64) -> Result<Self, PrincipalIdError> {
                if value > 0 {
                    Ok(Self(value))
                } else {
                    Err(PrincipalIdError::NonPositive)
                }
            }

            /// 返回持久化边界使用的正整数值。
            #[must_use]
            pub const fn get(self) -> i64 {
                self.0
            }
        }

        impl TryFrom<i64> for $name {
            type Error = PrincipalIdError;

            fn try_from(value: i64) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!(stringify!($name), "(<redacted>)"))
            }
        }
    };
}

principal_id! {
    /// 已通过鉴权的下游令牌标识。
    TokenId
}

principal_id! {
    /// 已通过鉴权的用户标识。
    UserId
}

principal_id! {
    /// 本次请求使用的有效分组标识。
    GroupId
}

principal_id! {
    /// 本次请求实际使用的上游渠道标识。
    ChannelId
}

principal_id! {
    /// 上游渠道绑定的凭据标识。
    CredentialId
}

/// 已完成令牌、用户与有效分组校验的网关请求主体。
///
/// 该值只携带稳定标识，不包含 API Key、摘要、展示前缀或可变授权策略，且不实现
/// `Display` 或序列化，避免被直接写入日志和协议响应。
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct GatewayPrincipal {
    token_id: TokenId,
    user_id: UserId,
    group_id: GroupId,
}

impl GatewayPrincipal {
    /// 使用已经分别校验的强类型标识构造请求主体。
    #[must_use]
    pub const fn new(token_id: TokenId, user_id: UserId, group_id: GroupId) -> Self {
        Self {
            token_id,
            user_id,
            group_id,
        }
    }

    /// 返回通过鉴权的令牌标识。
    #[must_use]
    pub const fn token_id(self) -> TokenId {
        self.token_id
    }

    /// 返回令牌所属用户标识。
    #[must_use]
    pub const fn user_id(self) -> UserId {
        self.user_id
    }

    /// 返回令牌覆盖或用户默认的有效分组标识。
    #[must_use]
    pub const fn group_id(self) -> GroupId {
        self.group_id
    }
}

impl fmt::Debug for GatewayPrincipal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GatewayPrincipal(<redacted>)")
    }
}
