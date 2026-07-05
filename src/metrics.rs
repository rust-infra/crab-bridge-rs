//! Runtime request/cache metrics for the admin dashboard and Prometheus export.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use serde::Serialize;

const PROVIDER_LABELS: [&str; 3] = ["deepseek", "kimi", "other"];

#[derive(Default)]
struct ProviderMetrics {
    responses_total: AtomicU64,
    models_total: AtomicU64,
    errors_total: AtomicU64,
    duration_ms_sum: AtomicU64,
    duration_ms_count: AtomicU64,
    stream_total: AtomicU64,
}

impl ProviderMetrics {
    fn snapshot(&self, label: &str) -> ProviderMetricsSnapshot {
        let responses = self.responses_total.load(Ordering::Relaxed);
        let models = self.models_total.load(Ordering::Relaxed);
        let errors = self.errors_total.load(Ordering::Relaxed);
        let duration_sum = self.duration_ms_sum.load(Ordering::Relaxed);
        let duration_count = self.duration_ms_count.load(Ordering::Relaxed);
        ProviderMetricsSnapshot {
            provider: label.to_string(),
            responses_total: responses,
            models_total: models,
            requests_total: responses + models,
            errors_total: errors,
            stream_total: self.stream_total.load(Ordering::Relaxed),
            avg_duration_ms: if duration_count == 0 {
                0.0
            } else {
                duration_sum as f64 / duration_count as f64
            },
        }
    }
}

pub struct BridgeMetrics {
    providers: [ProviderMetrics; 3],
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
}

