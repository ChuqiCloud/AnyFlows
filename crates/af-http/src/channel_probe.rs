use std::{future::Future, pin::Pin};

use af_domain::ChannelId;

/// 管理端单次渠道测活的脱敏结论。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminChannelProbeOutcome {
    /// 上游返回成功状态且响应结构合法。
    Healthy,
    /// 目标、凭据、传输、状态码或响应结构校验失败。
    Unhealthy,
    /// 单次真实探活超过服务端硬期限。
    TimedOut,
}

/// 管理端渠道测活端口返回的异步结果。
pub type AdminChannelProbeFuture<'a> =
    Pin<Box<dyn Future<Output = AdminChannelProbeOutcome> + Send + 'a>>;

/// 管理渠道测活端口；实现必须自行施加硬超时且不得返回敏感上游内容。
pub trait AdminChannelProbe: Send + Sync {
    /// 对指定渠道执行一次真实探活，并只返回脱敏结论。
    fn probe<'a>(&'a self, channel_id: ChannelId) -> AdminChannelProbeFuture<'a>;
}
