# Production Downloads Produce M4A/AAC, Not MP3

Status: accepted (desktop production wiring)

## Context

The desktop downloader's promotion contract validated audio output as ID3 /
MPEG-sync ("looks like mp3") and the shell named destinations `*.mp3`. The
pinned, reviewed media binaries (R06: ffmpeg/ffprobe 7.1.1 LGPL-only build,
yt-dlp 2025.08.11) contain the built-in AAC encoder but no libmp3lame or
libshine, so MP3 *encoding* is impossible with the reviewed toolchain. During
native verification the helper failed with `audio conversion failed: Encoder
not found` when asked for `--audio-format mp3`.

## Chosen Approach

- The bundled download helper requests `--audio-format m4a` (`-x`), which
  ffmpeg satisfies with the built-in LGPL AAC encoder.
- Promotion's audio signature check additionally accepts the MP4 `ftyp`
  signature for audio outputs (previously `ftyp` was video-only), so
  downloaded M4A files pass the same atomic promotion gate as fixture MP3s.
- The production shell names download destinations `*.m4a`; dev fixture runs
  keep the historical `*.mp3` naming so gate receipts remain unchanged.
- The library scanner already lists `m4a` in its supported extensions, so a
  rescanned download appears in the library without further changes.

## Consequences

- Production downloads are AAC-LC in an MP4 container instead of MP3; players
  and the library's symphonia decoder (isomp4 feature) handle them.
- The fixture helper contract (write ID3 bytes to the staged output) is
  unchanged and remains the gate-4 evidence path.
- If a future review adds an MP3 encoder to the bundled ffmpeg, the helper can
  switch back to `--audio-format mp3` without contract changes.

## Evidence

- Native run `luno-unit2.JxXx`: UI search -> download -> job `succeeded`; the
  promoted file is `ISO Media, Apple iTunes ALAC/AAC-LC (.M4A) Audio` with a
  leading `ftyp` atom, and appears in the library after a rescan.