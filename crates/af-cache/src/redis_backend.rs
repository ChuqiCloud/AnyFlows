use std::time::Duration;

use async_trait::async_trait;
use redis::{
    Cmd, ErrorKind, FromRedisValue,
    aio::{ConnectionManager, ConnectionManagerConfig},
};
use tokio::time;

use crate::{
    CacheError, CacheOperation, RedisConfig, RedisFailureKind,
    config::validate_ttl,
    remote::{RemoteCache, RemoteEntry},
};

const READ_WITH_TTL_SCRIPT: &str = r#"
local value_length = redis.call('STRLEN', KEYS[1])
if value_length > tonumber(ARGV[1]) then
    return {1, '', 0}
end
local value = redis.call('GET', KEYS[1])
if not value then
    return {0, '', 0}
end
return {2, value, redis.call('PTTL', KEYS[1])}
"#;

const READ_STATUS_MISS: i64 = 0;
const READ_STATUS_TOO_LARGE: i64 = 1;
const READ_STATUS_HIT: i64 = 2;

/// 使用可重连复用连接执行 Redis 缓存命令。
pub(crate) struct RedisBackend {
    connection: ConnectionManager,
    operation_timeout: Duration,
}

impl RedisBackend {
    pub(crate) async fn connect(config: &RedisConfig) -> Result<Self, CacheError> {
        let client = redis::Client::open(config.url()).map_err(|_| CacheError::InvalidRedisUrl)?;
        // 单次命令由外层统一计时，避免驱动超时先返回 IO 错误而丢失超时分类。
        let manager_config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(config.connect_timeout()))
            .set_response_timeout(None);

        let connection = time::timeout(
            config.connect_timeout(),
            client.get_connection_manager_with_config(manager_config),
        )
        .await
        .map_err(|_| CacheError::redis(CacheOperation::Connect, RedisFailureKind::Timeout))?
        .map_err(|error| redis_error(CacheOperation::Connect, &error))?;

        let backend = Self {
            connection,
            operation_timeout: config.operation_timeout(),
        };
        backend.health_check().await?;
        Ok(backend)
    }

    pub(crate) async fn query<T>(
        &self,
        operation: CacheOperation,
        command: &mut Cmd,
    ) -> Result<T, CacheError>
    where
        T: FromRedisValue,
    {
        let mut connection = self.connection.clone();
        time::timeout(self.operation_timeout, command.query_async(&mut connection))
            .await
            .map_err(|_| CacheError::redis(operation, RedisFailureKind::Timeout))?
            .map_err(|error| redis_error(operation, &error))
    }
}

#[async_trait]
impl RemoteCache for RedisBackend {
    async fn get(
        &self,
        key: &str,
        max_value_bytes: usize,
    ) -> Result<Option<RemoteEntry>, CacheError> {
        let max_value_bytes =
            i64::try_from(max_value_bytes).map_err(|_| CacheError::InvalidMaxValueSize)?;
        let (status, value, ttl_millis): (i64, Vec<u8>, i64) = self
            .query(
                CacheOperation::Read,
                redis::cmd("EVAL")
                    .arg(READ_WITH_TTL_SCRIPT)
                    .arg(1)
                    .arg(key)
                    .arg(max_value_bytes),
            )
            .await?;

        match status {
            READ_STATUS_MISS => return Ok(None),
            READ_STATUS_TOO_LARGE => return Err(CacheError::ValueTooLarge),
            READ_STATUS_HIT => {}
            _ => {
                return Err(CacheError::redis(
                    CacheOperation::Read,
                    RedisFailureKind::Protocol,
                ));
            }
        }

        if ttl_millis == 0 {
            return Ok(None);
        }
        if ttl_millis < 0 {
            return Err(CacheError::redis(
                CacheOperation::Read,
                RedisFailureKind::Protocol,
            ));
        }

        Ok(Some(RemoteEntry {
            value,
            ttl: Duration::from_millis(ttl_millis as u64),
        }))
    }

