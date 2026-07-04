//! Upstream provider presets (DeepSeek, Kimi/Moonshot, custom).

use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    DeepSeek,
    Kimi,
    Custom,
}

impl ProviderKind {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "deepseek" | "ds" => Self::DeepSeek,
            // Kimi Code (kimi-for-coding) is the default Kimi integration path.
            "kimi" | "kimi-code" | "kimi-for-coding" | "coding" | "moonshot" | "moonshot-ai" => {
                Self::Kimi
            }
            _ => Self::Custom,
        }
    }

    pub fn from_base_url(base_url: &str) -> Self {
        let lower = base_url.to_ascii_lowercase();
        if let Some(route) = bridge_route_from_url(base_url) {
            return Self::parse(route);
        }
        if lower.contains("deepseek") {
            Self::DeepSeek
        } else if lower.contains("api.kimi.com/coding")
            || lower.contains("moonshot")
            || lower.contains("kimi")
        {
            Self::Kimi
        } else {
            Self::Custom
        }
    }

    /// Route slug used in `http://host/{slug}/v1` and `[providers.{slug}]`.
    pub fn route_slug(self) -> &'static str {
        match self {
            Self::DeepSeek => "deepseek",
            Self::Kimi => "kimi",
            Self::Custom => "custom",
        }
    }

    /// Parse a bridge route segment (`deepseek`, `kimi`, …).
    pub fn from_route(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "deepseek" | "ds" => Some(Self::DeepSeek),
            "kimi" | "kimi-code" | "kimi-for-coding" | "coding" | "moonshot" | "moonshot-ai" => {
                Some(Self::Kimi)
            }
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

    /// Codex `model_providers` table name for a route slug.
    pub fn codex_provider_name(slug: &str) -> String {
        format!("crabbridge-{slug}")
    }

    pub fn builtin_slugs() -> &'static [&'static str] {
        &["deepseek", "kimi"]
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::DeepSeek => "https://api.deepseek.com/v1",
            // Kimi Code OpenAI-compatible endpoint (membership / coding agents).
            Self::Kimi => "https://api.kimi.com/coding/v1",
            Self::Custom => "https://api.deepseek.com/v1",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Self::DeepSeek => "deepseek-v4-pro",
            // Stable alias; backend maps to the latest coding model.
            Self::Kimi => "kimi-for-coding",
            Self::Custom => "deepseek-v4-pro",
        }
    }

    /// Env var Codex should read for the provider API key (`env_key` in config.toml).
    pub fn codex_env_key(self) -> &'static str {
        match self {
            Self::DeepSeek => "DEEPSEEK_API_KEY",
            Self::Kimi => "KIMI_API_KEY",
            Self::Custom => "UPSTREAM_API_KEY",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::DeepSeek => "DeepSeek",
            Self::Kimi => "Kimi Code",
            Self::Custom => "upstream",
        }
    }

    pub fn known_models(self) -> &'static [&'static str] {
        match self {
            Self::DeepSeek => &[
                "deepseek-chat",
                "deepseek-reasoner",
                "deepseek-v4-pro",
                "deepseek-v4-flash",
            ],
            Self::Kimi => &["kimi-for-coding"],
            Self::Custom => &[],
        }
    }

    pub fn preferred_api_key_vars(self) -> &'static [&'static str] {
        match self {
            Self::DeepSeek => &["DEEPSEEK_API_KEY", "UPSTREAM_API_KEY"],
            Self::Kimi => &["KIMI_API_KEY", "MOONSHOT_API_KEY", "UPSTREAM_API_KEY"],
            Self::Custom => &[
                "UPSTREAM_API_KEY",
                "DEEPSEEK_API_KEY",
                "KIMI_API_KEY",
                "MOONSHOT_API_KEY",
            ],
        }
    }

    fn preferred_base_url_vars(self) -> &'static [&'static str] {
        match self {
            Self::DeepSeek => &["DEEPSEEK_BASE_URL", "UPSTREAM_BASE_URL"],
            Self::Kimi => &["KIMI_BASE_URL", "KIMI_CODE_BASE_URL", "UPSTREAM_BASE_URL"],
            Self::Custom => &[
                "UPSTREAM_BASE_URL",
                "DEEPSEEK_BASE_URL",
                "KIMI_BASE_URL",
                "MOONSHOT_BASE_URL",
            ],
        }
    }

    fn preferred_model_vars(self) -> &'static [&'static str] {
        match self {
            Self::DeepSeek => &["DEEPSEEK_MODEL", "UPSTREAM_MODEL"],
            Self::Kimi => &["KIMI_MODEL", "UPSTREAM_MODEL"],
            Self::Custom => &[
                "UPSTREAM_MODEL",
                "DEEPSEEK_MODEL",
                "KIMI_MODEL",
                "MOONSHOT_MODEL",
            ],
        }
    }
}

