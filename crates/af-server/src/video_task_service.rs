use std::{fmt, sync::Arc};

use af_db::AsyncTaskRepositoryError;
use af_domain::{AfError, AsyncTaskId, AsyncTaskRequestId, GatewayPrincipal, UpstreamError};
use af_http::{VideoTaskService, VideoTaskServiceFuture, VideoTaskSnapshot};
use af_protocol::CanonicalVideoGenerationRequest;

use crate::scheduled_chat::{
    PersistentVideoTaskCoordinator, PersistentVideoTaskError, PersistentVideoTaskPollOutcome,
};

/// 使用持久化协调器实现的生产视频任务公开服务。
pub(crate) struct DatabaseVideoTaskService {
    coordinator: Arc<PersistentVideoTaskCoordinator>,
}

impl DatabaseVideoTaskService {
    /// 绑定单一协调器，确保提交、轮询与结果重取共享同一持久化边界。
    #[must_use]
    pub(crate) fn new(coordinator: PersistentVideoTaskCoordinator) -> Self {
        Self {
            coordinator: Arc::new(coordinator),
        }
    }
}

impl VideoTaskService for DatabaseVideoTaskService {
    fn submit<'a>(
        &'a self,
        principal: GatewayPrincipal,
        request_id: AsyncTaskRequestId,
        request: CanonicalVideoGenerationRequest,
        relay_request_id: &'a str,
    ) -> VideoTaskServiceFuture<'a> {
        Box::pin(async move {
            let task_id = random_task_id()?;
            let task = self
                .coordinator
                .submit(task_id, request_id, principal, request, relay_request_id)
                .await
                .map_err(map_submit_error)?;
            Ok(VideoTaskSnapshot::new(
                task.task_id(),
                task.status().clone(),
                None,
            ))
        })
    }

    fn poll<'a>(
        &'a self,
        principal: GatewayPrincipal,
        task_id: AsyncTaskId,
        relay_request_id: &'a str,
    ) -> VideoTaskServiceFuture<'a> {
        Box::pin(async move {
            let outcome = self
                .coordinator
                .poll(principal.user_id(), task_id, relay_request_id)
                .await
                .map_err(map_poll_error)?;
            match outcome {
                PersistentVideoTaskPollOutcome::Updated { task, poll, .. } => {
                    Ok(VideoTaskSnapshot::new(
                        task.task_id(),
                        task.status().clone(),
                        poll.output().cloned(),
                    ))
                }
                PersistentVideoTaskPollOutcome::Terminal(task) => {
                    let output = if matches!(task.status(), af_domain::TaskStatus::Succeeded) {
                        self.coordinator
                            .fetch_succeeded_output(
                                principal.user_id(),
                                task.task_id(),
                                relay_request_id,
                            )
                            .await
                            .map_err(map_poll_error)?
                            .output()
                            .cloned()
                    } else {
                        None
                    };
                    Ok(VideoTaskSnapshot::new(
                        task.task_id(),
                        task.status().clone(),
                        output,
                    ))
                }
            }
        })
    }
}

impl fmt::Debug for DatabaseVideoTaskService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseVideoTaskService(<脱敏>)")
    }
}

fn random_task_id() -> Result<AsyncTaskId, AfError> {
    for _ in 0..2 {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| AfError::Internal)?;
        if let Ok(task_id) = AsyncTaskId::new(bytes) {
            return Ok(task_id);
        }
    }
    Err(AfError::Internal)
}

fn map_submit_error(error: PersistentVideoTaskError) -> AfError {
    match error {
        PersistentVideoTaskError::Input => AfError::InvalidRequest,
        PersistentVideoTaskError::Repository(AsyncTaskRepositoryError::Conflict) => {
            AfError::IdempotencyConflict
        }
        PersistentVideoTaskError::Repository(AsyncTaskRepositoryError::OutcomeUnknown)
        | PersistentVideoTaskError::SubmissionOutcomeUnknown => AfError::RequestOutcomeUnknown,
        PersistentVideoTaskError::Runtime(error) => error,
        PersistentVideoTaskError::Repository(
            AsyncTaskRepositoryError::Query
            | AsyncTaskRepositoryError::Timeout
            | AsyncTaskRepositoryError::Invariant,
        )
        | PersistentVideoTaskError::NotFound
        | PersistentVideoTaskError::Entropy
        | PersistentVideoTaskError::Invariant
        | PersistentVideoTaskError::ResultUnavailable
        | PersistentVideoTaskError::Billing => AfError::Internal,
    }
}

fn map_poll_error(error: PersistentVideoTaskError) -> AfError {
    match error {
        PersistentVideoTaskError::NotFound => AfError::TaskNotFound,
        PersistentVideoTaskError::SubmissionOutcomeUnknown
        | PersistentVideoTaskError::Repository(
            AsyncTaskRepositoryError::Conflict | AsyncTaskRepositoryError::OutcomeUnknown,
        ) => AfError::RequestOutcomeUnknown,
        PersistentVideoTaskError::ResultUnavailable => AfError::from(UpstreamError::ProtocolError),
        PersistentVideoTaskError::Runtime(error) => error,
        PersistentVideoTaskError::Input
        | PersistentVideoTaskError::Repository(
            AsyncTaskRepositoryError::Query
            | AsyncTaskRepositoryError::Timeout
            | AsyncTaskRepositoryError::Invariant,
        )
        | PersistentVideoTaskError::Entropy
        | PersistentVideoTaskError::Invariant
        | PersistentVideoTaskError::Billing => AfError::Internal,
    }
}
