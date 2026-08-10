# Luno Library Export v1

This is a metadata-only transfer format named `luno.library.export` with
`manifest_version` equal to `1`.

## Guarantees

- Local paths and audio bytes are never included.
- Secrets, queue state, favorites, and playback history are never included.
- Unknown fields are rejected.
- Track references are bounded identifiers, not filesystem paths.
- A track without a provider identity must explicitly set
  `ambiguity_confirmation_required` to `true`.
- Imports are additive; consumers must not delete existing library content.

`schema.json` describes the wire shape. Consumers must also enforce the
semantic rules documented here and in their implementation tests, including
the virtual `All Music` playlist restriction and reference uniqueness.

The Python and Kotlin implementations are expected to accept the valid fixture
and reject the invalid fixture in this directory.
