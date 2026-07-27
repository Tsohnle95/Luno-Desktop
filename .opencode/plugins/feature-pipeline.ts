import { tool, type Plugin } from "@opencode-ai/plugin"

type WorkflowStatus = "PASS" | "FAIL" | "BLOCKED" | "NEEDS_INPUT"

type StageResult = {
  stage: string
  agent: string
  sessionID: string
  status: WorkflowStatus
  text: string
}

const STATUS_PATTERN =
  /(?:^|\n)\s*WORKFLOW_STATUS:\s*(PASS|FAIL|BLOCKED|NEEDS_INPUT)\s*$/im

function responseData<T>(result: any, operation: string): T {
  if (result?.error) {
    const message =
      typeof result.error === "string"
        ? result.error
        : JSON.stringify(result.error)
    throw new Error(`${operation} failed: ${message}`)
  }
  if (!result || !("data" in result)) {
    throw new Error(`${operation} returned no data`)
  }
  return result.data as T
}

function visibleText(parts: any[]): string {
  return parts
    .filter((part) => part?.type === "text" && typeof part.text === "string")
    .map((part) => part.text.trim())
    .filter(Boolean)
    .join("\n\n")
    .trim()
}

function parseStatus(text: string): WorkflowStatus | undefined {
  const match = text.match(STATUS_PATTERN)
  return match?.[1]?.toUpperCase() as WorkflowStatus | undefined
}

function stagePrompt(
  stage: string,
  request: string,
  context: string,
): string {
  const protocol = `
WORKFLOW PROTOCOL
- Complete only the assigned "${stage}" stage.
- Return a substantive visible-text report. Reasoning-only or empty output is invalid.
- Do not invoke another agent or hand off the task yourself.
- End the response with exactly one status line:
  WORKFLOW_STATUS: PASS
  WORKFLOW_STATUS: FAIL
  WORKFLOW_STATUS: BLOCKED
  WORKFLOW_STATUS: NEEDS_INPUT
- PASS means this stage completed successfully.
- FAIL means the stage found or produced a concrete failure that the scripted pipeline may send to Debugger.
- BLOCKED means the stage cannot continue because of tooling, provider, repository, or environmental failure.
- NEEDS_INPUT means one specific user decision or piece of evidence is required.
`.trim()

  const assignments: Record<string, string> = {
    Architect: `
Produce the architecture contract only. Do not inspect files or implement code.
Define the goal, non-goals, constraints, approach, interfaces/data flow, risks,
and acceptance criteria. If product intent is genuinely ambiguous, return one
focused question and NEEDS_INPUT instead of guessing.
`.trim(),
    Planner: `
Read project_brain.md first when present, then validate only relevant claims
against the repository. Do not edit files. Return a bounded, file-level plan,
dependencies, verification commands, and concrete risks. Use NEEDS_INPUT only
when repository evidence cannot resolve a decision.
`.trim(),
    Coder: `
Implement the approved architecture and plan. Preserve unrelated work. Run
bounded, deterministic verification; a timeout-killed command is not a pass.
Return changed files and actual command results. Use FAIL if implementation was
attempted but verification still fails.
`.trim(),
    Reviewer: `
Independently inspect the actual diff, relevant source, and tests. Report
actionable findings ordered by severity. PASS only when there are no actionable
findings and the acceptance criteria have credible verification. Always return
a visible report, even when there are no findings.
`.trim(),
    Debugger: `
Use the supplied failure evidence to reproduce or isolate the root cause, make
the smallest safe fix, and run focused regression checks. Do not broaden scope.
Use FAIL if the issue or verification failure remains.
`.trim(),
    "Brain Maintainer": `
Read project_brain.md maintenance instructions and inspect the final diff.
Update only project_brain.md, and only when architecture, ownership, data flow,
interfaces, dependencies, protocols, tests/run instructions, or documented
decisions changed. Otherwise report "Project brain unchanged" and PASS.
`.trim(),
  }
  const assignment =
    assignments[stage] ??
    (stage.endsWith("Reviewer") ? assignments.Reviewer : undefined)
  if (!assignment) {
    throw new Error(`No assignment prompt is defined for stage "${stage}"`)
  }

  return [
    protocol,
    `STAGE ASSIGNMENT\n${assignment}`,
    `ORIGINAL USER REQUEST\n${request}`,
    context ? `APPROVED PRIOR-STAGE CONTEXT\n${context}` : "",
  ]
    .filter(Boolean)
    .join("\n\n")
}

function reportSection(result: StageResult): string {
  const limit = 5000
  const text =
    result.text.length > limit
      ? `${result.text.slice(0, limit)}\n\n[Stage report truncated; inspect child session ${result.sessionID} for the complete output.]`
      : result.text
  return [
    `## ${result.stage} — ${result.status}`,
    `Agent: ${result.agent}`,
    `Session: ${result.sessionID}`,
    "",
    text,
  ].join("\n")
}

