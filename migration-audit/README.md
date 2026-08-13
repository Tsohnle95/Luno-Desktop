# Migration Audit Protocol

Each dossier must distinguish implemented behavior from documentation, plans,
and unknowns. Every substantive claim must cite concrete source, test, fixture,
or configuration evidence with repository-relative paths and line ranges when
available.

Use this structure for each feature:

```markdown
## Feature
- Status: implemented | documented-only | planned | obsolete | unknown
- Evidence:
  - `path:line-range`
- User behavior:
- State and data flow:
- Persistence and compatibility:
- Async, lifecycle, and failure behavior:
- UI states and visual requirements:
- Migration implementation requirements:
- Verification requirements:
- Confidence: high | medium | low
- Open questions:
```

Each dossier ends with:

```markdown
## Audit Receipt
- Files inspected:
- Tests/fixtures inspected:
- Claims requiring review:
- Coverage gaps:
- Status: complete | incomplete | inconclusive
```

The authority order is source and tests, then project brains and architecture
docs, then migration plans and roadmap proposals. Never present a proposal as
implemented behavior.
