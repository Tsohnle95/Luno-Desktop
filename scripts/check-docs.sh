#!/bin/bash
# Deterministic docs-drift checker: verifies mechanically checkable
# relationships between canonical docs and the repository. Proves surface
# alignment only — semantic correctness stays with agents/review, runtime
# correctness with tests.
set -euo pipefail
cd "$(dirname "$0")/.."

FAILURES=0
fail() { echo "check-docs FAIL: $1"; FAILURES=$((FAILURES + 1)); }

DOCS="AGENTS.md project_brain.md README.md docs/agent-execution.md
  docs/architecture/README.md docs/verification/desktop.md
  docs/release/desktop.md docs/decisions/README.md
  docs/decisions/download-format-m4a.md docs/modules/contracts.md
  docs/modules/frontend.md docs/modules/library.md docs/modules/playback.md
  docs/modules/downloads-discovery.md"

# 1. Every canonical doc exists and states its role (except the two
# lightweight entry points whose role is routing by position).
for doc in $DOCS; do
  [ -f "$doc" ] || { fail "missing canonical doc: $doc"; continue; }
  case "$doc" in
    AGENTS.md|README.md|docs/decisions/download-format-m4a.md) ;;
    *) grep -q "^Document role:" "$doc" || fail "$doc has no 'Document role:' statement" ;;
  esac
done

# 2. Relative markdown links in canonical docs resolve to real files.
TMP_LINKS=$(mktemp)
trap 'rm -f "$TMP_LINKS"' EXIT
for doc in $DOCS; do
  [ -f "$doc" ] || continue
  dir=$(dirname "$doc")
  matches=$(grep -o '\]([^http][^)]*)' "$doc" 2>/dev/null | sed 's/^](//; s/)$//' || true)
  [ -z "$matches" ] && continue
  echo "$matches" | while read -r link; do
    target=${link%%#*}
    [ -z "$target" ] && continue
    case "$target" in *.md|*.json|*.ts|*.rs|*.tsx|*.sh|*.toml|*.yml|*/*) ;;
      *) continue ;;
    esac
    if [ ! -e "$dir/$target" ] && [ ! -e "$target" ]; then
      echo "$doc links to missing target: $target" >> "$TMP_LINKS"
    fi
  done
done
if [ -s "$TMP_LINKS" ]; then
  sed 's/^/check-docs FAIL: /' "$TMP_LINKS"
  FAILURES=$((FAILURES + 1))
fi

# 3. Every backticked repo path claimed in the AGENTS.md module map and
# router tables exists on disk.
TMP_MAP=$(mktemp)
trap 'rm -f "$TMP_LINKS" "$TMP_MAP"' EXIT
grep -o '`[a-zA-Z0-9_./-]*\(/[a-zA-Z0-9_./-]*\)\+`' AGENTS.md \
  | tr -d '`' | sort -u | while read -r path; do
  target=${path%%#*}
  case "$target" in *.command|*.app) continue ;; esac
  if [ ! -e "$target" ]; then
    echo "AGENTS.md references missing path: $target" >> "$TMP_MAP"
  fi
done
if [ -s "$TMP_MAP" ]; then
  sed 's/^/check-docs FAIL: /' "$TMP_MAP"
  FAILURES=$((FAILURES + 1))
fi

# 4. Documented npm commands exist as package.json scripts.
for script in typecheck test verify build; do
  node -e "process.exit(require('./package.json').scripts['$script'] ? 0 : 1)" \
    || fail "package.json missing script referenced by docs: $script"
done

# 5. Contract version parity across the TS/Rust duality.
TSV=$(grep -o 'CONTRACT_VERSION = [0-9]*' src/shared/contracts.ts | grep -o '[0-9]*')
RSV=$(grep -o 'CONTRACT_VERSION: u16 = [0-9]*' src-tauri/src/contracts/types.rs | grep -o '[0-9]*$')
[ -n "$TSV" ] && [ "$TSV" = "$RSV" ] || fail "CONTRACT_VERSION drift: TS=$TSV Rust=$RSV"

# 6. No unstable hard-coded source line references (file.rs:123) in docs.
if grep -rnE '`[a-zA-Z0-9_./-]+\.(rs|ts|tsx):[0-9]+`' $DOCS AGENTS.md 2>/dev/null; then
  fail "hard-coded source line references found (use file paths/symbols)"
fi

# 7. Toolchain pins exist and CI derives Node from the canonical pin.
[ -f ".node-version" ] || fail "missing .node-version pin"
[ -f "rust-toolchain.toml" ] || fail "missing rust-toolchain.toml pin"
grep -q "node-version-file" .github/workflows/desktop-tauri-foundation.yml \
  || fail "CI does not derive Node from .node-version (node-version-file)"

# 8. Vitest include entries resolve to real files.
node -e "
  const fs = require('fs');
  const src = fs.readFileSync('vitest.config.ts', 'utf8');
  const includes = [...src.matchAll(/\"([^\"]+)\"/g)].map(m => m[1]).filter(p => p.endsWith('.ts') || p.endsWith('.tsx'));
  const missing = includes.filter(p => !fs.existsSync(p));
  if (missing.length) { console.error('missing vitest targets: ' + missing.join(', ')); process.exit(1); }
" || fail "vitest include lists files that do not exist"

# 9. No ambiguous bare source filenames in the AGENTS.md router
# (paths there must carry enough segments to locate the file).
if grep -nE '`(state|lib|main|mod)\.(rs|ts)`' AGENTS.md; then
  fail "ambiguous bare filename in AGENTS.md (use a rooted path)"
fi

if [ "$FAILURES" -ne 0 ]; then
  echo "check-docs: $FAILURES failure(s)"
  exit 1
fi
echo "check-docs: OK"
