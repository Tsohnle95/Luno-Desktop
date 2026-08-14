# R01 Keychain Migration Receipt

- Baseline: `4e251ef624c390e4d7e7ade4c2b5dc1fdcc284ec`
- Reviewed implementation commits: `755ca8959bb6eeb7ec15ca7b4198576520ce68f0`, `ecb5c921f27bb34e6a1934f6d14172ab5140cd67`, `aa418f6db5aefa8f6951fd0a45c19c15e1864cde`, `74ad11d`, `4db83c0`, `c23a4ef`, `d453ac2`, `5ca979d`, `08767e1`
- Final reviewed implementation HEAD: `08767e113e7bb5cd3f5b2a6dac3273995dc867c3`
- Included receipt commit: updated in the following receipt commit
- Adapter: macOS Security.framework generic-password item, service `com.ty.luno`, account `lastfm-key`.
- Startup: `main.rs` now uses `MacOsKeychain`; `NoopKeychain` and plaintext fallback are removed.
- Safety: object configs redact `lastfm_key` without modifying legacy input; malformed/non-object configs are omitted from backups with a reason marker because redaction cannot be proven; invalid/incomplete version records are quarantined and restored on every pre-commit failure, with aggregated restoration errors; absent preview roots are a no-op, while dangling/symlink roots are rejected before existence checks; preview entries are fully validated before reversible quarantine, with migration rollback still attempted when preview restoration fails; after durable commit, recovery-quarantine disposal failures return a successful receipt with a warning rather than a false migration failure; Keychain migration is idempotent only for an equal existing value and reports conflicts without replacing user data; interactive adapter checks use unique per-run namespaces and remove only their own item.
- Automated checks: `cargo fmt --all -- --check` and `cargo test --manifest-path src-tauri/Cargo.toml --test migration_first_run` were unavailable because `cargo`/`rustc` are not installed; **INCONCLUSIVE**, not PASS.
- Real Keychain check: ignored by default because it requires interactive login-Keychain access; unavailable/denied access is **INCONCLUSIVE**, not PASS.
- Review note: adapter errors contain only operation/status information and never secret material. Focused R01 fixes: `447d595`, `5ca979d`, `08767e1`.
