use af_domain::{TaskFailure, TaskFailureKind, TaskProgress, TaskState, TaskStatus};

use super::types::AsyncTaskRepositoryError;

pub(super) fn allowed_transition(current: &TaskStatus, target: &TaskStatus) -> bool {
    let current_state = current.state();
    let target_state = target.state();
    if current_state == target_state {
        return matches!(
            current_state,
            TaskState::Submitted | TaskState::Queued | TaskState::Running
        ) && target.progress() >= current.progress();
    }
    let state_advances = matches!(
        (current_state, target_state),
        (
            TaskState::Submitted,
            TaskState::Queued | TaskState::Running | TaskState::Succeeded | TaskState::Failed
        ) | (
            TaskState::Queued,
            TaskState::Running | TaskState::Succeeded | TaskState::Failed
        ) | (TaskState::Running, TaskState::Succeeded | TaskState::Failed)
    );
    state_advances && (target_state.is_terminal() || target.progress() >= current.progress())
}

pub(super) fn same_persisted_status(left: &TaskStatus, right: &TaskStatus) -> bool {
    status_to_persistence(left) == status_to_persistence(right)
}

pub(super) fn status_to_persistence(status: &TaskStatus) -> (i16, i16, Option<i16>) {
    (
        state_code(status.state()),
        i16::try_from(status.progress().basis_points()).expect("任务进度上界必须适配 i16"),
        status
            .failure()
            .map(|failure| failure_kind_code(failure.kind())),
    )
}

pub(super) fn status_from_persistence(
    status: i16,
    progress_basis_points: i16,
    failure_kind: Option<i16>,
) -> Result<TaskStatus, AsyncTaskRepositoryError> {
    let state = state_from_code(status)?;
    let progress_basis_points =
        u16::try_from(progress_basis_points).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let progress = TaskProgress::new(progress_basis_points)
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    match (state, failure_kind) {
        (TaskState::Submitted, None) => Ok(TaskStatus::Submitted { progress }),
        (TaskState::Queued, None) => Ok(TaskStatus::Queued { progress }),
        (TaskState::Running, None) => Ok(TaskStatus::Running { progress }),
        (TaskState::Succeeded, None) if progress == TaskProgress::COMPLETE => {
            Ok(TaskStatus::Succeeded)
        }
        (TaskState::Failed, Some(kind)) if progress == TaskProgress::ZERO => {
            Ok(TaskStatus::Failed {
                failure: TaskFailure::without_reason(failure_kind_from_code(kind)?),
            })
        }
        _ => Err(AsyncTaskRepositoryError::Invariant),
    }
}

pub(super) const fn state_code(state: TaskState) -> i16 {
    match state {
        TaskState::Submitted => 1,
        TaskState::Queued => 2,
        TaskState::Running => 3,
        TaskState::Succeeded => 4,
        TaskState::Failed => 5,
    }
}

const fn state_from_code(code: i16) -> Result<TaskState, AsyncTaskRepositoryError> {
    match code {
        1 => Ok(TaskState::Submitted),
        2 => Ok(TaskState::Queued),
        3 => Ok(TaskState::Running),
        4 => Ok(TaskState::Succeeded),
        5 => Ok(TaskState::Failed),
        _ => Err(AsyncTaskRepositoryError::Invariant),
    }
}

const fn failure_kind_code(kind: TaskFailureKind) -> i16 {
    match kind {
        TaskFailureKind::Rejected => 1,
        TaskFailureKind::Cancelled => 2,
        TaskFailureKind::TimedOut => 3,
        TaskFailureKind::Upstream => 4,
    }
}

fn failure_kind_from_code(code: i16) -> Result<TaskFailureKind, AsyncTaskRepositoryError> {
    match code {
        1 => Ok(TaskFailureKind::Rejected),
        2 => Ok(TaskFailureKind::Cancelled),
        3 => Ok(TaskFailureKind::TimedOut),
        4 => Ok(TaskFailureKind::Upstream),
        _ => Err(AsyncTaskRepositoryError::Invariant),
    }
}
