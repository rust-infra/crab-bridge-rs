//! Bridge server lifecycle for the desktop host.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use crabbridge_core::config::{load_config_file, user_config_path};
use crabbridge_server::opts::ServeArgs;
use crabbridge_server::serve::ServeHandle;
use crabbridge_server::session::{DEFAULT_MAX_SESSIONS, DEFAULT_SESSION_TTL};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeStatus {
    Stopped,
    Running,
    Error,
}

pub struct BridgeManager {
    inner: Mutex<BridgeInner>,
    config_dir: PathBuf,
}

struct BridgeInner {
    handle: Option<ServeHandle>,
    last_error: Option<String>,
    config_path: PathBuf,
    bind_addr: SocketAddr,
}

impl BridgeManager {
    pub fn new(config_path: PathBuf, bind_addr: SocketAddr) -> Result<Self> {
        let config_dir = config_path
            .parent()
            .context("config path must have a parent directory")?
            .to_path_buf();
        Ok(Self {
            inner: Mutex::new(BridgeInner {
                handle: None,
                last_error: None,
                config_path,
                bind_addr,
            }),
            config_dir,
        })
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    pub fn status(&self) -> BridgeStatus {
        let inner = self.inner.lock().expect("bridge manager lock");
        if inner.handle.is_some() {
            BridgeStatus::Running
        } else if inner.last_error.is_some() {
            BridgeStatus::Error
        } else {
            BridgeStatus::Stopped
        }
    }

    pub fn bind_addr(&self) -> SocketAddr {
        self.inner.lock().expect("bridge manager lock").bind_addr
    }

    pub fn config_path(&self) -> PathBuf {
        self.inner
            .lock()
            .expect("bridge manager lock")
            .config_path
            .clone()
    }

    pub fn admin_url(&self) -> Option<String> {
        let inner = self.inner.lock().expect("bridge manager lock");
        inner.handle.as_ref().map(ServeHandle::admin_url)
    }

    pub fn last_error(&self) -> Option<String> {
        self.inner
            .lock()
            .expect("bridge manager lock")
            .last_error
            .clone()
    }

    pub async fn start(&self) -> Result<()> {
        let (config_path, bind_addr) = {
            let inner = self.inner.lock().expect("bridge manager lock");
            if inner.handle.is_some() {
                bail!("bridge is already running");
            }
            (inner.config_path.clone(), inner.bind_addr)
        };

        let serve = serve_args_for_desktop(&config_path, bind_addr)?;
        crate::secrets::hydrate_api_keys()?;

        match crabbridge_server::serve::start_serve(serve, Some(config_path), false).await {
            Ok(handle) => {
                let mut inner = self.inner.lock().expect("bridge manager lock");
                inner.last_error = None;
                inner.handle = Some(handle);
                Ok(())
            }
            Err(err) => {
                let mut inner = self.inner.lock().expect("bridge manager lock");
                inner.last_error = Some(err.to_string());
                Err(err)
            }
        }
    }

    pub async fn stop(&self) -> Result<()> {
        let handle = {
            let mut inner = self.inner.lock().expect("bridge manager lock");
            inner.handle.take().context("bridge is not running")?
        };
        let result = handle.shutdown().await;
        if result.is_ok() {
            let mut inner = self.inner.lock().expect("bridge manager lock");
            inner.last_error = None;
        }
        result
    }
}

pub fn default_config_path() -> PathBuf {
    user_config_path()
}

pub fn ensure_config_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    Ok(())
}

pub fn config_exists(path: &Path) -> bool {
    path.is_file()
}

pub fn serve_args_for_desktop(config_path: &Path, bind_addr: SocketAddr) -> Result<ServeArgs> {
    let data_root = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let mut args = ServeArgs {
        bind_addr,
        max_tokens: None,
        temperature: None,
        log_level: "info".to_string(),
        cache_enabled: false,
        cache_ttl_secs: 300,
        cache_max_entries: 1000,
        rate_limit_rps: 0,
        max_sessions: DEFAULT_MAX_SESSIONS,
        session_ttl_hours: DEFAULT_SESSION_TTL.as_secs() / 60 / 60,
        session_db: data_root.join("data/crabbridge.db"),
        session_memory_only: false,
    };

    if config_path.is_file() {
        let cfg = load_config_file(config_path)?;
        if let Some(server) = cfg.server {
            if let Some(addr) = server.bind_addr {
                args.bind_addr = addr
                    .parse()
                    .with_context(|| format!("invalid bind_addr in {}", config_path.display()))?;
            }
            if let Some(level) = server.log_level {
                args.log_level = level;
            }
            args.max_tokens = server.max_tokens;
            args.temperature = server.temperature;
        }
        if let Some(session) = cfg.session {
            if let Some(db) = session.db {
                args.session_db = PathBuf::from(db);
            }
            if let Some(memory_only) = session.memory_only {
                args.session_memory_only = memory_only;
            }
            if let Some(max_sessions) = session.max_sessions {
                args.max_sessions = max_sessions;
            }
            if let Some(ttl_hours) = session.ttl_hours {
                args.session_ttl_hours = ttl_hours;
            }
        }
        if let Some(cache) = cfg.cache {
            if let Some(enabled) = cache.enabled {
                args.cache_enabled = enabled;
            }
            if let Some(ttl) = cache.ttl_secs {
                args.cache_ttl_secs = ttl;
            }
            if let Some(max_entries) = cache.max_entries {
                args.cache_max_entries = max_entries;
            }
        }
        if let Some(rate) = cfg.rate_limit
            && let Some(rps) = rate.rps
        {
            args.rate_limit_rps = rps;
        }
    }

    if let Some(parent) = args.session_db.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("failed to create session db directory {}", parent.display())
        })?;
    }

    Ok(args)
}
