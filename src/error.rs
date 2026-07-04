use thiserror::Error;

#[derive(Error, Debug)]
pub enum BridgeError {
    #[error("HTTP请求错误: {0}")]
    Http(#[from] reqwest::Error),
    #[error("序列化错误: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("上游错误: {0}")]
    Upstream(String),
}
