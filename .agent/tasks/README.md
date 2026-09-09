# Temporary Task Memory

Active multi-phase work lives in `.agent/tasks/<task>.md`.

## Lifecycle

`DRAFT` → `REVIEWED` → `ACTIVE` → `COMPLETE` → `DELETE`

- `PATCH` work must not create a task file.
- `FEATURE` work stays in-context unless durable continuation across
  sessions, models, agents, or commits is genuinely useful.
- `PROJECT` work uses one task file.
- Large `BATCH` work may use one when handoff/recovery benefits.

## Rules

- Task files are temporary execution memory, not permanent documentation.
- Never copy canonical architecture prose into a task file; link to it.
- Record facts, decisions, and checkpoint commit IDs.
- Do not preserve chain-of-thought or discussion transcripts.
- Before deletion verify: acceptance criteria satisfied, cumulative
  validation passed, canonical durable docs updated, future work moved to
  backlog/issues, final Git state reviewed.
- Git preserves history; the live project brain must not accumulate
  completed project logs.

See `_TEMPLATE.md` for the required structure.
