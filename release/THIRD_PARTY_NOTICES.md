# Luno bundled binary notices

This file is shipped beside the application and must be updated with every
binary update. Release approval must attach the upstream license text and the
reviewed SHA-256 for each architecture.

## FFmpeg

FFmpeg is licensed under the GNU Lesser General Public License (LGPL), version
2.1 or later, unless a release build enables GPL components. The exact build
configuration and complete upstream license text for the attached artifact are
release evidence, not generated at runtime.

## yt-dlp

yt-dlp is licensed under the Unlicense. Include the upstream notice for the
exact attached executable in the release evidence.

The manifest intentionally fails verification until `sha256` values and the
corresponding release artifacts have received human licensing/signing review.
