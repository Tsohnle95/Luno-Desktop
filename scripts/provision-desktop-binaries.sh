#!/bin/sh
set -eu

# Copy release-reviewed executables into the Tauri resource layout. Downloads
# are deliberately not implicit: release automation supplies an audited input.
src=${LUNO_BINARY_SOURCE_DIR:?Set LUNO_BINARY_SOURCE_DIR to the reviewed binary directory}
dest=${LUNO_BINARY_DEST_DIR:-src-tauri/resources/bin}
manifest=${LUNO_BINARY_MANIFEST:-release/desktop-binaries.json}

command -v jq >/dev/null 2>&1 || { echo "jq is required" >&2; exit 2; }
command -v file >/dev/null 2>&1 || { echo "file is required for architecture inspection" >&2; exit 2; }
mkdir -p "$dest"
for name in ffmpeg ffprobe yt-dlp; do
  file="$src/$name"
  [ -f "$file" ] || { echo "missing reviewed binary: $file" >&2; exit 1; }
  install -m 755 "$file" "$dest/$name"
done

for name in ffmpeg ffprobe yt-dlp; do
  binary="$dest/$name"
  description=$(file -b "$binary") || { echo "architecture inspection failed for $name" >&2; exit 1; }
  actual_arches=""
  case "$description" in *arm64*) actual_arches="arm64";; esac
  case "$description" in *x86_64*) actual_arches="${actual_arches:+$actual_arches }x86_64";; esac
  [ -n "$actual_arches" ] || { echo "unable to determine architecture for $name: $description" >&2; exit 1; }
  for actual in $actual_arches; do
    normalized=$actual
    [ "$actual" = arm64 ] && normalized=aarch64
    jq -e --arg arch "$normalized" '.architectures | index($arch) != null' "$manifest" >/dev/null || {
      echo "incompatible architecture for $name: $actual" >&2
      exit 1
    }
  done
done

for name in ffmpeg ffprobe yt-dlp; do
  expected_version=$(jq -r --arg n "$name" '.artifacts[] | select(.name == $n) | .version' "$manifest")
  case "$expected_version" in ""|null|REPLACE_*) echo "missing reviewed version for $name" >&2; exit 1;; esac
  case "$name" in
    ffmpeg|ffprobe)
      if version_output=$("$dest/$name" -version 2>&1); then
        :
      else
        command_status=$?
        echo "version command failed for $name (exit $command_status)" >&2
        exit 1
      fi
      reported_version=$(printf '%s\n' "$version_output" | awk 'NR == 1 { sub(/^.* version /, ""); print $1; exit }')
      ;;
    yt-dlp)
      if version_output=$("$dest/$name" --version 2>&1); then
        :
      else
        command_status=$?
        echo "version command failed for $name (exit $command_status)" >&2
        exit 1
      fi
      reported_version=$(printf '%s\n' "$version_output" | awk 'NR == 1 { sub(/\r$/, ""); print; exit }')
      ;;
  esac
  [ "$reported_version" = "$expected_version" ] || {
    echo "version mismatch for $name: expected $expected_version, got ${reported_version:-unavailable}" >&2
    exit 1
  }
done

for name in ffmpeg ffprobe yt-dlp; do
  hash=$(shasum -a 256 "$dest/$name" | awk '{print $1}')
  expected=$(jq -r --arg n "$name" '.artifacts[] | select(.name == $n) | .sha256' "$manifest")
  [ "$expected" != "REPLACE_WITH_REVIEWED_SHA256" ] || { echo "unreviewed checksum for $name" >&2; exit 1; }
  [ "$hash" = "$expected" ] || { echo "checksum mismatch for $name" >&2; exit 1; }
done
