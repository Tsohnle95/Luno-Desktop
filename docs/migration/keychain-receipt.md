# R01 Keychain Migration Receipt

- Baseline: `4e251ef624c390e4d7e7ade4c2b5dc1fdcc284ec`
- Final HEAD: `PENDING_COMMIT_SHA`
- Adapter: macOS Security.framework generic-password item, service `com.ty.luno`, account `lastfm-key`.
- Startup: `main.rs` now uses `MacOsKeychain`; `NoopKeychain` and plaintext fallback are removed.
- Safety: the injected Keychain boundary runs before target settings/version creation; failure leaves both absent.
- Automated checks: unavailable Rust/Cargo toolchain in this environment (`rustc: command not found`) is **INCONCLUSIVE**, not PASS.
- Real Keychain check: ignored by default because it requires interactive login-Keychain access; unavailable/denied access is **INCONCLUSIVE**, not PASS.
- Review note: adapter errors contain only operation/status information and never secret material.
