# Agent Execution System

Document role: canonical owner for how agents size work, plan, execute,
validate, review, delegate, checkpoint, replan, and recover.

Philosophy: use the minimum process that keeps expected repair cost
acceptably low. Planning tokens are useful when they reduce uncertainty,
coordination cost, or likely rework. Planning is not valuable as ceremony.

Core loop:

```text
understand → route context → classify → plan only as needed → implement
→ targeted validate → review → canonical validate → document durable
truth → commit
```

## 1. Understand and route

1. Read `AGENTS.md` and honor applicable scoped instructions.
2. Route via the task/symptom router to only the relevant canonical doc.
3. Inspect the specific implementation symbols and nearby tests named there.
4. Expand outward only when evidence (a call edge, contract, shared state,
   failing test) reveals another dependency or invariant.

Context is a scarce working set. After a completed phase, retain only facts
needed for the next phase. A fresh agent must be able to resume from
`AGENTS.md` + active task file + relevant canonical docs + source/tests +
recent commits; if it cannot, the task memory is insufficient, not the
conversation.

## 2. Classify

| Mode | Use when |
|---|---|
| `PATCH` | Localized scope, clear acceptance, unchanged architecture, low coupling, cheap verification. |
| `FEATURE` | Multiple files/components coordinate, behavior needs investigation, steps depend on each other, architecture stays understood. |
| `PROJECT` | Architecture, shared contracts, migrations, persistence, security/trust, concurrency, recovery, major integrations, or major uncertainty. |
| `BATCH` | One request holds multiple independently completable outcomes. Orchestration, not a complexity level. |

Signals: ambiguity of completion, blast radius across subsystems or
contracts, coupling between areas, architectural novelty, failure cost
(cosmetic → recoverable → data-affecting → security-sensitive), and
testability. When torn between two modes, start lighter only if escalation
can catch a wrong assumption early.

## 3. Plan only as needed

- `PATCH`: no planning file. Hold the change in context.
- `FEATURE`: concise in-context plan — outcome/acceptance,
  components/contracts, sequence, validation strategy, known risks.
  Implement in small testable slices; re-evaluate after meaningful slices.
  Durable task memory only if the work genuinely spans sessions, models,
  agents, commits, or handoff boundaries.
- `PROJECT`: before coding, create `.agent/tasks/<task>.md` from
  `_TEMPLATE.md`, then investigate, record facts (not copied prose),
  define acceptance criteria, invariants, affected surfaces, phases,
  validation; adversarially review the plan (section 6); revise the file
  to hold resolved decisions.
- `BATCH`: triage the whole batch once — shared root causes,
  dependencies, conflicts, same-subsystem units — then classify each
  logical unit independently and execute one unit at a time. Never keep
  many unrelated implementations active in one context.

## 4. Implement

- Smallest coherent change that satisfies acceptance; no unrelated
  refactors. Product-source changes during process work are justified only
  for agent infrastructure itself (e.g. a docs checker).
- One `PROJECT` phase at a time; keep phases independently understandable
  and preferably independently sound.
- Prefer self-explanatory code; comment invariants, protocol quirks,
  trust assumptions, race defenses, compatibility workarounds.
- Parallelize discovery aggressively (mapping subsystems, locating
  contracts/tests, auditing assumptions, reviewing diffs); parallelize
  implementation conservatively. Never run simultaneous edits to coupled
  contracts, build consumers against an unsettled provider boundary, or
  mutate shared state from multiple agents. The coordinating agent owns
  architecture, ordering, synthesis, and final validation; subagent output
  is evidence, not accepted truth.

## 5. Validate

Scale validation with risk and acceptance criteria:

| Change | Required validation |
|---|---|
| Cosmetic / local UI | Targeted type/build/component check + direct appearance check where acceptance depends on it |
| State / logic bug | Targeted unit/component test + affected integration path |
| Contract / event / API / IPC | Producer + consumer validation, fixtures/contracts, type checking |
| Persistence / recovery / security / concurrency | Focused invariant/failure-path tests + actual restart/recovery behavior where relevant |
| Platform / runtime / integration | Targeted automation + narrowest platform/CI/integration/manual smoke |
| `PROJECT` completion | Cumulative integration validation + canonical gate + acceptance/platform checks |

Canonical gate (authoritative only under the pinned toolchain):

```bash
npm run verify
cargo test --manifest-path src-tauri/Cargo.toml
npm run build
```

The gate is necessary but not sufficient for behavior it does not
exercise. If acceptance depends on live UI, restart/recovery, OS/media
behavior, external runtimes, integrations, or deployment, run the
narrowest validation that directly exercises that surface. Whatever
cannot be exercised here is reported explicitly as unverified — a
timeout, skip, or truncated run without a receipt is inconclusive, never
a pass.

## 6. PROJECT plan review

Before `PROJECT` implementation, attack the plan:

- Are acceptance criteria complete and verifiable?
- Are architectural assumptions supported by source/tests?
- Is an existing abstraction bypassed or functionality duplicated?
- Are all affected contracts, tests, and compatibility concerns covered?
- Are persistence/security/race/recovery concerns covered where relevant?
- Is any phase too large to validate or revert independently?
- Is sequencing correct and scope no larger than necessary?
- Are any goals unverifiable?

Then update the plan itself with resolved decisions and concise
rationale. No review transcripts.

## 7. Escalation and anti-ceremony

Escalate — preserve verified work, summarize new evidence, revise
plan/mode, continue from known-good state — when: an architectural
assumption proves false; scope spreads materially; tests reveal an
unexpected cross-component dependency; two repair attempts fail the same
way; unrelated refactors become prerequisites; a new durable
contract/migration appears unexpectedly; edits lose a clear correctness
invariant; or context fills with stale logs and obsolete assumptions.

De-escalate the rest: no specs, proposals, task files, subagent trees, or
decompositions when direct execution is cheaper. Never create process
artifacts to demonstrate compliance. Compress obsolete planning detail
after decisions resolve.

## 8. Review and document

- Review the phase diff and commit series for accidental scope before
  every checkpoint.
- Update only canonical docs whose truth changed: architecture changed →
  architecture owner; responsibility/state changed → module owner;
  contract/surface changed → contract owner + synchronized inventories;
  operational behavior changed → operations owner; routing changed →
  `AGENTS.md`. Pure refactors with no durable change cause no doc churn.
- Cross-domain invariants link to their canonical owner; never maintain
  two rewritten versions of one invariant.

## 9. Commit and recover

`plan/review → phase/unit → validate → commit → next`. A commit is a
recovery checkpoint for independently sound work, not end-of-task
cleanup. Task bookkeeping may ride with the next logical commit unless a
standalone checkpoint materially improves recovery or handoff.

Git safety: own only changes you create. Inspect `git status` first,
preserve unrelated user work exactly, never reset/clean/stash/discard it
for a clean tree, never commit secrets or debris.

When architecture evolves, evolve the topology: split oversized
catch-alls, merge overlapping stubs, move facts to the new canonical
owner, update the router and checks. Git owns history; docs describe the
current system.
