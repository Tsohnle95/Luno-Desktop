# Luno Repository Separation & Archival Implementation Plan - 

**Repository:** `Tsohnle95/musicPlayer`  
**Current default branch:** `main`  
**Target active repositories:**

- `Tsohnle95/Luno-Desktop`
- `Tsohnle95/Luno-Mobile`

**Legacy desktop disposition:** preserve the final Python/Tkinter/Pygame desktop implementation as an immutable archive branch/tag and optional source archive for reference.

---

# 1. Purpose

The current `musicPlayer` repository contains three distinct application generations/codebases:

1. **Current desktop application**
   - React
   - TypeScript
   - Vite
   - Tauri 2
   - Rust

2. **Mobile application**
   - Android
   - Kotlin
   - Jetpack Compose
   - Media3
   - Room
   - WorkManager
   - Gradle

3. **Legacy desktop application**
   - Python
   - Tkinter
   - Pygame
   - PyInstaller
   - yt-dlp / FFmpeg integration
   - macOS-specific integrations

The repository is now unnecessarily large and confusing because unrelated applications and migration-era material live in the same working tree.

This task is **repository separation and archival only**.

It must not change application behavior.

---

# 2. Primary Goals

The finished state must be:

## 2.1 Luno Desktop

The existing `musicPlayer` GitHub repository should become the active desktop repository and be renamed:

```text
Tsohnle95/musicPlayer
        ↓
Tsohnle95/Luno-Desktop
```

Its active `main` branch should contain only the current React/TypeScript/Tauri/Rust desktop application and the files required to develop, test, migrate, package, sign, and release that application.

Human-facing product name:

```text
Luno Desktop
```

Preferred GitHub repository slug:

```text
Luno-Desktop
```

The current application identifier must remain:

```text
com.ty.luno
```

unless an independently justified migration is performed in a future task.

---

## 2.2 Luno Mobile

The Android application should be extracted into a new repository:

```text
Tsohnle95/Luno-Mobile
```

Human-facing product name:

```text
Luno Mobile
```

Preferred GitHub repository slug:

```text
Luno-Mobile
```

The Android application ID must remain:

```text
com.luno.mobile
```

The extracted repository must be a normal standalone Android project rooted at the repository root, not nested inside another `mobile-app/` directory.

---

## 2.3 Legacy Python Desktop

The final Python desktop application must be preserved for historical reference and rollback/debugging purposes.

Preferred preservation strategy:

```text
branch: archive/python-desktop-final
tag:    legacy-python-desktop-final
```

The archive must be created **before** Python desktop files are removed from active `main`.

Optionally also create:

```text
legacy-python-desktop-final.tar.gz
```

or:

```text
legacy-python-desktop-final.zip
```

and attach it to a GitHub release or otherwise store it as an immutable source snapshot.

The active `main` branch should **not** retain a giant `legacy-desktop/` directory.

---

# 3. Critical Rule: This Is Not a Refactor

During repository separation:

> **Do not redesign, modernize, clean up, rename internal APIs, reorganize modules, rewrite services, replace dependencies, or alter behavior unless the change is strictly required because a file moved to a different repository/root.**

Allowed changes during the split:

- path updates caused by extraction;
- CI working-directory changes;
- build script path changes;
- README/documentation updates;
- repository/product display naming;
- release artifact naming;
- copying/versioning shared interoperability fixtures;
- fixing a pre-existing dependency issue only if necessary to prove that a fresh clone builds.

Not allowed during the split:

- feature changes;
- UI changes;
- data model rewrites;
- database migrations unrelated to the split;
- new architecture;
- replacing Gradle/Tauri/Vite;
- changing package identifiers;
- replacing the transfer format;
- changing playback behavior;
- changing download behavior;
- changing library semantics;
- changing migration behavior;
- removing compatibility readers simply because they are named `legacy`.

Treat the operation as **repository surgery**.

---

# 4. Non-Negotiable Compatibility Invariants

The split is successful only if all of these remain true.

## 4.1 Desktop invariants

Luno Desktop must preserve:

- existing React UI behavior;
- existing Tauri IPC behavior;
- existing Rust domain behavior;
- library scanning and mutation behavior;
- playback and queue behavior;
- media-key behavior;
- download behavior;
- discovery behavior;
- preview behavior;
- artwork behavior;
- video behavior;
- settings behavior;
- current first-run migration behavior;
- compatibility with existing Luno user data;
- transfer import/export behavior;
- current release packaging behavior.

## 4.2 Mobile invariants

Luno Mobile must preserve:

- current package/application identity;
- existing Room database compatibility;
- existing Android storage/library behavior;
- playback behavior;
- downloads;
- discovery/recommendation behavior;
- background work;
- release signing;
- transfer import/export behavior;
- current minimum/target SDK behavior;
- current NewPipeExtractor behavior;
- current tests.

## 4.3 Cross-platform invariant

The language-neutral library-transfer format must remain compatible between Desktop and Mobile.

A Desktop → Mobile → Desktop round-trip verification is required before declaring the migration complete.

---

# 5. Current Repository Boundaries

The agent must inspect the repository again before making destructive changes, but the current high-level boundaries are already clear.

## 5.1 Current Tauri desktop