    async fn set(&self, key: &str, value: &[u8], ttl: Duration) -> Result<(), CacheError> {
        let ttl_millis = validate_ttl(ttl)?;
        self.query(
            CacheOperation::Write,
            redis::cmd("SET")
                .arg(key)
                .arg(value)
                .arg("PX")
                .arg(ttl_millis),
        )
        .await
    }

    async fn delete(&self, key: &str) -> Result<bool, CacheError> {
        let deleted: u64 = self
            .query(CacheOperation::Delete, redis::cmd("DEL").arg(key))
            .await?;
        Ok(deleted > 0)
    }

    async fn health_check(&self) -> Result<(), CacheError> {
        let response: String = self
            .query(CacheOperation::HealthCheck, &mut redis::cmd("PING"))
            .await?;
        if response == "PONG" {
            return Ok(());
        }
        Err(CacheError::redis(
            CacheOperation::HealthCheck,
            RedisFailureKind::Protocol,
        ))
    }
}

pub(crate) fn redis_error(operation: CacheOperation, error: &redis::RedisError) -> CacheError {
    let kind = if error.is_timeout() {
        RedisFailureKind::Timeout
    } else {
        match error.kind() {
            ErrorKind::InvalidClientConfig | ErrorKind::Client => RedisFailureKind::Configuration,
            ErrorKind::AuthenticationFailed => RedisFailureKind::Authentication,
            ErrorKind::Io
            | ErrorKind::ClusterConnectionNotFound
            | ErrorKind::MasterNameNotFoundBySentinel
            | ErrorKind::NoValidReplicasFoundBySentinel
            | ErrorKind::EmptySentinelList => RedisFailureKind::Unavailable,
            ErrorKind::Parse | ErrorKind::UnexpectedReturnType | ErrorKind::RESP3NotSupported => {
                RedisFailureKind::Protocol
            }
            ErrorKind::Server(_) | ErrorKind::Extension => RedisFailureKind::Rejected,
            _ => RedisFailureKind::Unknown,
        }
    };
    CacheError::redis(operation, kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_redis_errors_without_preserving_details() {
        let source = redis::RedisError::from((
            ErrorKind::AuthenticationFailed,
            "redis://user:secret@example.invalid",
        ));
        let error = redis_error(CacheOperation::Connect, &source);
        assert_eq!(
            error,
            CacheError::Redis {
                operation: CacheOperation::Connect,
                kind: RedisFailureKind::Authentication,
            }
        );
        assert!(!format!("{error:?}").contains("secret"));
    }

    #[test]
    fn classifies_driver_io_timeout_before_generic_unavailable() {
        let source = redis::RedisError::from(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "secret endpoint timed out",
        ));
        let error = redis_error(CacheOperation::Read, &source);
        assert_eq!(
            error,
            CacheError::Redis {
                operation: CacheOperation::Read,
                kind: RedisFailureKind::Timeout,
            }
        );
        assert!(!format!("{error:?}").contains("secret"));
    }

    #[test]
    fn classifies_unavailable_protocol_and_server_rejection_without_details() {
        let unavailable = redis_error(
            CacheOperation::RateLimitCheck,
            &redis::RedisError::from((ErrorKind::Io, "connection refused")),
        );
        assert_eq!(
            unavailable,
            CacheError::Redis {
                operation: CacheOperation::RateLimitCheck,
                kind: RedisFailureKind::Unavailable,
            }
        );

        let protocol = redis_error(
            CacheOperation::RateLimitCheck,
            &redis::RedisError::from((ErrorKind::UnexpectedReturnType, "malformed response")),
        );
        assert_eq!(
            protocol,
            CacheError::Redis {
                operation: CacheOperation::RateLimitCheck,
                kind: RedisFailureKind::Protocol,
            }
        );

        let rejected = redis_error(
            CacheOperation::RateLimitCheck,
            &redis::RedisError::from((ErrorKind::Extension, "private key")),
        );
        assert_eq!(
            rejected,
            CacheError::Redis {
                operation: CacheOperation::RateLimitCheck,
                kind: RedisFailureKind::Rejected,
            }
        );
        assert!(!format!("{rejected:?}").contains("private key"));
    }
}
