use std::fs;
use std::path::{Path, PathBuf};

use reqwest::{Client, Url};
use serde_json::{json, Value};

use crate::handlers::join_base;

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
        || lower.contains("deepseek-v4");

    let (ctx, max_ctx) = if lower.contains("deepseek") {
        (262_144, 1_048_576)
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
) {
    let url = format!("{}models", join_base(upstream));
    let mut builder = client.get(&url);
    if !api_key.is_empty() {
        builder = builder.bearer_auth(api_key);
    }

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

    // Always include the configured default and common DeepSeek models so Codex
    // has catalog entries even when /v1/models omits them.
    for known in known_deepseek_models(default_model) {
        if !models.iter().any(|m| m == &known) {
            models.push(known);
        }
    }

    let preferred = if models.iter().any(|m| m == default_model) {
        default_model.to_string()
    } else {
        models
            .iter()
            .find(|m| m.contains("deepseek"))
            .or(models.first())
            .cloned()
            .unwrap_or_else(|| default_model.to_string())
    };

    let catalog_path = default_catalog_path();
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

    println!("# ── Codex config snippet for CrabBridge + DeepSeek ──");
    println!("# Copy the lines below into ~/.codex/config.toml");
    println!("#");
    println!("# Codex 0.105+ ignores [model_properties.*]. Metadata must come from");
    println!("# model_catalog_json (a ModelsResponse JSON file). Setting this path");
    println!("# replaces Codex's remote model catalog for this config.");
    println!();
    println!("model_provider = \"{provider_name}\"");
    println!("model = \"{preferred}\"");
    println!(
        "model_catalog_json = \"{}\"",
        catalog_path.display()
    );
    println!();
    println!("[model_providers.{provider_name}]");
    println!("name = \"{provider_name}\"");
    println!("base_url = \"http://127.0.0.1:11435/v1\"");
    println!("wire_api = \"responses\"");
    println!("env_key = \"DEEPSEEK_API_KEY\"");
    println!();

    // Also print the catalog JSON so users can copy it if the write failed.
    let catalog = build_model_catalog(&models);
    println!("# ── Optional: contents of {} ──", catalog_path.display());
    println!(
        "{}",
        serde_json::to_string_pretty(&catalog).unwrap_or_else(|_| "{}".into())
    );
}

/// Models Codex commonly selects when using DeepSeek via CrabBridge.
fn known_deepseek_models(default_model: &str) -> Vec<String> {
    let mut models = vec![
        "deepseek-chat".into(),
        "deepseek-reasoner".into(),
        "deepseek-v4-pro".into(),
        "deepseek-v4-flash".into(),
    ];
    if !models.iter().any(|m| m == default_model) {
        models.insert(0, default_model.to_string());
    }
    models
}

fn default_catalog_path() -> PathBuf {
    if let Ok(home) = std::env::var("CODEX_HOME") {
        return PathBuf::from(home).join("crabbridge-models.json");
    }
    dirs_home()
        .map(|h| h.join(".codex").join("crabbridge-models.json"))
        .unwrap_or_else(|| PathBuf::from("crabbridge-models.json"))
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn write_model_catalog(path: &Path, models: &[String]) -> std::io::Result<()> {
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
}
