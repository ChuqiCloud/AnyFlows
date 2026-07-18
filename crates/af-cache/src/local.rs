use std::{
    num::NonZeroUsize,
    sync::{Mutex, MutexGuard},
    time::{Duration, Instant},
};

use lru::LruCache;

use crate::{CacheError, CacheValue};

#[derive(Clone)]
struct LocalEntry {
    value: CacheValue,
    expires_at: Instant,
}

/// 进程内严格容量 LRU；锁内只执行常数时间操作，不跨异步等待持锁。
pub(crate) struct LocalCache {
    entries: Mutex<LruCache<String, LocalEntry>>,
    capacity: NonZeroUsize,
}

impl LocalCache {
    pub(crate) fn new(capacity: NonZeroUsize) -> Self {
        Self {
            entries: Mutex::new(LruCache::new(capacity)),
            capacity,
        }
    }

    pub(crate) const fn capacity(&self) -> NonZeroUsize {
        self.capacity
    }

    pub(crate) fn get(&self, key: &str) -> Result<Option<CacheValue>, CacheError> {
        let mut entries = self.lock()?;
        let Some(entry) = entries.get(key).cloned() else {
            return Ok(None);
        };

        if entry.expires_at <= Instant::now() {
            entries.pop(key);
            return Ok(None);
        }
        Ok(Some(entry.value))
    }

    pub(crate) fn set(
        &self,
        key: String,
        value: CacheValue,
        ttl: Duration,
    ) -> Result<(), CacheError> {
        let expires_at = Instant::now()
            .checked_add(ttl)
            .ok_or(CacheError::InvalidTtl)?;
        self.lock()?.put(key, LocalEntry { value, expires_at });
        Ok(())
    }

    pub(crate) fn delete(&self, key: &str) -> Result<bool, CacheError> {
        Ok(self.lock()?.pop(key).is_some())
    }

    fn lock(&self) -> Result<MutexGuard<'_, LruCache<String, LocalEntry>>, CacheError> {
        self.entries
            .lock()
            .map_err(|_| CacheError::LocalUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, thread};

    use super::*;

    fn value(content: &'static [u8]) -> Arc<[u8]> {
        Arc::from(content)
    }

    #[test]
    fn evicts_the_least_recently_used_entry() {
        let cache = LocalCache::new(NonZeroUsize::new(2).unwrap());
        cache
            .set("a".to_owned(), value(b"a"), Duration::from_secs(1))
            .unwrap();
        cache
            .set("b".to_owned(), value(b"b"), Duration::from_secs(1))
            .unwrap();
        assert!(cache.get("a").unwrap().is_some());

        cache
            .set("c".to_owned(), value(b"c"), Duration::from_secs(1))
            .unwrap();

        assert!(cache.get("a").unwrap().is_some());
        assert!(cache.get("b").unwrap().is_none());
        assert!(cache.get("c").unwrap().is_some());
    }

    #[test]
    fn removes_expired_entries_on_read() {
        let cache = LocalCache::new(NonZeroUsize::new(1).unwrap());
        cache
            .set(
                "short".to_owned(),
                value(b"value"),
                Duration::from_millis(1),
            )
            .unwrap();
        thread::sleep(Duration::from_millis(20));
        assert!(cache.get("short").unwrap().is_none());
    }
}
