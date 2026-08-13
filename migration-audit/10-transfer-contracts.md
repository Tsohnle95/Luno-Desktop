# Transfer Contract Audit

## Desktop Library Export/Import Contract
- Status: implemented
- Evidence:
  - `library_transfer.py:22-35` defines `luno.library.export` v1, bounded sizes, scopes, provider IDs, and duration tolerance.
  - `library_transfer.py:73-210` validates the complete manifest and rejects unknown fields, duplicate references, invalid references, unsafe virtual playlists, and unsupported providers.
  - `contracts/library-export/v1/README.md:3-21` defines the metadata-only, additive, no-path contract.
  - `contracts/library-export/v1/schema.json:1-53` freezes the v1 wire shape and provider enum.
- User behavior: Desktop users can export selected tracks, one playlist, or the library as JSON, and import a JSON manifest. The file is a portable reference manifest, not an audio backup. Import matches local audio, copies matches into playlist folders, and offers to download unresolved tracks.
- State and data flow:
  - `music_player.py:3655-3695` snapshots `_library` and selected paths, performs metadata probing and manifest writing on a daemon worker, then returns to Tk through `after(0, ...)`.
  - `library_transfer.py:239-260` derives title/artist from the filename and prefers Mutagen title, artist, album, and duration metadata; probe failures degrade to filename metadata and zero duration.
  - `library_transfer.py:290-369` creates stable refs from provider identity when supplied, otherwise from metadata, preserves playlist track order, and emits unassigned refs.
  - `music_player.py:3723-3749` validates with `load_manifest`, plans and applies the import on a worker, then reports results on the UI thread.
- Persistence and compatibility: The shared v1 schema requires `format`, version, scope, UTC-looking `created_at`, tracks, playlists, and unassigned refs. Track records contain title, artist, album, duration, optional `{provider,id}`, and a boolean ambiguity flag. `library_transfer.py:303` explicitly discards the supplied root, and `library_transfer.py:231-236` writes normalized JSON with ASCII escaping. No local path, audio bytes, secret, queue, favorites, or history field is accepted. Favorites are also explicitly excluded from the desktop model per `project_brain.md:246-253`.
- Async, lifecycle, and failure behavior: Export/import are serialized by `_transfer_running` (`music_player.py:3657-3659,3725-3727`) and heavy probing, file reads/copies, and validation stay off Tk. Errors are marshalled to `_finish_transfer_error` (`music_player.py:3690-3693,3744-3754`). `load_manifest` bounds input to 10 MiB and converts malformed JSON, encoding, and filesystem failures to `ManifestError` (`library_transfer.py:218-228`). Copy errors are accumulated without deleting existing files (`library_transfer.py:430-486`).
- UI states and visual requirements:
  - Playlist actions expose `Export JSON` and `Import JSON` (`views/library.py:539-570`), and Settings exposes `EXPORT LIBRARY` and `IMPORT JSON` (`views/settings.py:127-161`). Selected-track export is also wired through Library/Recent context actions (`music_player.py:3706-3711`; `views/library.py:46`; `views/recent.py:32`).
  - Export shows `Preparing export…`, then a completion dialog stating that audio and paths are excluded (`music_player.py:3678-3704`). Import shows `Checking import…`, reports copied/missing tracks, logs copy errors, and refreshes the library (`music_player.py:3736-3737,3756-3782`).
  - Missing-track handoff asks once whether to download and displays the number of metadata-only tracks requiring ambiguity confirmation (`music_player.py:3763-3772`). This is a global yes/no decision, not a per-track confirmation workflow.
