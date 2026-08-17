#!/bin/sh
# Stage the pinned, reviewed desktop binaries into src-tauri/resources/bin so
# the Tauri bundler ships them inside the app resources (mapped to "bin").
# Verification is against the pinned manifest release/desktop-binaries.json;
# unpinned binaries are never copied.
set -eu
cd "$(dirname "$0")/.."

MANIFEST=release/desktop-binaries.json
INPUT=release-input/bin
OUT=src-tauri/resources/bin
[ -f "$MANIFEST" ] || { echo "missing $MANIFEST" >&2; exit 1; }

python3 - "$MANIFEST" "$INPUT" "$OUT" <<'PY'
import hashlib, json, os, shutil, sys

manifest, input_dir, out_dir = sys.argv[1], sys.argv[2], sys.argv[3]
with open(manifest, encoding="utf-8") as f:
    manifest = json.load(f)

os.makedirs(out_dir, exist_ok=True)
for artifact in manifest["artifacts"]:
    name = artifact["name"]
    source = os.path.join(input_dir, name)
    if not os.path.isfile(source):
        print(f"missing input binary: {source}", file=sys.stderr)
        sys.exit(1)
    digest = hashlib.sha256(open(source, "rb").read()).hexdigest()
    if digest != artifact["sha256"]:
        print(f"sha256 mismatch for {name}: {digest}", file=sys.stderr)
        sys.exit(1)
    target = os.path.join(out_dir, name)
    shutil.copy2(source, target)
    os.chmod(target, 0o755)
    print(f"staged {name} (sha256 verified)")
print("provisioned resources/bin")
PY