#!/bin/sh
set -eu

# Copy release-reviewed executables into the Tauri resource layout. Downloads
# are deliberately not implicit: release automation supplies an audited input.
src=${LUNO_BINARY_SOURCE_DIR:?Set LUNO_BINARY_SOURCE_DIR to the reviewed binary directory}
dest=${LUNO_BINARY_DEST_DIR:-src-tauri/resources/bin}
manifest=${LUNO_BINARY_MANIFEST:-release/desktop-binaries.json}

command -v jq >/dev/null 2>&1 || { echo "jq is required" >&2; exit 2; }
mkdir -p "$dest"
for name in ffmpeg ffprobe yt-dlp; do
  file="$src/$name"
  [ -f "$file" ] || { echo "missing reviewed binary: $file" >&2; exit 1; }
  install -m 755 "$file" "$dest/$name"
done

for name in ffmpeg ffprobe yt-dlp; do
  hash=$(shasum -a 256 "$dest/$name" | awk '{print $1}')
  expected=$(jq -r --arg n "$name" '.artifacts[] | select(.name == $n) | .sha256' "$manifest")
  [ "$expected" != "REPLACE_WITH_REVIEWED_SHA256" ] || { echo "unreviewed checksum for $name" >&2; exit 1; }
  [ "$hash" = "$expected" ] || { echo "checksum mismatch for $name" >&2; exit 1; }
done
