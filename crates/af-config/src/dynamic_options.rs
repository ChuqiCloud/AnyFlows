use std::{collections::BTreeMap, fmt};

use async_trait::async_trait;
use thiserror::Error;

use crate::SecretString;

const MAX_OPTION_KEY_CHARS: usize = 255;

/// DB Option 的完整只读快照；缺少的键表示该配置当前不存在。
///
/// Option 仅用于全局、非敏感的运营参数。`SecretString` 只提供日志脱敏，不表示可以
/// 在 Option 中存放凭据、令牌、密码或加密主密钥。
#[derive(Clone, Default, Eq, PartialEq)]
pub struct DynamicOptionSnapshot {
    values: BTreeMap<String, SecretString>,
}

impl DynamicOptionSnapshot {
    /// 从一次一致性读取的全部 Option 构造快照，并校验存储键的结构约束。
    ///
    /// 此处只校验跨存储层都成立的结构条件；业务命名空间和允许的 Option 名单由上层
    /// 注册表负责，不能在基础配置 crate 中擅自扩展。
    pub fn try_from_values(
        values: BTreeMap<String, SecretString>,
    ) -> Result<Self, DynamicOptionError> {
        if values.keys().any(|key| !valid_option_key(key)) {
            return Err(DynamicOptionError::InvalidKey);
        }
        Ok(Self { values })
    }

    /// 按键读取受保护的配置值。
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&SecretString> {
        self.values.get(key)
    }

    /// 按键的稳定顺序遍历快照。
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &SecretString)> {
        self.values.iter().map(|(key, value)| (key.as_str(), value))
    }

    /// 返回快照中的配置数量。
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// 判断快照是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl fmt::Debug for DynamicOptionSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DynamicOptionSnapshot")
            .field("len", &self.values.len())
            .finish_non_exhaustive()
    }
}

/// 动态 Option 订阅信号；信号只表示快照失效，不携带配置值。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicOptionSignal {
    /// 已有 Option 被创建、修改或删除；多个连续通知允许合并。
    Invalidated,
    /// 订阅发生滞后或恢复连接，调用方必须重新读取完整快照。
    ResyncRequired,
}

/// 动态 Option 数据源错误；错误不保留可能泄露配置值的底层文本。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum DynamicOptionError {
    /// 无法从持久化来源读取完整快照。
    #[error("读取动态配置快照失败")]
    LoadFailed,

    /// 无法建立动态配置失效订阅。
    #[error("建立动态配置订阅失败")]
    SubscribeFailed,

    /// 已建立的订阅无法继续接收信号。
    #[error("接收动态配置通知失败")]
    ReceiveFailed,

    /// 订阅已永久关闭。
    #[error("动态配置订阅已关闭")]
    SubscriptionClosed,

    /// 持久化来源返回了不符合运行时结构约束的键。
    #[error("动态配置键无效")]
    InvalidKey,
}

/// 动态 Option 的失效订阅。
///
/// `recv` 必须可安全取消：取消尚未完成的 Future 不得消费待处理信号。发生错误后，
/// 调用方必须丢弃当前订阅，重新订阅并重新读取完整快照。丢弃订阅即表示取消，
/// 实现不得在 `Drop` 中执行异步 IO。该订阅是长生命周期资源，永久关闭不是正常结束，
/// 必须返回 `SubscriptionClosed` 并进入上述恢复流程。
#[async_trait]
pub trait DynamicOptionSubscription: Send + 'static {
    /// 等待下一次失效或强制重同步信号。
    async fn recv(&mut self) -> Result<DynamicOptionSignal, DynamicOptionError>;
}

/// 动态 Option 的快照与失效通知来源。
///
/// DB 是配置的唯一持久真相，通知通道只用于使内存投影失效。启动和重订阅时必须先
/// `subscribe`，再 `load_snapshot`，最后开始接收信号，避免在加载与订阅之间漏掉更新。
/// 首次加载失败必须拒绝服务启动；运行期重载失败应保留旧投影并持续重试，不能等待
/// 下一次通知后再恢复。
///
/// 失效通知不承诺可靠投递。调用方必须用单一串行循环完成加载与快照替换，避免较旧的
/// 并发加载覆盖新结果；即使没有收到信号，也必须周期性读取完整快照作为一致性校正。
#[async_trait]
pub trait DynamicOptionSource: Send + Sync + 'static {
    /// 从持久化来源读取一次完整、一致的 Option 快照。
    async fn load_snapshot(&self) -> Result<DynamicOptionSnapshot, DynamicOptionError>;

    /// 建立只承载失效信号的订阅。
    ///
    /// 返回成功时，底层订阅必须已经完成服务端注册并能接收之后的通知，不得返回惰性
    /// 连接；否则调用方无法保证“先订阅、后加载”的启动顺序。
    async fn subscribe(&self) -> Result<Box<dyn DynamicOptionSubscription>, DynamicOptionError>;
}

