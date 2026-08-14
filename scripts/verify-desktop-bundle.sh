#!/bin/sh
set -eu

app=${1:?usage: verify-desktop-bundle.sh /path/to/Luno.app}
root="$app/Contents/Resources"
manifest="$root/desktop-binaries.json"
[ -f "$manifest" ] || { echo "bundle manifest missing: $manifest" >&2; exit 1; }
[ -f "$root/THIRD_PARTY_NOTICES.md" ] || { echo "third-party notices missing" >&2; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "jq is required" >&2; exit 2; }

[ "$(jq -r .platform "$manifest")" = macos ] || { echo "wrong platform manifest" >&2; exit 1; }
for name in ffmpeg ffprobe yt-dlp; do
  binary="$root/bin/$name"
  [ -x "$binary" ] || { echo "non-executable bundled $name" >&2; exit 1; }
  expected=$(jq -r --arg n "$name" '.artifacts[] | select(.name == $n) | .sha256' "$manifest")
  case "$expected" in REPLACE_*|""|null) echo "unreviewed checksum for $name" >&2; exit 1;; esac
  actual=$(shasum -a 256 "$binary" | awk '{print $1}')
  [ "$actual" = "$expected" ] || { echo "checksum mismatch for $name" >&2; exit 1; }
done

for name in ffmpeg ffprobe yt-dlp; do
  binary="$root/bin/$name"
  expected_version=$(jq -r --arg n "$name" '.artifacts[] | select(.name == $n) | .version' "$manifest")
  case "$expected_version" in ""|null|REPLACE_*) echo "missing reviewed version for $name" >&2; exit 1;; esac
  case "$name" in
    ffmpeg|ffprobe) reported_version=$(PATH=/usr/bin:/bin "$binary" -version 2>&1 | sed -n '1s/^[^ ]* version \([^ ]*\).*/\1/p') ;;
    yt-dlp) reported_version=$(PATH=/usr/bin:/bin "$binary" --version 2>&1 | sed -n '1p' | tr -d '\r') ;;
  esac
  [ "$reported_version" = "$expected_version" ] || {
    echo "version mismatch for $name: expected $expected_version, got ${reported_version:-unavailable}" >&2
    exit 1
  }
done

# A clean-machine smoke: PATH cannot provide a fallback. The managed directory
# is the only executable source used by the release process.
PATH=/usr/bin:/bin "$root/bin/ffmpeg" -version >/dev/null
PATH=/usr/bin:/bin "$root/bin/ffprobe" -version >/dev/null
PATH=/usr/bin:/bin "$root/bin/yt-dlp" --version >/dev/null
echo "bundle inspection and no-developer-PATH smoke passed"
