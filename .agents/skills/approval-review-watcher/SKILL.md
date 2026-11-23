---
name: approval-review-watcher
description: >-
  Watch for Afterlife ApprovalGate arrivals and Governance Proposals via event-driven
  afterlife wait, perform non-destructive code reviews, and verify GitHub PR CI checks
  when a PR exists. Use when asked to monitor approvals, auto-review arriving jobs,
  or check PR CI status on approval arrivals.
---

# Approval Review Watcher

Event-driven runbook for monitoring Afterlife `ApprovalGate` arrivals and Governance Proposals, reviewing the corresponding branch without touching the working tree, and verifying GitHub PR CI status when a PR exists.

## Non-Negotiable Rules

1. **Event-Driven Waiting Only**: Always wait with `afterlife wait --since-seq <seq> ...` in a background task. Never poll in a loop or use shell `sleep`.
2. **Non-Destructive Inspection**: Assume other sessions may be actively editing branches or worktrees. Never run `git checkout`, `git switch`, or modify tracked files during review; inspect diffs strictly via read-only `git show`, `git diff`, `git ls-tree`, and `git grep`.
3. **Human Governance Boundary**: Never attempt to approve or rework `ApprovalGate` jobs or governance proposals as an agent. Present the review verdict to the human operator.
4. **Facts-Only Findings (No Prescribed Solutions)**: When reporting review issues or drafting a Rework note, state **only observed facts / anomalies** (what fails, leaks, diverges, or behaves inconsistently). Never prescribe implementation steps or lock in a specific `How`, so the implementing agent can resolve the root cause holistically from the district Philosophy.

## Workflow

### 1. Check Current State First

Fetch the current workspace cursor and active jobs:

```bash
afterlife jobs list
```

- Record `latest_seq`.
- If any unreviewed job is already at an `ApprovalGate` stage (or a pending proposal is awaiting review), immediately run Steps 3 through 6 for it before entering the idle wait state.

### 2. Start / Resume Event-Driven Watcher

Launch a one-shot background watcher starting from the latest processed sequence number:

```bash
afterlife wait --since-seq <latest_seq> --until job_transitioned,philosophy_proposed,spec_proposed,template_proposed,district_proposal_created
```

- Run as a non-daemon background command so the conversation wakes up automatically when a matching event arrives.
- When woken up:
  - Update `<latest_seq>` from the returned event's `seq`.
  - For `job_transitioned`, verify via `afterlife jobs list` or `afterlife jobs context <job_id>` whether the job's current stage is actually an `ApprovalGate`. If it is only an intermediate work stage transition, skip review and immediately re-arm `afterlife wait --since-seq <new_seq> ...`.
  - For an `ApprovalGate` arrival or any governance proposal event, execute Steps 3 through 6, then re-arm `afterlife wait` from the latest `seq` so monitoring continues seamlessly.

### 3. Locate Branch, Diff Range, and PR

1. Identify the candidate branch and commit range from `step_records` / artifacts and read-only git inspection:
   ```bash
   git branch -vv
   git log --all --graph --oneline -n 15
   ```
   Use `git merge-base main <branch>` as the diff base so unrelated commits on `main` do not pollute the review.
2. Check whether a GitHub PR exists for the branch:
   ```bash
   gh pr list --head <branch> --state all --json number,title,state,url,headRefOid,mergeable,isDraft
   ```

### 4. Verify PR and CI Checks

- **If a PR exists**:
  - Verify `headRefOid` matches the branch tip being reviewed (flag if unpushed local commits exist).
  - Inspect all required and optional checks:
    ```bash
    gh pr checks <pr_number>
    ```
  - If any check is failing, inspect the failed run logs (`gh run view <run_id> --log-failed`) to capture the exact failing job and observed failure cause.
  - If checks are still running (`pending` / `in_progress`), note them clearly instead of assuming pass.
- **If no PR exists**:
  - State clearly that no PR was found for `<branch>`.
  - If the branch is pushed to origin, optionally check recent branch workflow runs via `gh run list --branch <branch> --limit 5`.

### 5. Non-Destructive Code Review

Inspect the full diff (`git diff <base>..<branch>` and `git show <commit>`) against Afterlife's core invariants:

- **Correctness & Edge Cases**: Logic errors, concurrency/lease/TOCTOU gaps, input validation consistency, and regressions against existing behavior.
- **Authorization & Scope Isolation**: Fail-closed `Actor::Human` vs `Actor::Agent` boundaries, `ScopeSet` / district-scoped token filtering (both write authorization and read/list outputs), and per-owner isolation.
- **Persistence & Contract Sync**: Backward-compatible `#[serde(default)]` loading, sanitization of invalid/out-of-range values, atomic writes, and synchronization across domain rules, `schemas/mcp-tools.schema.json`, `crates/afterlife-daemon/src/mcp/schemas.rs`, CLI `toolmap`, and SSE event classification.
- **Code Hygiene & Portability**:
  - Zero comments in first-party Rust sources under `crates/` and `tools/`.
  - Every first-party `.rs` file within the 650-line budget (`module_size_guard`).
  - Zero test deletions or disconnected test modules; CI target names (such as `--test e2e_human_mcp`) preserved.
  - No machine-specific paths, idle redraw regressions, or hardcoded visual tokens bypassing `afterlife-design`.

### 6. Report to the Operator

Present a concise review summary containing:

1. **Verdict**: `Approve 可` / `Rework 推奨` / `CI 確認待ち`
2. **PR & CI Status**: PR link, merge/draft state, head SHA match, and check results (or explicit note that no PR exists).
3. **Observed Facts (only if issues exist)**: A copy-paste-ready Rework note listing **strictly observed anomalies and facts** without prescribing implementation approaches.
4. **Verified Points**: Brief confirmation of invariants checked, plus confirmation that the watcher has been re-armed from `<latest_seq>`.

### 7. Post-Merge Sync, Build, and Relaunch (Only When Operator Requests Merge)

Never merge on your own. Whenever the human operator explicitly asks to merge an approved PR or branch into `main`, always complete the full local delivery loop through app relaunch:

1. Merge the PR (`gh pr merge <pr_number> --merge`) or fast-forward/merge the branch into `main` and sync local `main` (`git pull --ff-only origin main`).
2. Verify the merged `main` passes the full suite (`./scripts/test-fast.sh`).
3. Release-build, package, and verify the desktop app signature (`./scripts/bundle-macos.sh --no-launch` followed by `codesign --verify --deep --strict dist/Afterlife.app`).
4. Stop any running `Afterlife` process and relaunch `dist/Afterlife.app` (or run `./scripts/bundle-macos.sh` directly) so the operator immediately sees the merged build live.
