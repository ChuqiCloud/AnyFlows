use af_db::{SchedulerCatalogSubject, SchedulerRuntimeProjection, SchedulerRuntimeRepository};

use super::SchedulerInvalidationRuntimeError;

/// 按闭合主体生成 Redis hash 的稳定非敏感键后缀。
pub(super) fn scheduler_projection_key(subject: SchedulerCatalogSubject) -> String {
    match subject {
        SchedulerCatalogSubject::Channel(channel_id) => {
            format!("channel:{}", channel_id.get())
        }
        SchedulerCatalogSubject::Group(group_id) => format!("group:{}", group_id.get()),
    }
}

/// 按 outbox 版本从数据库当前真相构造单主体投影。
pub(super) async fn load_database_projection(
    repository: &SchedulerRuntimeRepository,
    event_id: i64,
    subject: SchedulerCatalogSubject,
) -> Result<SchedulerRuntimeProjection, SchedulerInvalidationRuntimeError> {
    let version = u64::try_from(event_id)
        .ok()
        .filter(|version| *version > 0)
        .ok_or(SchedulerInvalidationRuntimeError::ProjectionState)?;
    let records = repository.load_subject(subject).await?;
    SchedulerRuntimeProjection::new(version, subject, records).map_err(Into::into)
}

/// 校验 Redis hash 版本、广播最低版本与正文内版本/主体完全一致。
pub(super) fn decode_stored_projection(
    stored_version: u64,
    signaled_version: u64,
    expected_subject: SchedulerCatalogSubject,
    payload: &[u8],
) -> Result<SchedulerRuntimeProjection, SchedulerInvalidationRuntimeError> {
    if stored_version < signaled_version {
        return Err(SchedulerInvalidationRuntimeError::ProjectionState);
    }
    let projection = SchedulerRuntimeProjection::decode(payload)?;
    if projection.version() != stored_version || projection.subject() != expected_subject {
        return Err(SchedulerInvalidationRuntimeError::ProjectionState);
    }
    Ok(projection)
}

#[cfg(test)]
mod tests {
    use af_domain::ChannelId;

    use super::*;

    #[test]
    fn stored_projection_must_match_hash_signal_and_subject() {
        let subject = SchedulerCatalogSubject::Channel(ChannelId::new(7).unwrap());
        let projection = SchedulerRuntimeProjection::new(9, subject, Vec::new()).unwrap();
        let payload = projection.encode().unwrap();

        assert!(decode_stored_projection(9, 8, subject, &payload).is_ok());
        assert!(matches!(
            decode_stored_projection(7, 8, subject, &payload),
            Err(SchedulerInvalidationRuntimeError::ProjectionState)
        ));
        assert!(matches!(
            decode_stored_projection(
                9,
                9,
                SchedulerCatalogSubject::Channel(ChannelId::new(8).unwrap()),
                &payload,
            ),
            Err(SchedulerInvalidationRuntimeError::ProjectionState)
        ));
    }
}
