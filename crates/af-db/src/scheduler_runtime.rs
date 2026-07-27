//! 调度运行时渠道目录的仓储、选择策略和快照值对象。

mod record;
mod repository;
mod selection;
mod source;
mod target;

#[cfg(test)]
mod tests;

pub use record::SchedulerRuntimeRecord;
pub use repository::{
    MAX_SCHEDULER_RUNTIME_CREDENTIAL_ENTRIES, SchedulerRuntimeRepository,
    SchedulerRuntimeRepositoryError,
};
pub use target::{
    SchedulerRuntimeCredentialRecord, SchedulerRuntimeHeader, SchedulerRuntimeTargetRecord,
    SchedulerRuntimeTargetRecordError,
};
