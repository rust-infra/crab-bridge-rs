//! `crabridge-cli` command handlers (`setup`, `print-codex-config`).

use std::path::PathBuf;

use anyhow::{Context, Result};
use reqwest::Client;

use crate::cli_opts::{Commands, CrabridgeCli, PrintCodexConfigArgs, SetupArgs};
use crate::codex_config::print_codex_config;
use crate::setup::{self, SetupOptions, print_setup_summary};
use crabbridge_core::config::{
    default_config_write_path, explicit_config_from_cli, load_config_file, resolve_api_key,
    validate_upstream_url,
};
use crabbridge_core::provider::ProviderKind;

/// Run `crabridge-cli` after Clap parsing.
pub async fn run(cli: CrabridgeCli) -> Result<()> {
    let config_path = explicit_config_from_cli(Some(cli.config));
    match cli.command {
        Commands::PrintCodexConfig(args) => run_print_codex_config(args).await,
        Commands::Setup(args) => run_setup(args, config_path).await,
    }
}

async fn run_print_codex_config(
    PrintCodexConfigArgs {
        api_key,
        base_url,
        model,
        bind_addr,
        provider,
        all_providers,
        providers,
    }: PrintCodexConfigArgs,
) -> Result<()> {
    let client = Client::new();
    let slugs = ProviderKind::resolve_setup_slugs(all_providers, providers.as_deref(), &provider)?;

    if slugs.len() > 1 || all_providers || providers.is_some() {
        for slug in &slugs {
            let kind = ProviderKind::from_route(slug).unwrap_or(ProviderKind::Custom);
            let upstream = validate_upstream_url(
                &std::env::var(format!("CRABRIDGE_{}_BASE_URL", slug.to_ascii_uppercase()))
                    .unwrap_or_else(|_| kind.default_base_url().to_string()),
            )?;
            let key = std::env::var(format!("CRABRIDGE_{}_API_KEY", slug.to_ascii_uppercase()))
                .or_else(|_| std::env::var("UPSTREAM_API_KEY"))
                .unwrap_or_else(|_| api_key.clone());
            let model_name =
                std::env::var(format!("CRABRIDGE_{}_MODEL", slug.to_ascii_uppercase()))
                    .unwrap_or_else(|_| kind.default_model().to_string());
            print_codex_config(
                &client,
                &upstream,
                &key,
                &ProviderKind::codex_provider_name(slug),
                &model_name,
                &bind_addr,
                slug,
            )
            .await;
        }
    } else {
        let kind = ProviderKind::parse(&provider);
        let upstream = validate_upstream_url(&base_url)?;
        print_codex_config(
            &client,
            &upstream,
            &api_key,
            &ProviderKind::codex_provider_name(kind.route_slug()),
            &model,
            &bind_addr,
            kind.route_slug(),
        )
        .await;
    }

    eprintln!("// CrabBridge bind address: http://{bind_addr}");
    Ok(())
}

async fn run_setup(
    SetupArgs {
        provider,
        api_key,
        base_url,
        model,
        bind_addr,
        codex_only,
        force_config,
        docker,
        all_providers,
        providers,
    }: SetupArgs,
    config: Option<PathBuf>,
) -> Result<()> {
    let config = default_config_write_path(config);
    let slugs = if docker && providers.is_none() && !all_providers && config.is_file() {
        let cfg = load_config_file(&config)
            .with_context(|| format!("failed to read {}", config.display()))?;
        if cfg.providers.is_empty() {
            vec![ProviderKind::parse(&provider).route_slug().to_string()]
        } else {
            cfg.providers.keys().cloned().collect()
        }
    } else {
        ProviderKind::resolve_setup_slugs(all_providers, providers.as_deref(), &provider)?
    };
    let is_multi = slugs.len() > 1;

    if docker {
        setup::run_setup_check(setup::SetupCheckOptions {
            provider_slugs: slugs,
            api_key,
            bridge_config_path: config,
            bind_addr,
        })
        .await?;
        return Ok(());
    }

    for (idx, slug) in slugs.iter().enumerate() {
        let provider_kind = ProviderKind::from_route(slug).unwrap_or(ProviderKind::Custom);
        let resolved_key = resolve_api_key(slug, provider_kind, api_key.clone());
        let result = setup::run_setup(SetupOptions {
            provider: provider_kind,
            provider_slug: slug.clone(),
            api_key: resolved_key,
            base_url: base_url.clone(),
            model: model.clone(),
            bind_addr,
            write_bridge_config: !codex_only && !is_multi,
            write_multi_bridge_config: !codex_only && is_multi && idx == 0,
            multi_provider_slugs: if is_multi { Some(slugs.clone()) } else { None },
            bridge_config_path: config.clone(),
            force_bridge_config: force_config,
            set_active_codex_provider: !is_multi || idx + 1 == slugs.len(),
        })
        .await?;
        print_setup_summary(&result);
    }

    Ok(())
}