/// Resolve provider aliases into `UPSTREAM_*` before Clap reads the environment.
pub fn bootstrap_upstream_env() {
    let provider = env::var("CRABRIDGE_PROVIDER")
        .or_else(|_| env::var("PROVIDER"))
        .map(|v| ProviderKind::parse(&v))
        .unwrap_or(ProviderKind::DeepSeek);

    // Prefer provider-specific vars, then fall back across known names.
    alias_first("UPSTREAM_API_KEY", provider.preferred_api_key_vars());
    alias_first(
        "UPSTREAM_API_KEY",
        &[
            "DEEPSEEK_API_KEY",
            "MOONSHOT_API_KEY",
            "KIMI_API_KEY",
            "UPSTREAM_API_KEY",
        ],
    );

    alias_first("UPSTREAM_BASE_URL", provider.preferred_base_url_vars());
    alias_first(
        "UPSTREAM_BASE_URL",
        &[
            "DEEPSEEK_BASE_URL",
            "MOONSHOT_BASE_URL",
            "KIMI_BASE_URL",
            "MOONSHOT_API_BASE",
        ],
    );

    alias_first("UPSTREAM_MODEL", provider.preferred_model_vars());
    alias_first(
        "UPSTREAM_MODEL",
        &["DEEPSEEK_MODEL", "MOONSHOT_MODEL", "KIMI_MODEL"],
    );

    set_if_missing("UPSTREAM_BASE_URL", provider.default_base_url());
    set_if_missing("UPSTREAM_MODEL", provider.default_model());
}

fn alias_first(target: &str, sources: &[&str]) {
    if env::var_os(target).is_some() {
        return;
    }
    for source in sources {
        if *source == target {
            continue;
        }
        if let Ok(value) = env::var(source)
            && !value.is_empty()
        {
            // SAFETY: called once at process start before other threads spawn.
            unsafe { env::set_var(target, value) };
            return;
        }
    }
}

/// Extract provider route slug from a bridge `base_url` path (e.g. `/kimi/v1`).
fn bridge_route_from_url(base_url: &str) -> Option<&str> {
    let trimmed = base_url.trim_end_matches('/');
    let path = trimmed
        .split("://")
        .nth(1)
        .and_then(|rest| rest.find('/').map(|idx| &rest[idx..]))?;
    let mut segments: Vec<&str> = path.split('/').filter(|seg| !seg.is_empty()).collect();
    if segments.last() == Some(&"v1") {
        segments.pop();
    }
    match segments.as_slice() {
        [slug] if ProviderKind::from_route(slug).is_some() => Some(*slug),
        _ => None,
    }
}

fn set_if_missing(key: &str, value: &str) {
    if env::var_os(key).is_none() {
        // SAFETY: called once at process start before other threads spawn.
        unsafe { env::set_var(key, value) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_provider_aliases() {
        assert_eq!(ProviderKind::parse("kimi"), ProviderKind::Kimi);
        assert_eq!(ProviderKind::parse("kimi-for-coding"), ProviderKind::Kimi);
        assert_eq!(ProviderKind::parse("kimi-code"), ProviderKind::Kimi);
        assert_eq!(ProviderKind::parse("deepseek"), ProviderKind::DeepSeek);
    }

    #[test]
    fn detects_provider_from_url() {
        assert_eq!(
            ProviderKind::from_base_url("https://api.kimi.com/coding/v1"),
            ProviderKind::Kimi
        );
        assert_eq!(
            ProviderKind::from_base_url("https://api.moonshot.ai/v1"),
            ProviderKind::Kimi
        );
        assert_eq!(
            ProviderKind::from_base_url("https://api.deepseek.com/v1"),
            ProviderKind::DeepSeek
        );
        assert_eq!(
            ProviderKind::from_base_url("http://127.0.0.1:11435/kimi/v1"),
            ProviderKind::Kimi
        );
        assert_eq!(
            ProviderKind::from_base_url("http://127.0.0.1:11435/deepseek/v1"),
            ProviderKind::DeepSeek
        );
    }

    #[test]
    fn route_slug_and_codex_name() {
        assert_eq!(ProviderKind::DeepSeek.route_slug(), "deepseek");
        assert_eq!(
            ProviderKind::codex_provider_name("kimi"),
            "crabbridge-kimi"
        );
    }

    #[test]
    fn kimi_defaults_use_coding_endpoint() {
        assert_eq!(
            ProviderKind::Kimi.default_base_url(),
            "https://api.kimi.com/coding/v1"
        );
        assert_eq!(ProviderKind::Kimi.default_model(), "kimi-for-coding");
        assert_eq!(ProviderKind::Kimi.codex_env_key(), "KIMI_API_KEY");
        assert_eq!(ProviderKind::Kimi.known_models(), &["kimi-for-coding"]);
    }
}
