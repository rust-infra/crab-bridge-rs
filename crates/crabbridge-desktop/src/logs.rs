//! In-memory and file logging for the desktop app.

use std::collections::VecDeque;
use std::fs::{OpenOptions, create_dir_all};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result};
use serde::Serialize;
use tracing::Level;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::{Layer, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;

const MAX_LINES: usize = 500;

struct LogState {
    lines: VecDeque<String>,
    log_path: PathBuf,
}

static LOG_STATE: OnceLock<Mutex<LogState>> = OnceLock::new();

struct MemoryLogLayer;

impl<S> Layer<S> for MemoryLogLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let meta = event.metadata();
        let level = meta.level();
        if *level < Level::INFO {
            return;
        }

        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let target = meta.target();
        let short_target = target.rsplit("::").next().unwrap_or(target);
        let line = format!(
            "{} [{level}] {short_target}: {}",
            local_timestamp(),
            visitor.message
        );

        let Some(state) = LOG_STATE.get() else {
            return;
        };
        let Ok(mut guard) = state.lock() else {
            return;
        };

        if guard.lines.len() >= MAX_LINES {
            guard.lines.pop_front();
        }
        guard.lines.push_back(line.clone());

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&guard.log_path)
        {
            let _ = writeln!(file, "{line}");
        }
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}").trim_matches('"').to_string();
        }
    }
}

fn local_timestamp() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = duration.as_secs();
    let hours = (total_secs / 3600) % 24;
    let mins = (total_secs / 60) % 60;
    let secs = total_secs % 60;
    format!("{hours:02}:{mins:02}:{secs:02}")
}

#[derive(Debug, Clone, Serialize)]
pub struct LogSnapshot {
    pub path: String,
    pub lines: Vec<String>,
}

pub fn init_tracing(config_dir: &Path) -> Result<PathBuf> {
    create_dir_all(config_dir)
        .with_context(|| format!("failed to create {}", config_dir.display()))?;
    let log_path = config_dir.join("desktop.log");

    LOG_STATE
        .set(Mutex::new(LogState {
            lines: VecDeque::with_capacity(MAX_LINES),
            log_path: log_path.clone(),
        }))
        .ok();

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(MemoryLogLayer)
        .try_init()
        .ok();

    tracing::info!(path = %log_path.display(), "desktop logging initialized");
    Ok(log_path)
}

pub fn tail_logs(limit: usize) -> Result<LogSnapshot> {
    let state = LOG_STATE.get().context("logging not initialized")?;
    let guard = state
        .lock()
        .map_err(|_| anyhow::anyhow!("log buffer lock poisoned"))?;

    let take = limit.min(guard.lines.len());
    let start = guard.lines.len().saturating_sub(take);
    Ok(LogSnapshot {
        path: guard.log_path.display().to_string(),
        lines: guard.lines.range(start..).cloned().collect(),
    })
}

pub fn log_path() -> Option<PathBuf> {
    LOG_STATE
        .get()
        .and_then(|state| state.lock().ok().map(|guard| guard.log_path.clone()))
}
