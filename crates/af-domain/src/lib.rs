//! 无 IO 的领域类型、值对象与错误。

mod enums;

pub use enums::{ChannelType, CredentialKind, Operation, ParseEnumError, Protocol, Role, Status};

#[cfg(test)]
mod enums_tests;
