use std::sync::Arc;

use reqwest::{Client, Url};

use crate::cache::SharedResponseCache;
use crate::session::SessionStore;
use crate::upstream_request::UpstreamRequestConfig;

#[derive(Clone)]
pub struct AppState {
    pub sessions: SessionStore,
    pub client: Client,
    pub upstream: Arc<Url>,
    pub api_key: Arc<String>,
    pub default_model: Arc<String>,
    pub default_max_tokens: Option<u32>,
    pub default_temperature: Option<f32>,
    pub upstream_request: Arc<UpstreamRequestConfig>,
    pub cache: Option<SharedResponseCache>,
}
