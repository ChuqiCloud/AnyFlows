use af_db::SchedulerCatalogSubject;
use af_domain::{ChannelId, GroupId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const WIRE_VERSION: u8 = 1;

/// 调度失效 wire 错误；不保留原始消息或 Serde 诊断。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub(super) enum SchedulerInvalidationWireError {
    /// 内部闭合消息无法序列化。
    #[error("编码调度失效消息失败")]
    Encode,
    /// 消息不是当前支持的闭合 JSON 结构。
    #[error("解析调度失效消息失败")]
    Decode,
    /// 消息版本、事件标识或主体标识无效。
    #[error("调度失效消息字段无效")]
    Invariant,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SchedulerInvalidationWire {
    version: u8,
    event_id: u64,
    subject: SchedulerInvalidationSubjectWire,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SchedulerInvalidationSubjectWire {
    Channel { id: i64 },
    Group { id: i64 },
}

/// 已验证且不含投影正文的调度失效信号。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SchedulerInvalidationSignal {
    event_id: u64,
    subject: SchedulerCatalogSubject,
}

impl SchedulerInvalidationSignal {
    /// 返回广播对应的 outbox 事件版本。
    pub(super) const fn event_id(self) -> u64 {
        self.event_id
    }

    /// 返回需要读取物化投影的闭合主体。
    pub(super) const fn subject(self) -> SchedulerCatalogSubject {
        self.subject
    }
}

/// 把闭合主体编码为不含模型、URL、Header 或凭据的版本化消息。
pub(super) fn encode_scheduler_invalidation(
    event_id: u64,
    subject: SchedulerCatalogSubject,
) -> Result<Vec<u8>, SchedulerInvalidationWireError> {
    if event_id == 0 || event_id > i64::MAX as u64 {
        return Err(SchedulerInvalidationWireError::Invariant);
    }
    let subject = match subject {
        SchedulerCatalogSubject::Channel(channel_id) => SchedulerInvalidationSubjectWire::Channel {
            id: channel_id.get(),
        },
        SchedulerCatalogSubject::Group(group_id) => {
            SchedulerInvalidationSubjectWire::Group { id: group_id.get() }
        }
    };
    serde_json::to_vec(&SchedulerInvalidationWire {
        version: WIRE_VERSION,
        event_id,
        subject,
    })
    .map_err(|_| SchedulerInvalidationWireError::Encode)
}

/// 解析并验证来自共享 Redis 通道的闭合消息。
pub(super) fn decode_scheduler_invalidation(
    payload: &[u8],
) -> Result<SchedulerInvalidationSignal, SchedulerInvalidationWireError> {
    let wire: SchedulerInvalidationWire =
        serde_json::from_slice(payload).map_err(|_| SchedulerInvalidationWireError::Decode)?;
    if wire.version != WIRE_VERSION || wire.event_id == 0 || wire.event_id > i64::MAX as u64 {
        return Err(SchedulerInvalidationWireError::Invariant);
    }
    let subject = match wire.subject {
        SchedulerInvalidationSubjectWire::Channel { id } => ChannelId::new(id)
            .map(SchedulerCatalogSubject::Channel)
            .map_err(|_| SchedulerInvalidationWireError::Invariant)?,
        SchedulerInvalidationSubjectWire::Group { id } => GroupId::new(id)
            .map(SchedulerCatalogSubject::Group)
            .map_err(|_| SchedulerInvalidationWireError::Invariant)?,
    };
    Ok(SchedulerInvalidationSignal {
        event_id: wire.event_id,
        subject,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versioned_wire_round_trips_closed_subjects() {
        for (event_id, subject) in [
            (
                7,
                SchedulerCatalogSubject::Channel(ChannelId::new(11).unwrap()),
            ),
            (8, SchedulerCatalogSubject::Group(GroupId::new(12).unwrap())),
        ] {
            let payload = encode_scheduler_invalidation(event_id, subject).unwrap();
            let signal = decode_scheduler_invalidation(&payload).unwrap();
            assert_eq!(signal.event_id(), event_id);
            assert_eq!(signal.subject(), subject);
            let rendered = String::from_utf8(payload).unwrap();
            assert!(!rendered.contains("model"));
            assert!(!rendered.contains("url"));
            assert!(!rendered.contains("credential"));
        }
    }

    #[test]
    fn wire_rejects_unknown_fields_versions_and_invalid_ids() {
        for payload in [
            br#"{"version":2,"event_id":1,"subject":{"kind":"channel","id":1}}"#.as_slice(),
            br#"{"version":1,"event_id":0,"subject":{"kind":"channel","id":1}}"#.as_slice(),
            br#"{"version":1,"event_id":1,"subject":{"kind":"group","id":0}}"#.as_slice(),
            br#"{"version":1,"event_id":1,"subject":{"kind":"channel","id":1},"payload":"forbidden"}"#.as_slice(),
        ] {
            assert!(decode_scheduler_invalidation(payload).is_err());
        }
    }
}
