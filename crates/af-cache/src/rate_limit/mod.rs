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

        decode_admit_response(rules, status, index, retry_after_millis)
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

/// 将 Redis Lua 返回值收敛为受控结果；任何越界或未知形状均失败关闭。
fn decode_admit_response(
    rules: &[RequestRateLimitRule],
    status: i64,
    index: i64,
    retry_after_millis: i64,
) -> Result<RequestRateLimitOutcome, CacheError> {
    match status {
        ADMIT_OK if index == 0 && retry_after_millis == 0 => Ok(RequestRateLimitOutcome::Admitted),
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

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use af_domain::{GroupId, TokenId, UserId};

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
        let same_subject_and_window =
            RequestRateLimitRule::new(rule.subject(), NonZeroU32::new(2).unwrap(), rule.window())
                .unwrap();
        assert_eq!(
            validate_rules(&[rule, same_subject_and_window]).unwrap_err(),
            CacheError::InvalidRateLimitBatch
        );
        assert_eq!(
            validate_rules(&[rule; MAX_REQUEST_RATE_LIMIT_RULES + 1]).unwrap_err(),
            CacheError::InvalidRateLimitBatch
        );
        assert!(validate_rules(&[]).is_ok());
    }

    #[test]
    fn batch_keeps_subject_types_and_windows_independent() {
        let limit = NonZeroU32::new(2).unwrap();
        let user = RequestRateLimitSubject::User(UserId::new(7).unwrap());
        let group = RequestRateLimitSubject::Group(GroupId::new(7).unwrap());
        let token = RequestRateLimitSubject::Token(TokenId::new(7).unwrap());
        let rules = [
            RequestRateLimitRule::new(user, limit, Duration::from_secs(1)).unwrap(),
            RequestRateLimitRule::new(user, limit, Duration::from_secs(2)).unwrap(),
            RequestRateLimitRule::new(group, limit, Duration::from_secs(1)).unwrap(),
            RequestRateLimitRule::new(token, limit, Duration::from_secs(1)).unwrap(),
        ];
        assert!(validate_rules(&rules).is_ok());
    }

    #[test]
    fn empty_batch_response_is_admitted() {
        assert!(validate_rules(&[]).is_ok());
        assert_eq!(
            decode_admit_response(&[], ADMIT_OK, 0, 0).unwrap(),
            RequestRateLimitOutcome::Admitted
        );
    }

    #[test]
    fn malformed_lua_responses_fail_closed_as_protocol_errors() {
        let rule = RequestRateLimitRule::new(
            RequestRateLimitSubject::User(UserId::new(8).unwrap()),
            NonZeroU32::new(1).unwrap(),
            Duration::from_secs(5),
        )
        .unwrap();
        let cases = [
            (ADMIT_OK, 1, 0),
            (ADMIT_LIMITED, -1, 1),
            (ADMIT_LIMITED, 0, 1),
            (ADMIT_LIMITED, 2, 1),
            (ADMIT_LIMITED, 1, 0),
            (ADMIT_LIMITED, 1, 6_000),
            (ADMIT_INCONSISTENT, 1, 1),
            (99, 0, 0),
        ];
        for (status, index, retry_after_millis) in cases {
            assert_eq!(
                decode_admit_response(&[rule], status, index, retry_after_millis).unwrap_err(),
                protocol_error()
            );
        }
    }

    #[test]
    fn limited_response_preserves_subject_and_retry_boundary() {
        let rule = RequestRateLimitRule::new(
            RequestRateLimitSubject::Token(TokenId::new(9).unwrap()),
            NonZeroU32::new(1).unwrap(),
            Duration::from_millis(1_500),
        )
        .unwrap();
        assert_eq!(
            decode_admit_response(&[rule], ADMIT_LIMITED, 1, 1_500).unwrap(),
            RequestRateLimitOutcome::Limited(RequestRateLimitRejection::new(
                rule.subject(),
                Duration::from_millis(1_500),
            ))
        );
    }
}
