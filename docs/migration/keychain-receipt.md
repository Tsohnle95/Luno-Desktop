# R01 Keychain Migration Receipt

- Baseline: `4e251ef624c390e4d7e7ade4c2b5dc1fdcc284ec`
- Reviewed implementation commits: `755ca8959bb6eeb7ec15ca7b4198576520ce68f0`, `ecb5c921f27bb34e6a1934f6d14172ab5140cd67`, `aa418f6db5aefa8f6951fd0a45c19c15e1864cde`, `74ad11d`, `4db83c0`, `c23a4ef`, `d453ac2`
- Final reviewed HEAD: `c8f565a`
- Adapter: macOS Security.framework generic-password item, service `com.ty.luno`, account `lastfm-key`.
- Startup: `main.rs` now uses `MacOsKeychain`; `NoopKeychain` and plaintext fallback are removed.
- Safety: object configs redact `lastfm_key` without modifying legacy input; malformed/non-object configs are omitted from backups with a reason marker because redaction cannot be proven; invalid/incomplete version records are quarantined and restored on failure; preview entries are fully validated before reversible quarantine, with rollback on cleanup failure; Keychain migration is idempotent only for an equal existing value and reports conflicts without replacing user data; interactive adapter checks use unique per-run namespaces and remove only their own item.
- Automated checks: unavailable Rust/Cargo toolchain in this environment (`rustc: command not found`) is **INCONCLUSIVE**, not PASS.
- Real Keychain check: ignored by default because it requires interactive login-Keychain access; unavailable/denied access is **INCONCLUSIVE**, not PASS.
- Review note: adapter errors contain only operation/status information and never secret material. Included commits: `755ca89`, `ecb5c92`, `aa418f6`, `74ad11d`, `4db83c0`, `c23a4ef`, `d453ac2`, `1bf09d4`, `b67a33f`, `7f073c1`, and `c8f565a`.
