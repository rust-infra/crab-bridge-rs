use std::fs;
use std::path::{Path, PathBuf};

use std::net::SocketAddr;

use reqwest::{Client, Url};
use serde_json::{json, Value};

use crate::handlers::join_base;
use crate::provider::{ProviderKind, apply_upstream_headers};

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

pub async fn print_codex_config(
    client: &Client,
    upstream: &Url,
    api_key: &str,
    provider_name: &str,
    default_model: &str,
    bind_addr: &SocketAddr,
    route_slug: &str,
) {
    let kind = ProviderKind::from_base_url(upstream.as_str());
    let models = prepare_model_catalog(client, upstream, api_key, kind, default_model).await;

    let preferred = preferred_model(&models, kind, default_model);

    let catalog_path = catalog_path_for_slug(route_slug);
    match write_model_catalog(&catalog_path, &models) {
        Ok(()) => {
            eprintln!(
                "// Wrote Codex model catalog to {}",
                catalog_path.display()
            );
        }
        Err(e) => {
            eprintln!(
                "// Failed to write model catalog to {}: {e}",
                catalog_path.display()
            );
            eprintln!("// Paste the JSON block at the end of this output into that path yourself.");
        }
    }

    let env_key = kind.codex_env_key();
    let bridge_base_url = format!("http://{bind_addr}/{route_slug}/v1");
    println!(
        "# ── Codex config snippet for CrabBridge + {} ──",
        kind.label()
    );
    println!("# Copy the lines below into ~/.codex/config.toml");
    println!("#");
    println!("# Codex 0.105+ ignores [model_properties.*]. Metadata must come from");
    println!("# model_catalog_json (a ModelsResponse JSON file). Setting this path");
    println!("# replaces Codex's remote model catalog for this config.");
    println!();
    println!("model_provider = \"{provider_name}\"");
    println!("model = \"{preferred}\"");
    println!("model_catalog_json = \"{}\"", catalog_path.display());
    println!();
    println!("[model_providers.{provider_name}]");
    println!("name = \"{provider_name}\"");
    println!("base_url = \"{bridge_base_url}\"");
    println!("wire_api = \"responses\"");
    println!("env_key = \"{env_key}\"");
    println!();

    // Also print the catalog JSON so users can copy it if the write failed.
    let catalog = build_model_catalog(&models);
    println!("# ── Optional: contents of {} ──", catalog_path.display());
    println!(
        "{}",
        serde_json::to_string_pretty(&catalog).unwrap_or_else(|_| "{}".into())
    );
}

/// Fetch upstream models and merge with provider-known defaults.
pub async fn prepare_model_catalog(
    client: &Client,
    upstream: &Url,
    api_key: &str,
    kind: ProviderKind,
    default_model: &str,
) -> Vec<String> {
    let url = format!("{}models", join_base(upstream));
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

fn preferred_model(models: &[String], kind: ProviderKind, default_model: &str) -> String {
    if models.iter().any(|m| m == default_model) {
        default_model.to_string()
    } else {
        models
            .iter()
            .find(|m| match kind {
                ProviderKind::Kimi => {
                    *m == "kimi-for-coding" || m.contains("kimi") || m.contains("moonshot")
                }
                ProviderKind::DeepSeek => m.contains("deepseek"),
                ProviderKind::Custom => true,
            })
            .or(models.first())
            .cloned()
            .unwrap_or_else(|| default_model.to_string())
    }
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

pub fn default_catalog_path() -> PathBuf {
    catalog_path_for_slug("deepseek")
}

pub fn codex_home_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("CODEX_HOME") {
        return Some(PathBuf::from(home));
    }
    dirs_home().map(|h| h.join(".codex"))
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
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
