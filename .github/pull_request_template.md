## Summary

<!-- What changed and why -->

## Surfaces touched

Check all that apply; reviewers must cover every checked box, not only Rust.

- [ ] Rust (`crates/**`)
- [ ] Desktop / admin UI (JS / HTML / CSS)
- [ ] Scripts (bash / PowerShell / Python / githooks)
- [ ] Docs (`README.md`, `AGENT_SPEC.md`, rules)
- [ ] Docker / CI / packaging
- [ ] Config examples (`*.toml`, `tauri.conf.json`)

## Test plan

- [ ] `cargo fmt --check --all`
- [ ] `cargo clippy --workspace -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Manual / UI checks (if desktop or admin changed):
  - [ ] Settings / welcome flows
  - [ ] Bridge start/stop from tray