impl Default for BridgeMetrics {
    fn default() -> Self {
        Self {
            providers: Default::default(),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderMetricsSnapshot {
    pub provider: String,
    pub requests_total: u64,
    pub responses_total: u64,
    pub models_total: u64,
    pub errors_total: u64,
    pub stream_total: u64,
    pub avg_duration_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub uptime_secs: u64,
    pub requests_total: u64,
    pub errors_total: u64,
    pub stream_requests_total: u64,
    pub cache_hits_total: u64,
    pub cache_misses_total: u64,
    pub cache_hit_rate: f64,
    pub avg_duration_ms: f64,
    pub by_provider: Vec<ProviderMetricsSnapshot>,
}

impl BridgeMetrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn provider_slot(provider: &str) -> usize {
        match provider {
            "deepseek" => 0,
            "kimi" => 1,
            _ => 2,
        }
    }

    pub fn record_request(
        &self,
        provider: &str,
        route: &str,
        status: u16,
        elapsed_ms: u64,
        stream: bool,
    ) {
        let slot = &self.providers[Self::provider_slot(provider)];
        match route {
            "responses" => slot.responses_total.fetch_add(1, Ordering::Relaxed),
            "models" => slot.models_total.fetch_add(1, Ordering::Relaxed),
            _ => slot.responses_total.fetch_add(1, Ordering::Relaxed),
        };
        if status >= 400 {
            slot.errors_total.fetch_add(1, Ordering::Relaxed);
        }
        if stream {
            slot.stream_total.fetch_add(1, Ordering::Relaxed);
        }
        slot.duration_ms_sum
            .fetch_add(elapsed_ms, Ordering::Relaxed);
        slot.duration_ms_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self, started_at: Instant) -> MetricsSnapshot {
        let by_provider: Vec<_> = PROVIDER_LABELS
            .iter()
            .zip(self.providers.iter())
            .map(|(label, metrics)| metrics.snapshot(label))
            .collect();

        let requests_total: u64 = by_provider.iter().map(|p| p.requests_total).sum();
        let errors_total: u64 = by_provider.iter().map(|p| p.errors_total).sum();
        let stream_requests_total: u64 = by_provider.iter().map(|p| p.stream_total).sum();
        let cache_hits = self.cache_hits.load(Ordering::Relaxed);
        let cache_misses = self.cache_misses.load(Ordering::Relaxed);
        let cache_total = cache_hits + cache_misses;

        let duration_sum: u64 = self
            .providers
            .iter()
            .fold(0, |acc, p| acc + p.duration_ms_sum.load(Ordering::Relaxed));
        let duration_count: u64 = self.providers.iter().fold(0, |acc, p| {
            acc + p.duration_ms_count.load(Ordering::Relaxed)
        });

        MetricsSnapshot {
            uptime_secs: started_at.elapsed().as_secs(),
            requests_total,
            errors_total,
            stream_requests_total,
            cache_hits_total: cache_hits,
            cache_misses_total: cache_misses,
            cache_hit_rate: if cache_total == 0 {
                0.0
            } else {
                cache_hits as f64 / cache_total as f64
            },
            avg_duration_ms: if duration_count == 0 {
                0.0
            } else {
                duration_sum as f64 / duration_count as f64
            },
            by_provider,
        }
    }

    pub fn to_prometheus(&self, started_at: Instant) -> String {
        let snapshot = self.snapshot(started_at);
        let mut out = String::new();

        out.push_str("# HELP crabbridge_uptime_seconds Process uptime in seconds.\n");
        out.push_str("# TYPE crabbridge_uptime_seconds gauge\n");
        out.push_str(&format!(
            "crabbridge_uptime_seconds {}\n",
            snapshot.uptime_secs
        ));

        out.push_str(
            "# HELP crabbridge_requests_total Total HTTP requests handled by the bridge.\n",
        );
        out.push_str("# TYPE crabbridge_requests_total counter\n");
        for provider in &snapshot.by_provider {
            out.push_str(&format!(
                "crabbridge_requests_total{{provider=\"{}\",route=\"responses\"}} {}\n",
                provider.provider, provider.responses_total
            ));
            out.push_str(&format!(
                "crabbridge_requests_total{{provider=\"{}\",route=\"models\"}} {}\n",
                provider.provider, provider.models_total
            ));
        }

        out.push_str(
            "# HELP crabbridge_request_errors_total Total failed HTTP requests (status >= 400).\n",
        );
        out.push_str("# TYPE crabbridge_request_errors_total counter\n");
        for provider in &snapshot.by_provider {
            out.push_str(&format!(
                "crabbridge_request_errors_total{{provider=\"{}\"}} {}\n",
                provider.provider, provider.errors_total
            ));
        }

        out.push_str(
            "# HELP crabbridge_stream_requests_total Total streaming responses requests.\n",
        );
        out.push_str("# TYPE crabbridge_stream_requests_total counter\n");
        for provider in &snapshot.by_provider {
            out.push_str(&format!(
                "crabbridge_stream_requests_total{{provider=\"{}\"}} {}\n",
                provider.provider, provider.stream_total
            ));
        }

        out.push_str("# HELP crabbridge_cache_hits_total Non-streaming response cache hits.\n");
        out.push_str("# TYPE crabbridge_cache_hits_total counter\n");
        out.push_str(&format!(
            "crabbridge_cache_hits_total {}\n",
            snapshot.cache_hits_total
        ));

        out.push_str("# HELP crabbridge_cache_misses_total Non-streaming response cache misses.\n");
        out.push_str("# TYPE crabbridge_cache_misses_total counter\n");
        out.push_str(&format!(
            "crabbridge_cache_misses_total {}\n",
            snapshot.cache_misses_total
        ));

        out.push_str(
            "# HELP crabbridge_request_duration_ms_sum Cumulative request latency in milliseconds.\n",
        );
        out.push_str("# TYPE crabbridge_request_duration_ms_sum counter\n");
        for (label, slot) in PROVIDER_LABELS.iter().zip(self.providers.iter()) {
            out.push_str(&format!(
                "crabbridge_request_duration_ms_sum{{provider=\"{}\"}} {}\n",
                label,
                slot.duration_ms_sum.load(Ordering::Relaxed)
            ));
        }

        out.push_str(
            "# HELP crabbridge_request_duration_ms_count Request count for latency averaging.\n",
        );
        out.push_str("# TYPE crabbridge_request_duration_ms_count counter\n");
        for (label, slot) in PROVIDER_LABELS.iter().zip(self.providers.iter()) {
            out.push_str(&format!(
                "crabbridge_request_duration_ms_count{{provider=\"{}\"}} {}\n",
                label,
                slot.duration_ms_count.load(Ordering::Relaxed)
            ));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_requests_and_exports_prometheus() {
        let metrics = BridgeMetrics::new();
        let started = Instant::now();
        metrics.record_request("deepseek", "responses", 200, 120, false);
        metrics.record_request("kimi", "models", 502, 45, false);
        metrics.record_cache_hit();
        metrics.record_cache_miss();

        let snapshot = metrics.snapshot(started);
        assert_eq!(snapshot.requests_total, 2);
        assert_eq!(snapshot.errors_total, 1);
        assert_eq!(snapshot.cache_hits_total, 1);
        assert_eq!(snapshot.cache_misses_total, 1);

        let body = metrics.to_prometheus(started);
        assert!(
            body.contains("crabbridge_requests_total{provider=\"deepseek\",route=\"responses\"} 1")
        );
        assert!(body.contains("crabbridge_request_errors_total{provider=\"kimi\"} 1"));
        assert!(body.contains("crabbridge_cache_hits_total 1"));
    }
}
