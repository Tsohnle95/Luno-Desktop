# Clean-Install Receipt: Simulated Clean-Profile Install Mechanics (2026-08-16)

Receipt date: `2026-08-16`. Source HEAD: `3430bf3a`. This receipt documents
the install **mechanics** verified on this machine under a simulated clean
profile (restricted `PATH`, isolated `HOME`/`TMPDIR`, staged install
location), and provides the runbook for the real clean-machine install,
which remains a user action (sign-off prerequisite 2).

## Packaged Artifact (exact HEAD)

- Built: `CARGO_NET_OFFLINE=true npm run tauri -- build --bundles dmg`
  at `3430bf3a`, exit `0`.
- DMG: `src-tauri/target/release/bundle/dmg/Luno_0.1.0_aarch64.dmg`,
  106,810,893 bytes, SHA-256
  `917e5eec931e8a06c9ecd327a9e4941b4e296f6d29362d92665b5a847c6f002a`.

## Verified Mechanics

Fixture root `/private/tmp/luno-clean-install`:

| Step | Command | Result |
|---|---|---|
| Attach | `hdiutil attach -readonly -nobrowse -mountpoint …` | exit 0; `Apple_HFS`; contents `Applications`, `Luno.app` |
| Copy | `ditto <dmg>/Luno.app → <staging>/install/Luno.app` | exit 0 |
| Detach | `hdiutil detach …` | exit 0 |
| Launch | `env PATH=/usr/bin:/bin HOME=<fresh-fixture> TMPDIR=<fixture> <install>/Luno.app/Contents/MacOS/luno-desktop` | process alive after 10 s; no panic; single stderr line: bundle helper resolved at `…/install/Luno.app/Contents/Resources/bin/luno-download-helper` |
| Terminate | `kill -TERM` | clean exit within 2 s |

Notes, stated precisely:

- The staged install boots and resolves its bundled helper **without any
  development PATH**: production wiring needs no Node/Cargo/opencode tooling.
- An idle fresh-home boot writes nothing under the fixture home (boot is
  read-only for an empty home; settings persist only on user-driven change,
  which needs UI interaction and a real profile).
- Signature checks remain red, as expected without a signing identity:
  `codesign --verify --deep --strict` exit `1` (`code has no resources but
  signature indicates they must be present`), `spctl -a` exit `1` (same),
  `security find-identity -v -p codesigning` → `0 valid identities found`.
  This is sign-off prerequisite 1 and is not affected by these mechanics.

## Boundary

This is a **simulation on the development machine**: the real clean-machine
install (a macOS profile/machine without development tools) is still
unperformed and is required for release approval.

## Runbook For The Real Clean Machine

1. Copy `Luno_0.1.0_aarch64.dmg` (verify SHA-256 against the value above) to
   the clean machine; `hdiutil attach -nobrowse` (or double-click).
2. Drag `Luno.app` to `/Applications` (standard install) **or** a staged
   location for evaluation; first launch from the clean login session,
   default environment (do not pre-set `HOME`/`PATH`).
3. Confirm: window opens; library scan runs on the default empty library
   root; Settings shows the resolved library path; a download through the
   bundled helper lands an M4A under `Music/Unsorted` (see
   `packaged-download-helper-level.md` for the wiring this exercises).
4. Exercise rollback per `docs/migration/rollback.md` (Python/PyInstaller
   fallback boundary and legacy user data).
5. Record: machine/profile details, macOS version, all observable output,
   and any deviation. Do **not** sign, notarize, or distribute from this
   machine without the signing prerequisite.

No release approval is claimed by this receipt.