use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use moka::future::Cache;
use serde::Serialize;
use serde_json::Value;
use tracing::debug;

#[derive(Debug, Clone, Serialize)]
pub struct CacheStats {
    pub enabled: bool,
    pub entry_count: u64,
    pub weighted_size_bytes: u64,
}

#[derive(Clone)]
pub struct ResponseCache {
    inner: Cache<String, Bytes>,
}

impl ResponseCache {
    pub fn new(max_entries: u64, ttl_secs: u64) -> Self {
        Self {
            inner: Cache::builder()
                .max_capacity(max_entries)
                .time_to_live(Duration::from_secs(ttl_secs))
                .build(),
        }
    }

    pub fn cache_key(provider: &str, url: &str, api_key: &str, body: &Value) -> String {
        let serialized = serde_json::to_string(body).unwrap_or_default();
        let mut hasher = DefaultHasher::new();
        provider.hash(&mut hasher);
        url.hash(&mut hasher);
        api_key.hash(&mut hasher);
        serialized.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    pub async fn get(&self, key: &str) -> Option<Bytes> {
        self.inner.get(key).await
    }

    pub async fn insert(&self, key: String, value: Bytes) {
        debug!(cache_key = %key, bytes = value.len(), "cached chat completion response");
        self.inner.insert(key, value).await;
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            enabled: true,
            entry_count: self.inner.entry_count(),
            weighted_size_bytes: self.inner.weighted_size(),
        }
    }
}

pub type SharedResponseCache = Arc<ResponseCache>;