- Migration implementation requirements: Preserve strict unknown-field rejection and all semantic validation beyond JSON Schema, including bounded arrays/strings, unique refs, valid playlist refs, no `All Music` import playlist, path-safe names, provider allowlist, and the required ambiguity flag when `source` is absent. Preserve additive copy behavior, collision suffixing, same-path skipping, and no deletion. Keep export/import off the UI thread and keep unresolved tracks visible/reported.
- Verification requirements: Retain the shared valid and invalid fixtures and add desktop coverage for every supported provider, source-ID matching, duration tolerance/mismatch, duplicate local candidates, playlist collision behavior, copy failures, malformed/oversized input, and missing-track handoff. Run `python -m unittest test_library_transfer` or the configured `python -m pytest` desktop gate when dependencies are installed.
- Confidence: high for codec and merge behavior; medium for end-to-end provider behavior and UI confirmation semantics.
- Open questions: The desktop controller never passes `source_by_path` to `build_manifest` or `plan_import` (`music_player.py:3682-3688,3741-3743`), and no desktop source indexes provider IDs. Thus provider IDs are supported by the library codec but are not produced by normal desktop UI exports and cannot match existing desktop files through the controller. The downloader handoff only converts YouTube IDs to URLs (`music_player.py:3787-3797`); SoundCloud, Deezer, and MusicBrainz IDs fall back to artist/title search. Decide whether this is acceptable or whether desktop metadata/source identity persistence must be added.

## Contract Fixtures And Desktop Tests
- Status: implemented
- Evidence:
  - `test_contract_fixtures.py:9-28` accepts `valid.json` and rejects `invalid-unknown-field.json`.
  - `test_library_transfer.py:16-90` covers path-free/order-preserving export, selected-track ambiguity, unknown fields/path traversal, metadata matching, additive copy, and non-deletion.
  - `contracts/library-export/v1/valid.json:1-30` includes one YouTube source track and one metadata-only ambiguous track.
  - `contracts/library-export/v1/invalid-unknown-field.json:1-10` includes a forbidden `secret` field.
- User behavior: The checked-in fixtures define the compatibility baseline for consumers, while desktop unit tests exercise the implementation-specific merge behavior.
- State and data flow: Fixture validation normalizes the input through the same `validate_manifest` function used by file loading and import planning.
- Persistence and compatibility: The tests prove the no-path and additive guarantees for representative cases, but do not constitute complete provider or ambiguity coverage.
- Async, lifecycle, and failure behavior: No UI/thread integration test is present for export/import; those paths are source-evidenced only.
- UI states and visual requirements: No automated Tk UI test verifies entrypoint availability, dialogs, or refresh behavior.
- Migration implementation requirements: Treat fixture contents as frozen v1 inputs. Do not add desktop-only fields to the wire format.
- Verification requirements: In this workspace, `.venv/bin/python -m unittest test_library_transfer` passed 4 tests. `.venv/bin/python -m unittest test_library_transfer test_contract_fixtures` ran those 4 tests but failed to import the fixture module because `pytest` is not installed; the pytest gate is therefore inconclusive, not passing.
- Confidence: high for the tested cases; low for untested provider and UI integration cases.
- Open questions: Whether the CI environment installs pytest is established by `.github/workflows/desktop.yml`, but was not reproduced in this local virtualenv.

## Audit Receipt
- Files inspected: `migration-audit/README.md`, `migration-audit/00-repository-census.md`, `project_brain.md`, `README.md`, `library_transfer.py`, `music_player.py`, `views/library.py`, `views/settings.py`, `views/recent.py`, `downloader.py`, `contracts/library-export/v1/README.md`, `contracts/library-export/v1/schema.json`.
- Tests/fixtures inspected: `test_library_transfer.py`, `test_contract_fixtures.py`, `contracts/library-export/v1/valid.json`, `contracts/library-export/v1/invalid-unknown-field.json`.
- Claims requiring review: Provider-ID production/indexing is absent from desktop controller wiring; non-YouTube provider IDs do not receive provider-specific download handoff; ambiguity confirmation is global rather than per track.
- Coverage gaps: No pytest package in the local environment; no desktop Tk integration tests; no provider-ID fixtures/tests; no duration mismatch, duplicate-candidate, copy-error, or download-completion assertions for transfer.
- Status: complete
