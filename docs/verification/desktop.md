# Luno Desktop Verification

Document role: canonical owner for operational procedures — setup, build,
verification, smoke testing, CI, supported platforms, and operational
debugging. Release packaging truth lives in `docs/release/desktop.md`.

This guide covers the active React/TypeScript/Tauri/Rust application. Historical
Python differential receipts remain in `docs/archive/python-to-tauri/` and on
the `legacy-python-desktop-final` archive tag.

## Toolchain

- Node 22, pinned in `.node-version`; CI derives its runtime from that
  file. Rust stable, pinned in `rust-toolchain.toml`.
- Dependencies resolve from lockfiles: `npm ci` (never plain `npm
  install` in CI or verification), `Cargo.lock` for Rust.
- Validation under any other toolchain is exploratory, not authoritative.
  If the active toolchain does not match the pins, re-run the final gate
  under the supported one before claiming a pass.

## Automated gates

Run from the repository root:

```bash
npm ci
npm run verify      # typecheck + vitest + docs check
cargo test --manifest-path src-tauri/Cargo.toml
npm run build
```

`vitest.config.ts` is the authority on which frontend tests execute
(`src/test/**` only). For a production package:

```bash
npx tauri build
./scripts/verify-desktop-bundle.sh \
  "src-tauri/target/release/bundle/macos/Luno Desktop.app"
```

The DMG bundler deletes the intermediate `.app` after packaging, so to install
the release app into `/Applications` use `npx tauri build --bundles app` (or
`scripts/install-release-app.sh`, which builds, quits the running copy,
installs, verifies, and relaunches). The bundle verifier checks the managed binary manifest, third-party notices,
executable bits, checksums, architectures, versions, and behavior with the
developer PATH unavailable.

`./build-test.command` is the local end-to-end harness (Rust tests,
typecheck, frontend tests, production build, app launch, best-effort DMG)
for a macOS GUI session. `./dev.command` launches development.

For daily use without a terminal window, `npm run launch` starts the same
development server detached (PID and logs live under the system temp
directory; `scripts/luno-tray.sh status|logs|stop` manages it).
`scripts/install-dock-icon.sh` builds a double-clickable `Luno Dev` app in
`~/Applications` from `scripts/Luno-Dev.applescript` that fires the launcher
in the background. The installer reveals it in Finder for manual docking; it
never modifies the user's Dock. Each click runs `luno-tray.sh restart`, so it
stops any running dev server and starts a fresh debug build; while it runs,
frontend edits hot-reload and Rust edits rebuild incrementally. Starting dev
quits the installed release app first: both share the single-instance port
(48713) and config, so they cannot run concurrently.
The installed `.app` is a generated local artifact holding this machine's
repository path — never commit it, and re-run the installer after moving the
repository.

The dev launcher runs the debug binary `src-tauri/target/debug/luno-desktop`;
the Dock-pinned `/Applications/Luno Desktop.app` is a RELEASE bundle and never
rebuilds — refresh it with `scripts/install-release-app.sh`.
The `.command` and Dock launchers resolve the Node major from `.node-version`
before starting Tauri, including the local `~/.local/share/orbit-toolchains`
install layout, so a GUI's reduced `PATH` does not silently skip the dev build.

## Known verification gaps (do not treat as passes)

- Root `tests/**/*.rs` are not wired into Cargo (no `[[test]]` targets;
  integration tests would live under `src-tauri/tests/`, which is empty).
  Executable Rust proof is the in-tree unit suites
  (`src-tauri/src/gate*_tests.rs`, `state.rs` tests) run by `cargo test`.
- Root `tests/contracts/contracts.test.ts` and
  `tests/logging/logs.test.ts` are assert-scripts outside the vitest
  `include` list; they do not run in `npm test`.
- Missing runtime, visual, accessibility, signing, notarization, device, or
  manual-journey evidence is inconclusive; it must not be reported as a pass.

## Compatibility gates

- Keep `com.ty.luno` unchanged.
- Exercise first-run migration against existing user-data fixtures.
- Run Desktop transfer export/import tests against
  `contracts/library-export/v1/`.
- Run the cross-repository Desktop → Mobile → Desktop transfer round trip using
  the equivalent Mobile codec and real v1 fixtures.
- Review every release path for `Luno Desktop.app` and the generated DMG.

## CI

- `desktop-tauri-foundation.yml` (push/PR touching `src/**`,
  `src-tauri/**`, configs): clean checkout, Node from `.node-version`,
  `npm ci`, docs check, typecheck, frontend build, `cargo test`.
- `desktop-tauri-release.yml` (manual dispatch): reviewed binaries,
  provisioning, typecheck, build, Rust tests, signed DMG, bundle verify,
  publish. Requires Apple signing secrets from the release owner.
- macOS 13+ is the only supported release target. Platform claims without
  a CI or smoke path exercising them must not be documented as supported.
