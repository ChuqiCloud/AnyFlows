//! 调度运行时渠道目录的仓储、选择策略和快照值对象。

mod policy;
mod projection;
mod record;
mod repository;
mod selection;
mod source;
mod target;

#[cfg(test)]
mod tests;

pub use policy::{
    ChannelModelMappings, ChannelParameterOverrides, ChannelRequestPolicyError,
    MAX_CHANNEL_MODEL_MAPPINGS, MAX_CHANNEL_OUTPUT_TOKENS, MAX_CHANNEL_PARAMETER_OVERRIDES,
    MAX_CHANNEL_REQUEST_POLICY_BYTES, MAX_CHANNEL_STOP_SEQUENCE_BYTES, MAX_CHANNEL_STOP_SEQUENCES,
    validate_channel_header_overrides,
};
pub use projection::{
    MAX_SCHEDULER_RUNTIME_PROJECTION_BYTES, SchedulerRuntimeProjection,
    SchedulerRuntimeProjectionError,
};
pub use record::SchedulerRuntimeRecord;
pub use repository::{
    MAX_SCHEDULER_RUNTIME_CREDENTIAL_ENTRIES, SchedulerRuntimeRepository,
    SchedulerRuntimeRepositoryError,
};
pub use target::{
    MAX_SCHEDULER_RUNTIME_CREDENTIALS_PER_CHANNEL, SchedulerRuntimeCredentialRecord,
    SchedulerRuntimeHeader, SchedulerRuntimeProxyRecord, SchedulerRuntimeTargetRecord,
    SchedulerRuntimeTargetRecordError,
};
