use std::collections::HashMap;
use std::sync::Arc;

use reqwest::{Client, Url};

use crate::cache::SharedResponseCache;
use crate::session::SessionStore;
use crate::upstream_request::UpstreamRequestConfig;

#[derive(Clone)]
pub struct ProviderRuntime {
    pub upstream: Url,
    pub default_max_tokens: Option<u32>,
    pub default_temperature: Option<f32>,
    pub model_map: Option<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub sessions: SessionStore,
    pub client: Client,
    pub providers: Arc<HashMap<String, ProviderRuntime>>,
    pub default_provider: Arc<String>,
    pub upstream_request: Arc<UpstreamRequestConfig>,
    pub cache: Option<SharedResponseCache>,
}

impl AppState {
    pub fn provider(&self, slug: &str) -> Option<ProviderRuntime> {
        self.providers.get(slug).cloned()
    }
}