Primary current-desktop files/directories include:

```text
src/
src-tauri/
package.json
package-lock.json
tsconfig.json
vitest.config.ts
dev.command
build-test.command
release/
release-input/
scripts/
.github/workflows/desktop-tauri-foundation.yml
.github/workflows/desktop-tauri-release.yml
```

The root Node package is already named:

```text
luno-desktop
```

The Rust crate is already named:

```text
luno-desktop
```

The Tauri product currently uses human-facing `Luno` branding and should become `Luno Desktop`.

---

## 5.2 Current mobile app

The Android project currently lives under:

```text
mobile-app/
```

Important contents include:

```text
mobile-app/app/
mobile-app/gradle/
mobile-app/gradlew
mobile-app/gradlew.bat
mobile-app/build.gradle.kts
mobile-app/settings.gradle.kts
mobile-app/gradle.properties
mobile-app/ANDROID_RELEASES.md
mobile-app/project_brain.md
```

The app currently uses:

```text
namespace     = com.luno.mobile
applicationId = com.luno.mobile
```

These identifiers must not be changed as part of this task.

---

## 5.3 Current legacy Python desktop

The old desktop implementation is spread across root-level Python files and support directories rather than one isolated folder.

Known legacy candidates include:

```text
music_player.py
engine.py
downloader.py
discovery.py
previews.py
rec_artwork.py
video_preview.py
library_tools.py
library_transfer.py
source_lookup.py
theme.py
utils.py
widgets.py
views/
requirements.txt
pytest.ini
Luno.spec
icon.icns
build.command
```

There are also Python tests such as:

```text
test_art_cache.py
test_contract_fixtures.py
test_discover_mode.py
test_dl.py
test_downloader_sync.py
test_engine_playback.py
test_focus.py
test_label_ph.py
test_library_transfer.py
test_media_keys.py
test_tkvideo.py
test_video_preview.py
tests/
```

Do **not** assume this list is exhaustive.

Before deletion, produce a dependency inventory of the Python application and archive every file needed to:

- run it;
- test it;
- package it;
- understand its behavior.

---

# 6. Important Distinction: Legacy Python vs Tauri Compatibility Code

Do not confuse old Python source with Rust code that reads/migrates old Luno data.

Files such as these belong to the **new Tauri application** and must stay active:

```text
src-tauri/src/migration/first_run.rs
src-tauri/src/migration/keychain.rs
src-tauri/src/migration/legacy_readers.rs
src-tauri/src/persistence/legacy.rs
```

The presence of words like:

```text
legacy
migration
old
compatibility
```

does **not** mean a file belongs in the Python archive.

If the current Rust desktop runtime uses it, keep it in Luno Desktop.

---

# 7. Shared Transfer Contract

The desktop and mobile applications intentionally share a language-neutral transfer format.

Current shared fixtures live under:

```text
contracts/library-export/v1/
```

The Android test configuration currently reaches outside `mobile-app/` to consume those fixtures.

That means `mobile-app/` cannot simply be copied to a new repository unchanged.

## Required approach

During the split, copy/version the transfer contract fixtures into both repositories.

Target:

```text
Luno-Desktop/
└── contracts/
    └── library-export/
        └── v1/
```

and:

```text
Luno-Mobile/
└── contracts/
    └── library-export/
        └── v1/
```

Update Luno Mobile's test-resource path so it points to the contract inside its own repository.

Do **not** introduce a third shared repository/package/submodule during this cleanup.

Duplicating a small versioned interoperability fixture set is preferable to creating new infrastructure while the split is happening.

A later task may centralize the contract if needed.

---

# 8. Pre-Split Safety Checkpoint

Before changing anything:

```bash
git status
```

The working tree must be clean.

Create an immutable checkpoint:

```bash
git switch main
git pull --ff-only
git tag pre-luno-repo-split
git branch archive/pre-luno-repo-split
```

Push both:

```bash
git push origin pre-luno-repo-split
git push origin archive/pre-luno-repo-split
```

Record:

```bash
git rev-parse HEAD
```

Place the commit SHA in the migration notes.

No destructive cleanup may happen before this checkpoint exists remotely.

---

# 9. Establish Baseline Verification Before Moving Files

The agent must establish whether all applications build/test **before** the split.

This matters because any existing failure must not later be blamed on repository separation.

---

## 9.1 Current Tauri desktop baseline

From the existing repository root:

```bash
npm ci
npm run typecheck
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

If a normal development launch is part of the existing workflow:

```bash
npm run tauri dev
```

If a full package is practical:

```bash
npx tauri build
```

Record pass/fail results.

---

## 9.2 Legacy Python baseline

Use the existing supported Python environment.

At minimum:

```bash
python -m pytest
```

Also verify the legacy build path if reasonably possible:

```bash
pyinstaller --clean --noconfirm Luno.spec
```

or use the existing helper:

```bash
./build.command
```

Record pass/fail results.

---

## 9.3 Android baseline

Perform this from a **fresh clone**, not only the existing developer working tree.

Expected commands include:

```bash
cd mobile-app
./gradlew testDebugUnitTest
./gradlew lintDebug
./gradlew assembleDebug
```

If a device/emulator is available:

```bash
./gradlew :app:installDebug
```

---

# 10. Critical Pre-Existing Android Dependency Check

Current Android settings expect:

```text
vendor/NewPipeExtractor
```

as an included build.

The repository also has an empty `.gitmodules` file, and the expected vendor directory may not be present in a fresh clone.

This is a **pre-existing hazard**.

Before splitting:

1. inspect `mobile-app/settings.gradle.kts`;
2. inspect all Gradle dependency substitution;
3. inspect `.gitmodules`;
4. inspect whether `vendor/NewPipeExtractor` is tracked;
5. inspect CI behavior;
6. perform a fresh clone build;
7. determine the intended canonical dependency mechanism.

Do not perform the repository split until the Android dependency arrangement is known.

The final `Luno-Mobile` repository must build from a fresh clone without relying on an untracked local directory.

If the correct source is a Git submodule, restore/configure it properly.

If it is meant to be vendored normally, ensure it is tracked normally.

If the project has since changed and the dependency is no longer needed, prove that before removing it.

Do not casually upgrade NewPipeExtractor during this task.

---

# 11. Phase 1 — Extract Luno Mobile

Mobile should be extracted before deleting it from the original repository.

## 11.1 Preserve mobile history

Preferred approach: use `git filter-repo`.

Work from a separate clone.

Example strategy:

```bash
git clone <current-musicPlayer-url> luno-mobile-extract
cd luno-mobile-extract
```

Create a safety branch locally.

Then filter to retain the mobile project plus its supporting files/history.

Conceptually retain:

```text
mobile-app/
build-mobile.command
.github/workflows/android-release.yml
contracts/library-export/v1/
```

Then move `mobile-app/` contents to repository root.

An example `git filter-repo` approach may look like:

```bash
git filter-repo \
  --path mobile-app/ \
  --path build-mobile.command \
  --path .github/workflows/android-release.yml \
  --path contracts/library-export/v1/ \
  --path-rename mobile-app/:
```

The agent must verify the result before pushing.

Do not blindly run this command if current repository history/paths differ.

---

## 11.2 Create the new GitHub repository

Create:

```text
Tsohnle95/Luno-Mobile
```

Prefer private visibility initially if the source repository is private.

Push the filtered history to it.

Ensure the default branch is:

```text
main
```

---

# 12. Target Luno Mobile Layout

After extraction, the project should be rooted naturally:

```text
Luno-Mobile/
├── .github/
│   └── workflows/
│       └── android-release.yml
├── app/
├── contracts/
│   └── library-export/
│       └── v1/
├── gradle/
├── gradlew
├── gradlew.bat
├── build.gradle.kts
├── settings.gradle.kts
├── gradle.properties
├── ANDROID_RELEASES.md
├── build-mobile.command
├── README.md
└── .gitignore
```

If `vendor/NewPipeExtractor` is legitimately required, include its canonical mechanism too.

---

# 13. Luno Mobile Path Repairs

After moving `mobile-app/` to repo root, only fix paths that became invalid because of the move.

## 13.1 Transfer fixture path

Current Android tests reference something equivalent to:

```kotlin
rootProject.file("../contracts/library-export/v1")
```

Update it to the new repo-root path, such as:

```kotlin
rootProject.file("contracts/library-export/v1")
```

or another correct root-relative equivalent.

Do not change fixture content during the split unless required to preserve existing behavior.

---

## 13.2 build-mobile.command

The current helper assumes an Android subdirectory.

After extraction, remove the `mobile-app` nesting assumption.

Target behavior:

- the script runs from repository root;
- Gradle wrapper is at repository root;
- output paths resolve correctly;
- package launch still uses `com.luno.mobile`.

Example conceptual change:

From:

```bash
ANDROID_DIR="$ROOT_DIR/mobile-app"
```

To:

```bash
ANDROID_DIR="$ROOT_DIR"
```

Then verify every derived path.

---

## 13.3 Android CI

The release workflow currently assumes:

```yaml
working-directory: mobile-app
```

Remove that assumption after extraction.

Likewise update artifact paths from:

```text
mobile-app/app/build/...
```

to:

```text
app/build/...
```

---

# 14. Luno Mobile Naming

Human-facing naming should become consistently:

```text
Luno Mobile
```

Repository:

```text
Luno-Mobile
```

Release artifact recommendation:

```text
Luno-Mobile-vMAJOR.MINOR.PATCH.apk
```

Update documentation and release labels accordingly.

Do **not** change:

```text
applicationId = com.luno.mobile
namespace     = com.luno.mobile
```

unless a future app-identity migration is intentionally designed.

---

# 15. Luno Mobile Secrets & Release Configuration

The current Android release workflow uses signing secrets.

Recreate the required secrets in `Luno-Mobile`, including the existing equivalents of:

```text
ANDROID_RELEASE_KEYSTORE_BASE64
ANDROID_RELEASE_STORE_PASSWORD
ANDROID_RELEASE_KEY_ALIAS
ANDROID_RELEASE_KEY_PASSWORD
```

Do not print secret values.

Do not commit signing material.

Ensure `.gitignore` still ignores:

```text
*.jks
*.keystore
*.p12
local.properties
```

and any other local Android secrets.

Run a release workflow or equivalent release build after migration.

---

# 16. Verify Luno Mobile Before Removing It From Desktop Repo

From a fresh clone of `Luno-Mobile`:

```bash
./gradlew testDebugUnitTest
./gradlew lintDebug
./gradlew assembleDebug
```

If possible:

```bash
./gradlew :app:installDebug
```

Verify:

- app launches;
- existing package ID remains;
- Room schemas are present;
- playback works;
- library access works;
- downloads work;
- discovery/recommendations work;
- transfer fixtures load;
- transfer tests pass;
- release workflow paths are valid;
- signing configuration is intact.

Do not proceed to delete mobile code from the original repo until this passes.

---

# 17. Phase 2 — Freeze the Legacy Python Desktop

Return to the original repository.

Create a final immutable Python desktop branch/tag **before removing Python files from main**.

Recommended:

```bash
git switch main
git pull --ff-only
git branch archive/python-desktop-final
git tag legacy-python-desktop-final
git push origin archive/python-desktop-final
git push origin legacy-python-desktop-final
```

If desired, make a dedicated commit on the archive branch that adds an archive README explaining:

- this is the final Python/Tkinter/Pygame generation;
- it is unsupported/frozen;
- the active application moved to Tauri/Rust;
- how to install dependencies;
- how to run tests;
- how to build the PyInstaller package.

Do not rewrite the historical application merely to make the archive prettier.

---

# 18. Legacy Python Dependency Inventory

Before deleting anything from active `main`, audit imports and build references.

Determine the full legacy boundary using:

- `music_player.py` imports;
- all modules imported by those modules;
- `views/`;
- `requirements.txt`;
- `Luno.spec`;
- Python tests;
- build scripts;
- legacy release helpers;
- legacy docs needed to understand the code.

Create a file such as:

```text
docs/archive/python-desktop-file-inventory.md
```

on the archive branch, or record the inventory in the migration notes.

The archive must be able to answer:

> “What exact files comprised the final Python desktop application?”

---

# 19. Optional Legacy Source Package

From the immutable archive tag:

```bash
git archive \
  --format=tar.gz \
  --output=legacy-python-desktop-final.tar.gz \
  legacy-python-desktop-final
