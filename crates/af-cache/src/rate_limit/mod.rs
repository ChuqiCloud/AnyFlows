mod scripts;
mod types;

use std::{collections::BTreeSet, sync::Arc, time::Duration};

use crate::{
    CacheError, CacheOperation, RedisFailureKind, config::validate_cache_key,
    redis_backend::RedisBackend,
};
use scripts::ADMIT_SCRIPT;

pub use types::{
    MAX_REQUEST_RATE_LIMIT_RULES, RedisRequestRateLimitConfig, RequestRateLimitOutcome,
    RequestRateLimitRejection, RequestRateLimitRule, RequestRateLimitSubject,
};

const ADMIT_OK: i64 = 0;
const ADMIT_LIMITED: i64 = 1;
const ADMIT_INCONSISTENT: i64 = 2;

/// 使用 Redis 服务端时间原子维护多主体固定窗口请求计数。
#[derive(Clone)]
pub struct RedisRequestRateLimitStore {
    backend: Arc<RedisBackend>,
    key_prefix: Arc<str>,
}

impl RedisRequestRateLimitStore {
    /// 建立 Redis 连接并验证请求限流键空间。
    pub async fn connect(config: RedisRequestRateLimitConfig) -> Result<Self, CacheError> {
        config.validate()?;
        let key_prefix = format!("{{{}}}:request-rate-limit", config.namespace);
        validate_cache_key(&key_prefix)?;
        Ok(Self {
            backend: Arc::new(RedisBackend::connect(&config.redis).await?),
            key_prefix: Arc::from(key_prefix),
        })
    }

    /// 原子检查并计入一批固定窗口规则；空规则集不访问 Redis。
    pub async fn admit(
        &self,
        rules: &[RequestRateLimitRule],
    ) -> Result<RequestRateLimitOutcome, CacheError> {
        validate_rules(rules)?;
        if rules.is_empty() {
            return Ok(RequestRateLimitOutcome::Admitted);
        }

        let keys = rules
            .iter()
            .copied()
            .map(|rule| self.rule_key(rule))
            .collect::<Result<Vec<_>, _>>()?;
        let mut command = redis::cmd("EVAL");
        command.arg(ADMIT_SCRIPT).arg(keys.len()).arg(&keys);
        for rule in rules {
            command.arg(rule.limit().get()).arg(rule.window_millis());
        }
        let (status, index, retry_after_millis): (i64, i64, i64) = self
            .backend
            .query(CacheOperation::RateLimitCheck, &mut command)
            .await?;

        match status {
            ADMIT_OK if index == 0 && retry_after_millis == 0 => {
                Ok(RequestRateLimitOutcome::Admitted)
            }
            ADMIT_LIMITED => {
                let index = usize::try_from(index)
                    .ok()
                    .and_then(|index| index.checked_sub(1))
                    .filter(|index| *index < rules.len())
                    .ok_or_else(protocol_error)?;
                let retry_after_millis = u64::try_from(retry_after_millis)
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(protocol_error)?;
                let retry_after = Duration::from_millis(retry_after_millis);
                if retry_after > rules[index].window() {
                    return Err(protocol_error());
                }
                Ok(RequestRateLimitOutcome::Limited(
                    RequestRateLimitRejection::new(rules[index].subject(), retry_after),
                ))
            }
            ADMIT_INCONSISTENT => Err(protocol_error()),
            _ => Err(protocol_error()),
        }
    }

    fn rule_key(&self, rule: RequestRateLimitRule) -> Result<String, CacheError> {
        let subject = rule.subject();
        let key = format!(
            "{}:{}:{}:w{}",
            self.key_prefix,
            subject.kind(),
            subject.identifier(),
            rule.window_millis()
        );
        validate_cache_key(&key)?;
        Ok(key)
    }
}

fn validate_rules(rules: &[RequestRateLimitRule]) -> Result<(), CacheError> {
    if rules.len() > MAX_REQUEST_RATE_LIMIT_RULES {
        return Err(CacheError::InvalidRateLimitBatch);
    }
    let mut unique = BTreeSet::new();
    for rule in rules {
        let (kind, identifier) = rule.subject().duplicate_key();
        if !unique.insert((kind, identifier, rule.window_millis())) {
            return Err(CacheError::InvalidRateLimitBatch);
        }
    }
    Ok(())
}

fn protocol_error() -> CacheError {
    CacheError::Redis {
        operation: CacheOperation::RateLimitCheck,
        kind: RedisFailureKind::Protocol,
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use af_domain::UserId;

    use super::*;

    #[test]
    fn batch_rejects_duplicate_rules_and_excess_capacity() {
        let rule = RequestRateLimitRule::new(
            RequestRateLimitSubject::User(UserId::new(1).unwrap()),
            NonZeroU32::new(1).unwrap(),
            Duration::from_secs(60),
        )
        .unwrap();
        assert_eq!(
            validate_rules(&[rule, rule]).unwrap_err(),
            CacheError::InvalidRateLimitBatch
        );
        assert_eq!(
            validate_rules(&[rule; MAX_REQUEST_RATE_LIMIT_RULES + 1]).unwrap_err(),
            CacheError::InvalidRateLimitBatch
        );
        assert!(validate_rules(&[]).is_ok());
    }
}
