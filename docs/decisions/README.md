# Architecture Decisions

This directory contains durable decisions that should not be mixed into the
current-state architecture map or product roadmap.

Each decision should record its context, chosen approach, consequences, and
status. Historical implementation details belong in the changelog rather than
in the architecture overview.

## Existing Decisions

- The repository is a monorepo containing independent desktop and Android
  implementations.
- The library export format is versioned and path-free.
- Android currently uses manual application-level dependency wiring; Hilt is
  deferred until it solves a concrete testing or construction problem.