```

Optionally generate SHA-256:

```bash
shasum -a 256 legacy-python-desktop-final.tar.gz \
  > legacy-python-desktop-final.tar.gz.sha256
```

Store these somewhere durable.

This is supplemental.

The branch/tag is the primary historical record.

---

# 20. Phase 3 — Convert Existing Repository Into Luno Desktop

Once:

- Luno Mobile is independently verified;
- the Python archive exists remotely;
- the pre-split checkpoint exists remotely;

clean the active `main`.

Remove:

```text
mobile-app/
build-mobile.command
.github/workflows/android-release.yml
```

Then remove the Python desktop runtime/build/test files that the dependency audit classified as legacy-only.

Likely removals include:

```text
music_player.py
engine.py
downloader.py
discovery.py
previews.py
rec_artwork.py
video_preview.py
library_tools.py
library_transfer.py
source_lookup.py
theme.py
utils.py
widgets.py
views/
requirements.txt
pytest.ini
Luno.spec
build.command
```

and the Python-specific tests/workflow:

```text
test_*.py
Python-only tests/
.github/workflows/desktop.yml
```

But do not delete based solely on this list.

Use actual dependency ownership.

---

# 21. Files That Must Remain In Luno Desktop

Retain all current Tauri application assets, including:

```text
src/
src-tauri/
package.json
package-lock.json
tsconfig.json
vitest.config.ts
dev.command
build-test.command
contracts/library-export/v1/
release/
release-input/
scripts/
.github/workflows/desktop-tauri-foundation.yml
.github/workflows/desktop-tauri-release.yml
```

Retain Tauri migration/legacy compatibility code.

Retain desktop binary provisioning infrastructure.

Retain third-party notices required by the Tauri package.

Retain anything referenced by:

```text
src-tauri/tauri.conf.json
```

or by the active Tauri CI/release scripts.

---

# 22. Be Careful With release/ and release-input/

Do not assume `release/` is old Python junk.

The Tauri application currently bundles resources from the root release infrastructure, including items such as:

```text
release/desktop-binaries.json
release/THIRD_PARTY_NOTICES.md
src-tauri/resources/bin
```

and the desktop release workflow relies on:

```text
release-input/bin
scripts/provision-desktop-binaries.sh
scripts/verify-desktop-bundle.sh
```

These belong to Luno Desktop.

Only archive/delete files proven to belong exclusively to Python.

---

# 23. Rename Repository To Luno Desktop

After active `main` is desktop-only:

Rename the GitHub repository:

```text
musicPlayer
    ↓
