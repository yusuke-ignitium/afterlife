---
name: approval-pr
description: Create a Pull Request and attach its URL as a pull_request artifact right before advancing a codebase-changing Afterlife job into a pre-delivery (merge/ship) human ApprovalGate. Do not trigger for upstream intermediate approvals (such as design, spec, or discovery approval gates) or for jobs without repository changes.
---

## Trigger Scope

Apply this skill **only** when BOTH conditions hold:
- The Afterlife job has implemented and verified codebase changes on a git branch ready for delivery.
- The next `ApprovalGate` (`afterlife jobs advance` target) is a **pre-delivery / pre-merge approval** (e.g., immediately preceding a terminal delivery stage such as `shipped`, `resolved`, `optimized`, or `hardened`).

**Do NOT apply** when:
- Moving into an upstream or intermediate `ApprovalGate` before delivery (such as design approval, architecture review, RFC/spec sign-off, or discovery approval) in a multi-gate District.
- The job has no repository code/config diffs (research, shaping, or non-code work).

## Required Steps (Before Advancing Into the Pre-Delivery ApprovalGate)

Human approval gates are fail-closed and reject agent artifact attachments once entered, so complete every step **before** the final `afterlife jobs advance` into `ApprovalGate`:

1. Commit cleanly on a dedicated feature branch following `.agents/skills/commit/SKILL.md` (stage only task files; never commit `dist/` or local artifacts).
2. Push the feature branch to the GitHub remote (`github` if configured, otherwise `origin`).
3. Open a Pull Request targeting the repository's default branch via `gh pr create --body-file - <<'EOF'` with a concise Why summary, structural changes, and Before/After verification results.
4. Attach the live PR URL to the Afterlife job before the final advance:
   `afterlife artifacts attach <job_id> --kind pull_request --label "PR #<number>" --uri "<pr_url>" --fencing-token <token>`
   and include its `artifact_id` (alongside the verification report) in `afterlife jobs advance`.
5. Include the PR link prominently in the handoff message to the human reviewer. Never approve the `ApprovalGate` or merge the PR as an agent.