export const FeaturePipelinePlugin: Plugin = async ({ client, directory }) => {
  async function runStage(
    parentSessionID: string,
    request: string,
    stage: string,
    agent: string,
    context = "",
  ): Promise<StageResult> {
    const created = responseData<{ id: string }>(
      await client.session.create({
        body: {
          parentID: parentSessionID,
          title: `Feature Pipeline — ${stage}`,
        },
        query: { directory },
      }),
      `Create ${stage} session`,
    )

    const reply = responseData<{ parts: any[] }>(
      await client.session.prompt({
        path: { id: created.id },
        query: { directory },
        body: {
          agent,
          parts: [
            {
              type: "text",
              text: stagePrompt(stage, request, context),
            },
          ],
        },
      }),
      `Run ${stage}`,
    )

    const text = visibleText(reply.parts)
    if (!text) {
      throw new Error(
        `${stage} returned no visible text (reasoning-only/empty completion)`,
      )
    }

    const status = parseStatus(text)
    if (!status) {
      throw new Error(`${stage} omitted the required WORKFLOW_STATUS line`)
    }

    return {
      stage,
      agent,
      sessionID: created.id,
      status,
      text,
    }
  }

  async function runReviewer(
    parentSessionID: string,
    request: string,
    context: string,
    label: string,
  ): Promise<StageResult> {
    try {
      return await runStage(
        parentSessionID,
        request,
        label,
        "reviewer",
        context,
      )
    } catch (primaryError) {
      try {
        return await runStage(
          parentSessionID,
          request,
          label,
          "pipeline-reviewer-backup",
          [
            context,
            `PRIMARY REVIEWER FAILURE\n${String(primaryError)}`,
          ].join("\n\n"),
        )
      } catch (backupError) {
        throw new Error(
          `${label} failed with both reviewers.\nPrimary: ${String(
            primaryError,
          )}\nBackup: ${String(backupError)}`,
        )
      }
    }
  }

  return {
    tool: {
      feature_pipeline: tool({
        description:
          "Run the deterministic Architect → Planner → Coder → Reviewer → optional Debugger → final Reviewer → Brain Maintainer pipeline. Call exactly once with the user's complete request.",
        args: {
          request: tool.schema
            .string()
            .min(1)
            .describe("The user's complete feature or bug request"),
        },
        async execute({ request }, context) {
          const results: StageResult[] = []
          const parentSessionID = context.sessionID

          try {
            const architect = await runStage(
              parentSessionID,
              request,
              "Architect",
              "architect",
            )
            results.push(architect)
            if (architect.status !== "PASS") {
              return [
                "# Feature pipeline stopped at Architect",
                reportSection(architect),
              ].join("\n\n")
            }

            const planner = await runStage(
              parentSessionID,
              request,
              "Planner",
              "planner",
              architect.text,
            )
            results.push(planner)
            if (planner.status !== "PASS") {
              return [
                "# Feature pipeline stopped at Planner",
                ...results.map(reportSection),
              ].join("\n\n")
            }

            const coder = await runStage(
              parentSessionID,
              request,
              "Coder",
              "coder",
              [architect.text, planner.text].join("\n\n"),
            )
            results.push(coder)
            if (coder.status === "BLOCKED" || coder.status === "NEEDS_INPUT") {
              return [
                "# Feature pipeline stopped at Coder",
                ...results.map(reportSection),
              ].join("\n\n")
            }

            let reviewer = await runReviewer(
              parentSessionID,
              request,
              [architect.text, planner.text, coder.text].join("\n\n"),
              "Reviewer",
            )
            results.push(reviewer)

            const repairRequired =
              coder.status === "FAIL" || reviewer.status === "FAIL"

            if (
              reviewer.status === "BLOCKED" ||
              reviewer.status === "NEEDS_INPUT"
            ) {
              return [
                "# Feature pipeline blocked: independent review incomplete",
                ...results.map(reportSection),
              ].join("\n\n")
            }

            if (repairRequired) {
              const debuggerResult = await runStage(
                parentSessionID,
                request,
                "Debugger",
                "debugger",
                [coder.text, reviewer.text].join("\n\n"),
              )
              results.push(debuggerResult)
              if (debuggerResult.status !== "PASS") {
                return [
                  "# Feature pipeline stopped at Debugger",
                  ...results.map(reportSection),
                ].join("\n\n")
              }

              reviewer = await runReviewer(
                parentSessionID,
                request,
                [
                  architect.text,
                  planner.text,
                  coder.text,
                  debuggerResult.text,
                ].join("\n\n"),
                "Final Reviewer",
              )
              results.push(reviewer)
              if (reviewer.status !== "PASS") {
                return [
                  "# Feature pipeline blocked: final review did not pass",
                  ...results.map(reportSection),
                ].join("\n\n")
              }
            } else if (reviewer.status !== "PASS") {
              return [
                "# Feature pipeline blocked: review did not pass",
                ...results.map(reportSection),
              ].join("\n\n")
            }

            const brain = await runStage(
              parentSessionID,
              request,
              "Brain Maintainer",
              "brain-maintainer",
              results.map(reportSection).join("\n\n"),
            )
            results.push(brain)

            const completed = brain.status === "PASS"
            return [
              completed
                ? "# Feature pipeline completed"
                : "# Feature pipeline completed implementation but brain maintenance is blocked",
              ...results.map(reportSection),
            ].join("\n\n")
          } catch (error) {
            return [
              "# Feature pipeline blocked by an orchestration error",
              String(error),
              results.length
                ? `## Completed stages\n\n${results
                    .map(reportSection)
                    .join("\n\n")}`
                : "",
              "",
              "No later stage was substituted or silently skipped.",
            ]
              .filter(Boolean)
              .join("\n\n")
          }
        },
      }),
    },
  }
}