Luno-Desktop
```

Update local remote if necessary:

```bash
git remote -v
git remote set-url origin <new-Luno-Desktop-url>
```

Verify pushes/pulls still work.

---

# 24. Rename Human-Facing Desktop Product

The internal npm and Rust package names are already `luno-desktop`.

Update Tauri human-facing branding.

In `src-tauri/tauri.conf.json`, change:

```json
"productName": "Luno"
```

to:

```json
"productName": "Luno Desktop"
```

Change the application window title from:

```json
"title": "Luno"
```

to:

```json
"title": "Luno Desktop"
```

where appropriate.

---

# 25. Do Not Change Desktop Application Identifier

Keep:

```text
com.ty.luno
```

This is not merely branding.

Changing it can affect:

- application identity;
- OS data locations;
- upgrades;
- keychain/access control behavior;
- permissions;
- signing expectations;
- migration behavior.

A future deliberate identifier migration can be performed separately if desired.

---

# 26. Update Desktop Package/Release Paths After Product Rename

Changing Tauri `productName` may change generated bundle names.

Audit and update scripts/workflows that currently assume:

```text
Luno.app
```

Examples include:

```text
build-test.command
.github/workflows/desktop-tauri-release.yml
scripts/verify-desktop-bundle.sh
```

Expected new bundle may be:

```text
Luno Desktop.app
```

Do not guess.

Build once and use the actual generated path.

Likewise verify DMG naming.

---

# 27. Desktop CI Cleanup

After Python/mobile extraction, `.github/workflows/` in Luno Desktop should contain only workflows that apply to the current desktop codebase.

Expected active workflows:

```text
desktop-tauri-foundation.yml
desktop-tauri-release.yml
```

Remove:

```text
android-release.yml
desktop.yml
```

from active `main` after their corresponding codebases are safely preserved elsewhere.

Optionally rename the remaining workflow files later for clarity, but that is not required for functional separation.

---

# 28. Root .gitignore Cleanup

Rewrite `.gitignore` so it reflects Luno Desktop only.

Retain entries for things such as:

```text
node_modules/
dist/
src-tauri/target/
src-tauri/gen/
release-input/bin/
src-tauri/resources/bin/*
```

and any Tauri/macOS generated files.

Remove Android-specific entries from Desktop once Mobile has been extracted, unless they remain useful generically.

The Luno Mobile repo should get its own Android-specific `.gitignore` containing:

```text
.gradle/
build/
*.apk
local.properties
*.jks
*.keystore
*.p12
```

plus normal IDE/platform output as appropriate.

---

# 29. Documentation Strategy

Do documentation cleanup **after** functional separation is proven.

Do not spend the first half of this migration rewriting docs while paths are still moving.

---

## 29.1 Luno Desktop README

Replace the old monorepo README with a focused README.

Suggested structure:

```markdown
# Luno Desktop

Luno Desktop is the desktop edition of Luno, built with React, TypeScript,
Tauri 2, and Rust.

## Stack
## Development
## Testing
## Packaging
## Architecture
## User Data / Migration
## Luno Mobile
## Legacy Python Desktop
```

Mention:

- Luno Mobile now lives in its own repository;
- the Python desktop generation is archived under the legacy tag/branch;
- the shared transfer format remains compatible.

---

## 29.2 Luno Mobile README

Create a dedicated README describing:

- Android/Kotlin/Compose architecture;
- supported SDKs;
- development setup;
- NewPipeExtractor setup;
- tests;
- release build;
- signing environment variables;
- transfer format;
- relationship to Luno Desktop.

---

# 30. Migration-Era Documentation Cleanup

The desktop repo currently contains extensive migration-era material.

Examples may include:

```text
migration-plan.md
migration-implementation-plan.md
migration-audit/
migration-work/
docs/migration/
docs/verification/
```

Do not bulk-delete these without classification.

Classify each item as:

1. **Still operationally relevant to current Tauri desktop**
2. **Historical Python → Tauri migration reference**
3. **Obsolete scratch/work artifact**

Recommended final structure:

```text
docs/
├── architecture/
├── decisions/
├── product/
├── release/
├── verification/
└── archive/
    └── python-to-tauri/
```

Move useful historical migration material to:

```text
docs/archive/python-to-tauri/
```

Delete only obvious obsolete scratch artifacts after confirming they are not referenced by active tests/scripts/docs.

Do not let the active root remain dominated by migration planning files.

---

# 31. project_brain.md / Agent Documentation

There are currently project-brain/agent-oriented files for different app generations.

After separation:

- Desktop gets only Desktop-relevant agent instructions/context.
- Mobile gets Mobile-relevant agent instructions/context.
- Python-specific operational guidance lives with the archive.

Audit:

```text
AGENTS.md
project_brain.md
mobile-app/project_brain.md
```

and split/rewrite them by repository ownership.

Do not leave instructions in Luno Desktop that tell agents to modify Android code that no longer exists.

Do not leave instructions in Luno Mobile that assume the desktop source is one directory above.

---

# 32. Verify Desktop After Cleanup

From a **fresh clone** of `Luno-Desktop`:

```bash
npm ci
npm run typecheck
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

Then:

```bash
npm run tauri dev
```

Verify launch.

Then perform a production bundle:

```bash
npx tauri build
```

Verify:

- generated `.app`;
- generated DMG if configured;
- packaged binaries/resources;
- bundle verification script;
- current user-data migration;
- application title says `Luno Desktop`;
- identifier remains `com.ty.luno`.

---

# 33. Desktop Functional Smoke Test

At minimum manually verify:

- app launch;
- library root loading;
- library scan;
- playlists;
- track selection;
- playback;
- pause/resume;
- seek;
- next/previous;
- queue;
- favorites;
- recent/history behavior;
- search;
- downloader;
- download completion;
- artwork;
- discover/recommendations;
- preview playback;
- saving a recommendation;
- video behavior if currently supported;
- settings;
- media keys;
- app shutdown;
- restart;
- existing user config/data migration.

The split must not create “works in tests but broken in packaged app” regressions.

---

# 34. Mobile Functional Smoke Test

From a fresh clone of Luno Mobile:

- build debug APK;
- install;
- launch;
- load existing library/storage;
- verify playback;
- queue controls;
- library scanning;
- Room DB behavior;
- downloads;
- background work;
- artwork;
- recommendations/discovery;
- settings;
- transfer import/export;
- app restart.

If release signing is available, build a release APK too.

---

# 35. Cross-Repository Transfer Compatibility Gate

This is a hard requirement.

Use real fixtures and/or a small temporary library.

Test:

## 35.1 Desktop → Mobile

1. Create/export library-transfer manifest from Luno Desktop.
2. Import into Luno Mobile.
3. Verify:
   - schema accepted;
   - playlist/track matching works;
   - no unexpected path leakage;
   - provider IDs remain valid;
   - ambiguity handling remains unchanged;
   - no unintended deletions.

## 35.2 Mobile → Desktop

1. Export from Luno Mobile.
2. Import into Luno Desktop.
3. Verify equivalent behavior.

## 35.3 Round trip

Perform:

```text
Desktop → Mobile → Desktop
```

and compare expected logical content.

Do not declare the split complete until this passes.

---

# 36. Git History Expectations

## Luno Desktop

Because the existing repository is being renamed, it should retain the full historical repository history.

That is acceptable.

The active `main` tree becomes clean even though historical commits still contain mobile/Python files.

Do **not** rewrite Luno Desktop history simply to erase old files unless there is a specific security/legal reason.

## Luno Mobile

Prefer preserving meaningful mobile commit history through filtering.

The Mobile repository does not need the entire desktop history.

---

# 37. Avoid Git Submodules Between Desktop and Mobile

Do not solve the split by turning Mobile into a submodule of Desktop or vice versa.

The final goal is two independent active repositories.

Shared language-neutral fixtures may temporarily exist in both.

No runtime or build should require cloning the other application repository.

---

# 38. No Cross-Repo Relative Paths

After separation, search both repositories for assumptions like:

```text
../mobile-app
../contracts
../src-tauri
../musicPlayer
../../desktop
```

Every build/test/runtime dependency must resolve inside its own repository or through a legitimate external dependency manager.

Useful searches:

```bash
rg '\.\./'
rg 'mobile-app'
rg 'musicPlayer'
rg 'Luno-Android'
rg 'Luno\.app'
rg 'com\.ty\.luno'
rg 'com\.luno\.mobile'
```

Review matches manually.

Do not blindly replace identifiers.

---

# 39. No Hidden Local-Only Dependencies

A fresh clone is the authority.

A migration is not complete if it only builds on the original developer laptop because of:

- untracked vendor code;
- local symlinks;
- local Gradle cache assumptions;
- absolute paths;
- `/Users/ty/...` references;
- globally installed binaries that packaging expected to bundle;
- local signing files;
- ignored release input.

Search for absolute developer-machine paths.

Examples:

```bash
rg '/Users/'
rg '/opt/homebrew/'
rg '/usr/local/'
```

Some runtime PATH fallbacks may be intentional, but all occurrences must be reviewed.

The legacy Python archive may preserve historical machine-specific paths if they were genuinely part of that final build; the active apps should not rely on accidental local state.

---

# 40. Secrets and Sensitive Files

Before pushing the new Mobile repository, inspect:

```text
local.properties
*.jks
*.keystore
*.p12
.env*
signing files
API keys
```

Do not copy local-only secret files into the new repo.

The current project may contain ignored files locally that are not tracked; filtering Git history will not include them, but manual copying might.

Do not manually `cp -R mobile-app` without reviewing ignored content.

---

# 41. Commit Strategy

Use small, reversible commits.

Recommended sequence in Luno Desktop:

```text
chore(repo): checkpoint pre-split state
chore(repo): archive final python desktop generation
chore(repo): remove extracted android application
chore(repo): remove archived python desktop runtime
chore(desktop): rename product to Luno Desktop
chore(ci): remove legacy python and android workflows
docs: update repository for desktop-only architecture
```

Recommended sequence in Luno Mobile:

```text
chore(repo): extract android app from musicPlayer
chore(build): repair root-relative gradle paths
chore(contracts): vendor library transfer v1 fixtures
chore(ci): adapt android release workflow to standalone repo
chore(branding): rename release artifacts to Luno Mobile
docs: add standalone mobile repository documentation
```

Do not squash everything into one impossible-to-review migration commit if avoidable.

---

# 42. Rollback Strategy

At every destructive phase, rollback must be obvious.

## Before mobile extraction

Rollback source:

```text
pre-luno-repo-split
archive/pre-luno-repo-split
```

## After mobile extraction but before deletion from Desktop

If Mobile fails:

- fix Luno Mobile;
- do not delete original `mobile-app/` yet.

## After Python archive creation

If Python archive is incomplete:

- recreate/fix archive before deleting anything from main.

## After Desktop cleanup

If Desktop functionality breaks:

```bash
git revert <cleanup commits>
```

or restore from:

```text
archive/pre-luno-repo-split
```

Do not force-push rewritten main history as a rollback strategy.

---

# 43. Destructive Operations Policy

Before any command like:

```bash
rm -rf
git clean
git reset --hard
git filter-repo
```

the agent must:

1. confirm the correct repository;
2. confirm current branch;
3. confirm clean working tree;
4. confirm remote checkpoint exists;
5. print/inspect the target paths;
6. use a separate clone when rewriting history.

Never run `git filter-repo` against the primary working clone used for Luno Desktop.

Use a disposable extraction clone for Luno Mobile.

---

# 44. Expected Final Luno Desktop Structure

Exact files may vary, but the top level should conceptually resemble:

```text
Luno-Desktop/
├── .github/
│   └── workflows/
│       ├── desktop-tauri-foundation.yml
│       └── desktop-tauri-release.yml
├── contracts/
│   └── library-export/
│       └── v1/
├── docs/
├── release/
├── release-input/
├── scripts/
├── src/
├── src-tauri/
├── .gitignore
├── AGENTS.md
├── README.md
├── build-test.command
├── dev.command
├── package.json
├── package-lock.json
├── tsconfig.json
└── vitest.config.ts
```

There should be no active Android project.

There should be no active Python/Tkinter desktop runtime.

---

# 45. Expected Final Luno Mobile Structure

Conceptually:

```text
Luno-Mobile/
├── .github/
│   └── workflows/
│       └── android-release.yml
├── app/
├── contracts/
│   └── library-export/
│       └── v1/
├── gradle/
├── gradlew
├── gradlew.bat
├── build.gradle.kts
├── gradle.properties
├── settings.gradle.kts
├── ANDROID_RELEASES.md
├── build-mobile.command
├── README.md
└── .gitignore
```

Plus the canonical NewPipeExtractor dependency arrangement if required.

There should be no Desktop source.

---

# 46. Legacy Archive Expectations

The archive must preserve the final Python generation as an understandable historical artifact.

At minimum it should preserve:

- source;
- views;
- requirements;
- tests;
- PyInstaller spec;
- app icon/resources required by the old build;
- old build helper;
- relevant docs.

Recommended archive README:

```markdown
# Luno Legacy Python Desktop

This branch/tag preserves the final Python/Tkinter/Pygame desktop generation of
Luno before the active desktop application moved to React/TypeScript/Tauri/Rust.

This code is frozen for historical reference and compatibility research.

Active desktop development:
Tsohnle95/Luno-Desktop

Mobile development:
Tsohnle95/Luno-Mobile
```

---

# 47. Things The Agent Must Not Accidentally Delete

Explicitly verify before deletion:

```text
src-tauri/src/migration/
src-tauri/src/persistence/legacy.rs
contracts/library-export/v1/
release/
release-input/
scripts/
src-tauri/resources/
current Tauri tests
current TypeScript tests
```

These may look migration-related or old, but they are part of the active application/toolchain.

---

# 48. Things The Agent Must Not Accidentally Rename

Do not change:

```text
com.ty.luno
com.luno.mobile
```

Do not bulk search/replace `Luno` across all source files.

Only update human-facing branding where appropriate.

A mass rename could alter:

- storage keys;
- file names;
- config paths;
- IPC contracts;
- migration logic;
- bundle identifiers;
- protocol names;
- test fixtures.

Review every rename context.

---

# 49. Application Data Compatibility

Repository cleanup must not require users to rebuild their libraries or lose settings.

Desktop must continue to understand existing data such as current legacy configuration/migration inputs.

Do not remove Tauri first-run migration support simply because the Python runtime has been archived.

Mobile must preserve existing Room schema history and application ID so existing installations upgrade normally.

---

# 50. Release Verification

## Desktop

Verify:

- production app bundle builds;
- DMG builds if configured;
- required binaries are bundled;
- third-party notices are present;
- signing workflow paths are correct;
- bundle verification script uses the new product path/name;
- release artifact is named appropriately.

## Mobile

Verify:

- release APK builds;
- release version validation still works;
- signing env vars are read;
- `apksigner` validation works;
- GitHub release file path is correct;
- release artifact uses `Luno-Mobile-*`.

---

# 51. Definition of Done

The task is complete only when all conditions below are true.

## Repository separation

- [ ] `Tsohnle95/musicPlayer` has been renamed to `Tsohnle95/Luno-Desktop`.
- [ ] `Tsohnle95/Luno-Mobile` exists.
- [ ] Mobile code no longer exists on active Luno Desktop `main`.
- [ ] Python desktop runtime no longer exists on active Luno Desktop `main`.
- [ ] Python desktop is preserved under an immutable archive branch/tag.
- [ ] Shared transfer v1 fixtures exist in both active repositories.
- [ ] No active build depends on the other repository.

## Luno Desktop

- [ ] `npm ci` passes.
- [ ] TypeScript typecheck passes.
- [ ] frontend tests pass.
- [ ] frontend build passes.
- [ ] Rust tests pass.
- [ ] Tauri dev app launches.
- [ ] production bundle builds.
- [ ] product displays `Luno Desktop`.
- [ ] identifier remains `com.ty.luno`.
- [ ] existing user data still loads/migrates.
- [ ] release workflow succeeds or is fully validated.

## Luno Mobile

- [ ] fresh clone builds.
- [ ] NewPipeExtractor dependency is reproducible.
- [ ] unit tests pass.
- [ ] lint passes.
- [ ] debug APK builds.
- [ ] app installs/launches.
- [ ] application ID remains `com.luno.mobile`.
- [ ] Room schema history is preserved.
- [ ] release workflow is adapted to repository root.
- [ ] signing secrets exist in the new repo.
- [ ] release APK builds/validates.

## Compatibility

- [ ] Desktop transfer fixture tests pass.
- [ ] Mobile transfer fixture tests pass.
- [ ] Desktop → Mobile import works.
- [ ] Mobile → Desktop import works.
- [ ] Desktop → Mobile → Desktop round trip works.
- [ ] no functionality was intentionally changed as part of the split.

## Cleanup

- [ ] Desktop README describes only Desktop.
- [ ] Mobile README describes only Mobile.
- [ ] agent/project documentation is repo-specific.
- [ ] obsolete monorepo path references are gone.
- [ ] obsolete Android CI is gone from Desktop.
- [ ] obsolete Python CI is gone from Desktop.
- [ ] active docs are not dominated by completed migration scratchwork.
- [ ] no secrets or local-only files were accidentally committed.

---

# 52. Required Final Report From The Agent

At completion, produce a concise migration report containing:

## Repositories

```text
Desktop:
<final GitHub URL>

Mobile:
<final GitHub URL>
```

## Checkpoints

```text
Pre-split commit:
<SHA>

Archive branch:
archive/python-desktop-final

Archive tag:
legacy-python-desktop-final
```

## Verification Results

```text
Luno Desktop
- npm ci:
- typecheck:
- frontend tests:
- frontend build:
- cargo tests:
- tauri dev:
- production build:
- package verification:

Luno Mobile
- fresh clone:
- Gradle unit tests:
- lint:
- debug build:
- install/launch:
- release build:
- release signing verification:

Transfer
- Desktop -> Mobile:
- Mobile -> Desktop:
- round trip:
```

## Important Changes

List:

- paths changed;
- CI changed;
- branding changed;
- shared fixtures copied;
- dependency fixes required for fresh clone;
- files archived;
- files removed from Desktop `main`.

## Deviations

Document any deviation from this plan and why it was necessary.

Do not silently improvise around failures.

---

# 53. Recommended Execution Order

Follow this order unless a concrete repository fact requires otherwise.

```text
1. Inspect repo and confirm branch/worktree state.
2. Create/push pre-split tag and archive branch.
3. Record pre-split commit SHA.
4. Establish Tauri baseline.
5. Establish Python baseline.
6. Fresh-clone Android and establish Android baseline.
7. Resolve/verify the NewPipeExtractor dependency arrangement.
8. Create a separate extraction clone for Mobile.
9. Filter Mobile history into standalone repo shape.
10. Include transfer v1 fixtures in Mobile.
11. Repair Mobile Gradle root-relative fixture path.
12. Repair build-mobile.command paths.
13. Repair Android release workflow paths.
14. Create/push Tsohnle95/Luno-Mobile.
15. Recreate Android release secrets.
16. Verify Luno Mobile from a fresh clone.
17. Create/push archive/python-desktop-final.
18. Create/push legacy-python-desktop-final tag.
19. Verify archived Python app/tests/package.
20. Optionally create legacy source tarball + checksum.
21. Return to original main.
22. Remove extracted Mobile code/workflow/helper.
23. Audit Python dependency boundary.
24. Remove Python-only runtime/tests/build files from main.
25. Retain Tauri compatibility/migration code.
26. Retain Tauri release/release-input/scripts/resources.
27. Rename GitHub repo to Luno-Desktop.
28. Update local remote URL.
29. Change Tauri product/window branding to Luno Desktop.
30. Keep com.ty.luno unchanged.
31. Repair .app/DMG paths affected by product rename.
32. Remove stale Python/Android CI from Desktop.
33. Split/clean .gitignore files.
34. Rewrite Desktop README.
35. Write Mobile README.
36. Split AGENTS/project-brain guidance.
37. Archive or remove stale migration-era docs.
38. Fresh-clone Luno Desktop.
39. Run full Desktop verification.
40. Fresh-clone Luno Mobile.
41. Run full Mobile verification.
42. Run Desktop -> Mobile transfer test.
43. Run Mobile -> Desktop transfer test.
44. Run round-trip transfer test.
45. Review both repos for cross-repo relative paths.
46. Review both repos for machine-specific absolute paths.
47. Review both repos for accidentally committed secrets.
48. Produce final migration report.
49. Only then declare the old monorepo layout retired.
```

---

# 54. Final Principle

The success criterion is not merely:

> “The files are now in two repositories.”

The success criterion is:

> **Luno Desktop and Luno Mobile are independently understandable, independently buildable, independently releasable applications, while the final Python desktop remains permanently recoverable for reference, and no user-visible behavior or data compatibility was lost during the separation.**

Keep the migration boring, reversible, and evidence-driven.
