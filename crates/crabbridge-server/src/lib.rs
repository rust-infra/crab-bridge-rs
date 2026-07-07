//! HTTP bridge server: Responses API ↔ Chat Completions translation.

pub mod admin;
pub mod app;
pub mod cache;
pub mod handlers;
pub mod metrics;
pub mod opts;
pub mod prompt;
pub mod serve;
pub mod server;
pub mod session;
pub(crate) mod session_sqlite;
pub mod state;
pub mod stream;
pub mod translate;
pub mod upstream_request;
