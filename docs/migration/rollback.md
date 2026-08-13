# First-Run Migration Rollback

Migration is macOS-only and additive. Before conversion, U42 copies every
present legacy config, URL map, duplicate set, and marker file into a
`migration-backup-*` directory beneath the target data directory. Legacy files
are never deleted or rewritten. Audio files are never touched.

If migration fails or is interrupted, do not remove the legacy application or
its data. Launch the Python release using the original `~/.luno_config.json`.
The target has no completed `migration-version.json` unless all durable writes
finished. A retry is safe; an already completed migration returns an explicit
`AlreadyComplete` result rather than overwriting target state.

Preview cleanup is the only deletion performed. It is limited to files in the
configured temporary preview directory and preserves explicitly current or
promoted assets. A receipt records the snapshot, target files, warning count,
and removed preview paths.
