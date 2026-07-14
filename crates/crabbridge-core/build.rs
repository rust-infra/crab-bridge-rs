//! Embed release version into `crabbridge-core` at compile time.
//!
//! Resolution order:
//! 1. `CRABBRIDGE_VERSION` (CI / packaging scripts strip a leading `v`)
//! 2. GitHub Actions tag (`GITHUB_REF_TYPE=tag` + `GITHUB_REF_NAME`)
//! 3. `git describe --tags --always --dirty`
//! 4. `CARGO_PKG_VERSION` from Cargo.toml

use std::process::Command;

fn normalize_version(raw: &str) -> String {
    let trimmed = raw.trim();
    trimmed.strip_prefix('v').unwrap_or(trimmed).to_string()
}

fn version_from_github() -> Option<String> {
    let ref_type = std::env::var("GITHUB_REF_TYPE").ok()?;
    if ref_type != "tag" {
        return None;
    }
    let name = std::env::var("GITHUB_REF_NAME").ok()?;
    let normalized = normalize_version(&name);
    (!normalized.is_empty()).then_some(normalized)
}

fn version_from_git() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let described = String::from_utf8(output.stdout).ok()?;
    let normalized = normalize_version(&described);
    (!normalized.is_empty()).then_some(normalized)
}

fn main() {
    println!("cargo:rerun-if-env-changed=CRABBRIDGE_VERSION");
    println!("cargo:rerun-if-env-changed=GITHUB_REF_TYPE");
    println!("cargo:rerun-if-env-changed=GITHUB_REF_NAME");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/tags");

    let version = std::env::var("CRABBRIDGE_VERSION")
        .ok()
        .map(|v| normalize_version(&v))
        .filter(|v| !v.is_empty())
        .or_else(version_from_github)
        .or_else(version_from_git)
        .unwrap_or_else(|| {
            std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string())
        });

    println!("cargo:rustc-env=CRABBRIDGE_GIT_VERSION={version}");
}
