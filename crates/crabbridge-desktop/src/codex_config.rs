use std::fs;
use std::path::{Path, PathBuf};

use reqwest::{Client, Url};
use serde_json::{Value, json};
use tracing::warn;

use crabbridge_core::provider::{ProviderKind, apply_upstream_headers, join_upstream_base};

pub struct ModelProps {
    pub context_window: u32,
    pub max_context_window: u32,
    pub supports_parallel_tool_calls: bool,
    pub supports_reasoning_summaries: bool,
}

pub fn estimate_model_properties(model_id: &str) -> ModelProps {
    let lower = model_id.to_lowercase();

    let has_reasoning = lower.contains("reasoner")
        || lower.contains("r1")
        || lower.contains("thinking")
        || lower.contains("deepseek-v4")
        || lower.contains("kimi")
        || lower.contains("moonshot");

    let (ctx, max_ctx) = if lower.contains("deepseek") {
        (262_144, 1_048_576)
    } else if lower.contains("kimi") || lower.contains("moonshot") {
        (262_144, 262_144)
    } else {
        (128_000, 128_000)
    };

    ModelProps {
        context_window: ctx,
        max_context_window: max_ctx,
        supports_parallel_tool_calls: true,
        supports_reasoning_summaries: has_reasoning,
    }
}

/// Fetch upstream models and merge with provider-known defaults.
pub async fn prepare_model_catalog(
    client: &Client,
    upstream: &Url,
    api_key: &str,
    kind: ProviderKind,
    default_model: &str,
) -> Vec<String> {
    let url = format!("{}models", join_upstream_base(upstream));
    let builder = apply_upstream_headers(client.get(&url), kind, api_key);

    let mut models: Vec<String> = match builder.send().await {
        Ok(r) if r.status().is_success() => match r.json::<serde_json::Value>().await {
            Ok(body) => body
                .get("data")
                .or_else(|| body.get("models"))
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|m| m.get("id").and_then(|id| id.as_str()).map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            Err(e) => {
                eprintln!("// Failed to parse upstream models: {e}");
                Vec::new()
            }
        },
        status => {
            eprintln!("// Failed to fetch upstream models (status: {status:?})");
            Vec::new()
        }
    };

    models.retain(|m| kind.model_matches_provider(m));

    for known in known_models_for(kind, upstream.as_str(), default_model) {
        if !models.iter().any(|m| m == &known) {
            models.push(known);
        }
    }

    models
}

fn known_models_for(kind: ProviderKind, base_url: &str, default_model: &str) -> Vec<String> {
    let mut models: Vec<String> = kind
        .known_models_for_upstream(base_url)
        .iter()
        .map(|m| (*m).to_string())
        .collect();
    if !models.iter().any(|m| m == default_model) {
        models.insert(0, default_model.to_string());
    }
    models
}

pub fn catalog_path_for_slug(slug: &str) -> PathBuf {
    codex_home_dir()
        .map(|h| h.join(format!("crabbridge-models-{slug}.json")))
        .unwrap_or_else(|| PathBuf::from(format!("crabbridge-models-{slug}.json")))
}

pub fn codex_home_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("CODEX_HOME") {
        return Some(PathBuf::from(home));
    }
    dirs_home().map(|h| h.join(".codex"))
}

fn dirs_home() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    if home.is_none() {
        warn!(
            "codex_home_dir: HOME and USERPROFILE not set, cannot resolve Codex config directory"
        );
    }
    home
}

pub fn write_model_catalog(path: &Path, models: &[String]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let catalog = build_model_catalog(models);
    let body = serde_json::to_string_pretty(&catalog)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(path, body)
}

fn build_model_catalog(models: &[String]) -> Value {
    let entries: Vec<Value> = models
        .iter()
        .enumerate()
        .map(|(idx, model)| catalog_entry(model, idx as i32))
        .collect();
    json!({ "models": entries })
}

fn catalog_entry(model: &str, priority: i32) -> Value {
    let props = estimate_model_properties(model);
    let display = display_name(model);
    let reasoning_levels = if props.supports_reasoning_summaries {
        json!([
            { "effort": "low", "description": "Faster responses with lighter reasoning" },
            { "effort": "medium", "description": "Balances speed and reasoning depth" },
            { "effort": "high", "description": "Greater reasoning depth for complex problems" }
        ])
    } else {
        json!([])
    };
    let default_reasoning = if props.supports_reasoning_summaries {
        json!("medium")
    } else {
        Value::Null
    };

    json!({
        "slug": model,
        "display_name": display,
        "description": format!("{display} via CrabBridge"),
        "default_reasoning_level": default_reasoning,
        "supported_reasoning_levels": reasoning_levels,
        "shell_type": "shell_command",
        "visibility": "list",
        "supported_in_api": true,
        "priority": priority,
        "availability_nux": null,
        "upgrade": null,
        "base_instructions": BASE_INSTRUCTIONS,
        "supports_reasoning_summaries": props.supports_reasoning_summaries,
        "support_verbosity": false,
        "default_verbosity": null,
        "apply_patch_tool_type": "freeform",
        "truncation_policy": { "mode": "tokens", "limit": 10000 },
        "supports_parallel_tool_calls": props.supports_parallel_tool_calls,
        "context_window": props.context_window,
        "max_context_window": props.max_context_window,
        "experimental_supported_tools": [],
        "input_modalities": ["text"]
    })
}

fn display_name(model: &str) -> String {
    model
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

const BASE_INSTRUCTIONS: &str = "\
You are a coding agent working in the user's workspace. \
Read the codebase carefully, implement changes end to end, and verify your work. \
Prefer existing project patterns over inventing new abstractions. \
Use tools when needed and keep responses concise.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_includes_required_fields() {
        let catalog = build_model_catalog(&["deepseek-v4-pro".into()]);
        let model = &catalog["models"][0];
        assert_eq!(model["slug"], "deepseek-v4-pro");
        assert!(model["base_instructions"].as_str().unwrap().len() > 10);
        assert_eq!(model["shell_type"], "shell_command");
        assert_eq!(model["supports_reasoning_summaries"], true);
        assert_eq!(model["context_window"], 262144);
        assert!(model["experimental_supported_tools"].is_array());
        assert!(model.get("availability_nux").is_some());
        assert!(model.get("upgrade").is_some());
        assert!(model.get("default_verbosity").is_some());
        assert!(model.get("apply_patch_tool_type").is_some());
    }

    #[test]
    fn kimi_for_coding_has_reasoning_metadata() {
        let props = estimate_model_properties("kimi-for-coding");
        assert!(props.supports_reasoning_summaries);
        assert_eq!(props.context_window, 262_144);

        let catalog = build_model_catalog(&["kimi-for-coding".into()]);
        assert_eq!(catalog["models"][0]["slug"], "kimi-for-coding");
        assert_eq!(catalog["models"][0]["supports_reasoning_summaries"], true);
    }
}
