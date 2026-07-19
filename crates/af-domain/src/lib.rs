//! 无 IO 的领域类型、值对象与错误。

mod enums;
mod error;

pub use enums::{
    ChannelType, CredentialKind, Operation, ParseEnumError, Protocol, PublicErrorCode, Role, Status,
};
pub use error::{AfError, UpstreamError, UpstreamServerStatus};

#[cfg(test)]
mod enums_tests;
#[cfg(test)]
mod error_tests;
