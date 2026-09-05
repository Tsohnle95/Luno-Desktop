# Luno Desktop Verification

This guide covers the active React/TypeScript/Tauri/Rust application. Historical
Python differential receipts remain in `docs/archive/python-to-tauri/` and on
the `legacy-python-desktop-final` archive tag.

## Automated gates

Run from the repository root:

```bash
npm ci
npm run typecheck
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

For a production package:

```bash
npx tauri build
./scripts/verify-desktop-bundle.sh \
  "src-tauri/target/release/bundle/macos/Luno Desktop.app"
```

The bundle verifier checks the managed binary manifest, third-party notices,
executable bits, checksums, architectures, versions, and behavior with the
developer PATH unavailable.

## Compatibility gates

- Keep `com.ty.luno` unchanged.
- Exercise first-run migration against existing user-data fixtures.
- Run Desktop transfer export/import tests against
  `contracts/library-export/v1/`.
- Run the cross-repository Desktop → Mobile → Desktop transfer round trip using
  the equivalent Mobile codec and real v1 fixtures.
- Review every release path for `Luno Desktop.app` and the generated DMG.

Missing runtime, visual, accessibility, signing, notarization, device, or
manual-journey evidence is inconclusive; it must not be reported as a pass.
