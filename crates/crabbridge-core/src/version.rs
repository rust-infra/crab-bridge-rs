//! Compile-time release version (git tag / CI), baked in via `build.rs`.

/// Version string embedded at compile time.
///
/// Preference: `CRABBRIDGE_VERSION` → GitHub tag → `git describe` → Cargo package version.
/// A leading `v` is stripped (so tag `v1.0.3` becomes `1.0.3`).
pub const VERSION: &str = env!("CRABBRIDGE_GIT_VERSION");

#[cfg(test)]
mod tests {
    use super::VERSION;

    #[test]
    fn version_is_non_empty() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn version_has_no_leading_v() {
        assert!(!VERSION.starts_with('v'));
    }
}