fn valid_option_key(key: &str) -> bool {
    !key.trim().is_empty()
        && key.chars().count() <= MAX_OPTION_KEY_CHARS
        && !key.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, VecDeque},
        future::Future,
        pin::pin,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        task::{Context, Poll, Waker},
    };

    use super::*;

    fn ready<F: Future>(future: F) -> F::Output {
        let mut context = Context::from_waker(Waker::noop());
        let mut future = pin!(future);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("测试 Future 应立即完成"),
        }
    }

    struct FakeSource {
        snapshot: DynamicOptionSnapshot,
        signals: VecDeque<Result<DynamicOptionSignal, DynamicOptionError>>,
        calls: Arc<Mutex<Vec<&'static str>>>,
        subscription_dropped: Arc<AtomicBool>,
    }

    #[async_trait]
    impl DynamicOptionSource for FakeSource {
        async fn load_snapshot(&self) -> Result<DynamicOptionSnapshot, DynamicOptionError> {
            self.calls.lock().unwrap().push("load_snapshot");
            Ok(self.snapshot.clone())
        }

        async fn subscribe(
            &self,
        ) -> Result<Box<dyn DynamicOptionSubscription>, DynamicOptionError> {
            self.calls.lock().unwrap().push("subscribe");
            Ok(Box::new(FakeSubscription {
                signals: self.signals.clone(),
                dropped: Arc::clone(&self.subscription_dropped),
            }))
        }
    }

    struct FakeSubscription {
        signals: VecDeque<Result<DynamicOptionSignal, DynamicOptionError>>,
        dropped: Arc<AtomicBool>,
    }

    impl Drop for FakeSubscription {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::Release);
        }
    }

    #[async_trait]
    impl DynamicOptionSubscription for FakeSubscription {
        async fn recv(&mut self) -> Result<DynamicOptionSignal, DynamicOptionError> {
            self.signals
                .pop_front()
                .unwrap_or(Err(DynamicOptionError::SubscriptionClosed))
        }
    }

    fn snapshot(entries: &[(&str, &str)]) -> DynamicOptionSnapshot {
        DynamicOptionSnapshot::try_from_values(
            entries
                .iter()
                .map(|(key, value)| ((*key).to_owned(), SecretString::new(*value)))
                .collect(),
        )
        .expect("测试 Option 键必须有效")
    }

    #[test]
    fn snapshot_is_complete_ordered_and_expresses_deletion_by_absence() {
        let initial = snapshot(&[("beta", "second"), ("alpha", "first")]);
        assert_eq!(initial.len(), 2);
        assert_eq!(initial.get("alpha").unwrap().expose(), "first");
        assert_eq!(
            initial.iter().map(|(key, _)| key).collect::<Vec<_>>(),
            ["alpha", "beta"]
        );

        let reloaded = snapshot(&[("beta", "updated")]);
        assert!(reloaded.get("alpha").is_none());
        assert_eq!(reloaded.get("beta").unwrap().expose(), "updated");
    }

    #[test]
    fn snapshot_debug_never_contains_keys_or_values() {
        let snapshot = snapshot(&[("private-option-name", "private-option-value")]);
        let rendered = format!("{snapshot:?}");

        assert!(rendered.contains("len: 1"));
        assert!(!rendered.contains("private-option-name"));
        assert!(!rendered.contains("private-option-value"));
    }

    #[test]
    fn trait_objects_preserve_bootstrap_order_signals_and_subscription_drop() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let subscription_dropped = Arc::new(AtomicBool::new(false));
        let source: Arc<dyn DynamicOptionSource> = Arc::new(FakeSource {
            snapshot: snapshot(&[("feature.enabled", "true")]),
            signals: VecDeque::from([
                Ok(DynamicOptionSignal::Invalidated),
                Ok(DynamicOptionSignal::ResyncRequired),
                Err(DynamicOptionError::SubscriptionClosed),
            ]),
            calls: Arc::clone(&calls),
            subscription_dropped: Arc::clone(&subscription_dropped),
        });

        let mut subscription: Box<dyn DynamicOptionSubscription> =
            ready(source.subscribe()).unwrap();
        let loaded = ready(source.load_snapshot()).unwrap();

        assert_eq!(
            calls.lock().unwrap().as_slice(),
            ["subscribe", "load_snapshot"]
        );
        assert_eq!(loaded.get("feature.enabled").unwrap().expose(), "true");
        assert_eq!(
            ready(subscription.recv()).unwrap(),
            DynamicOptionSignal::Invalidated
        );
        assert_eq!(
            ready(subscription.recv()).unwrap(),
            DynamicOptionSignal::ResyncRequired
        );
        assert_eq!(
            ready(subscription.recv()),
            Err(DynamicOptionError::SubscriptionClosed)
        );

        drop(subscription);
        assert!(subscription_dropped.load(Ordering::Acquire));
    }

    #[test]
    fn errors_are_stable_and_do_not_accept_backend_details() {
        let cases = [
            (DynamicOptionError::LoadFailed, "读取动态配置快照失败"),
            (DynamicOptionError::SubscribeFailed, "建立动态配置订阅失败"),
            (DynamicOptionError::ReceiveFailed, "接收动态配置通知失败"),
            (DynamicOptionError::SubscriptionClosed, "动态配置订阅已关闭"),
            (DynamicOptionError::InvalidKey, "动态配置键无效"),
        ];

        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn empty_snapshot_uses_the_same_read_only_contract() {
        let snapshot = DynamicOptionSnapshot::default();
        assert!(snapshot.is_empty());
        assert_eq!(snapshot.iter().len(), 0);
    }

    #[test]
    fn snapshot_rejects_invalid_keys_without_echoing_them() {
        let invalid_keys = [
            String::new(),
            "private-key-name\nline".to_owned(),
            "x".repeat(MAX_OPTION_KEY_CHARS + 1),
        ];

        for key in invalid_keys {
            let values = BTreeMap::from([(key.clone(), SecretString::new("private-value"))]);
            let error = DynamicOptionSnapshot::try_from_values(values).unwrap_err();
            let rendered = format!("{error:?}\n{error}");

            assert_eq!(error, DynamicOptionError::InvalidKey);
            assert!(!rendered.contains("private-value"));
            if !key.is_empty() {
                assert!(!rendered.contains(&key));
            }
        }
    }
}
