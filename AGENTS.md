# Afterlife Agent Guidelines

## 1. Concept

> **A local-first autonomous workflow control plane that structurally separates AI execution (`Actor::Agent`) from human governance (`Actor::Human`).**

- AI agents execute Jobs autonomously inside **Districts** governed by versioned **Philosophy** (values and decision criteria), **ProcedureSpec** (workflows and nested state machines), and **GateRubric** (verification checklists).
- Agents acquire exclusive stage leases (`fencing_token`), record passing `GateVerification` results and `Artifact` evidence, and **always stop at human `ApprovalGate` boundaries**.
- Structured `Feedback` captured from human approvals and rework drives controlled, versioned evolution of both `Philosophy` and `ProcedureSpec`.

---

## 2. Philosophy & Principles

### 1. Ship Every State as a Production-Grade Whole
- Never bolt features on through patchwork or incremental hacks.
- Maintain the codebase so that **the current state is always a complete, production-grade commercial product**.
- When adding or changing capabilities, select the approach that **best fits the product's UX, visual language, reliability, and performance as a unified whole**, refactoring the foundation whenever needed rather than layering workarounds.

### 2. Physical Separation of Governance (`Actor::Human`) and Execution (`Actor::Agent`)
- Delegate execution power aggressively to AI agents, while never surrendering approval authority or governance control.
- Enforce the human/agent boundary through **fail-closed defense-in-depth** across authentication, the domain engine, and persistence — never through prompt-level reminders alone.

### 3. Explicit Separation of Philosophy and Procedure
- Separate enduring values (principles, anti-patterns, heuristics) from operational execution (stages, transitions, checklists).
- Compile the authoritative execution context dynamically at runtime so any connected model or agent can resume work with consistent quality.

### 4. Deterministic Guards Over Prose
- Encode rules in types, design tokens, schema synchronization checks, and automated black-box tests rather than relying on manual vigilance.
- Keep first-party Rust code free of comments; express intent strictly through clear types, naming, structure, and tests. Treat `afterlife toolmap` as the single live contract for agents.

### 5. Local-First Purity with Cloud Isomorphism
- Keep the core domain engine free of I/O so local execution and cloud deployment share identical business logic behind swappable port traits.
- Treat the local desktop application (`dist/Afterlife.app`) as a first-class product with zero compromise in polish or reliability.

### 6. Calm, Tactile Visuals Without Performance Compromise
- Pursue a quiet, cohesive control-room aesthetic (Liquid Glass, restrained motion, clear visual hierarchy) built for sustained daily focus.
- Never trade performance or battery life for visuals: guarantee zero idle redraws, compute expensive effects strictly on demand, and keep interactions immediately responsive.

---

## 3. Do

- **Delegate autonomous execution and proposals to AI agents**
  - Let agents queue, claim, heartbeat, advance sub-stages, attach artifacts, record gate verifications, rework failures, and propose versioned `Philosophy` or `ProcedureSpec` updates from accumulated feedback.
- **Evolve the design holistically to fit the product**
  - Align tokens, state management, rendering, and domain models with the whole product whenever requirements evolve instead of adding one-off branches.
- **Make every rule machine-verifiable**
  - Back authorization boundaries, concurrency guarantees, visual token consistency, schema sync, and config sanitization with deterministic automated tests.
- **Sweat daily ergonomics and state continuity**
  - Preserve operator state seamlessly across restarts (window geometry, panel sizing, visual preferences) and integrate cleanly with external tools via safe deep links.
- **Keep interactive demos first-class and committed**
  - Build every UI, visual, or interaction demo as a runnable binary under `tools/demos/` (`./scripts/run-demo.sh`) and commit it alongside the change it validates rather than leaving throwaway scratch outside the repository.

---

## 4. Don't

- **Never leave patchwork, workarounds, or local-only hacks**
  - Do not introduce ad-hoc overrides, scattered hardcoded colors or dimensions, or fragile state patches that fight the surrounding architecture.
- **Never weaken authorization, concurrency, or idempotency boundaries**
  - Never allow AI agents to bypass `ApprovalGate` stages or approve governance proposals.
  - Never bypass lease fencing (`fencing_token`), optimistic concurrency checks, command idempotency, or per-owner data isolation.
- **Never write comments in first-party Rust code**
  - Do not add `//`, `///`, `//!`, or `/* ... */` comments in first-party Rust crates or tools. When code feels hard to follow, improve its types, decomposition, names, and tests instead.
- **Never allow drift between implementation and contracts**
  - Do not let domain rules, MCP schemas, `afterlife toolmap`, or shared design defaults diverge. Avoid maintaining parallel prose manuals that rot over time.
- **Never trade stability or efficiency for visual effects**
  - Do not introduce idle frame polling, unmeasured rendering regressions, or persistence logic that breaks on missing fields, `NaN`, out-of-range inputs, or concurrent writes.
- **Never commit local-machine-specific or organization-specific artifacts**
  - Keep proprietary company data, credentials, private URLs, home directory paths, hostnames, OS temp or sandbox cache paths, and machine-specific paths out of all tracked files, scripts, tests, and benchmarks.

---

## 5. Non-Negotiable Invariants

These properties must hold continuously on every commit and release:

1. **Cohesive Production Quality**
   - Every change leaves UX, visual craft, error resilience, and responsiveness integrated as a single finished product.
2. **Fail-Closed Authorization & Tenant Isolation**
   - Human-vs-agent boundaries and per-owner isolation remain enforced across authentication, domain logic, and storage.
3. **Strict Concurrency Correctness**
   - Zero double-claims, zero zombie writes after lease expiration, and exactly-once command idempotency remain proven by automated tests.
4. **Zero Contract Drift**
   - Domain rules, MCP tool schemas, CLI contracts (`afterlife toolmap`), and shared default tokens stay continuously synchronized.
5. **Code Hygiene**
   - Zero comments in first-party Rust sources, zero compiler/lint warnings, and a 100% passing test suite.
6. **Resilient, Backward-Compatible Persistence**
   - Persisted local state always uses atomic writes, sanitizes invalid values, and loads older formats without data loss.
7. **Visual Craft & Power Efficiency Together**
   - Zero wasted animation frames at idle and no unverified regression in frame time or CPU/GPU overhead.
8. **Reproducible `dist/Afterlife.app` Packaging**
   - Desktop changes remain verified, release-built, and signed into a runnable `dist/Afterlife.app`.
9. **Full Portability**
   - The repository builds, tests, and runs cleanly regardless of directory location, host machine, or deployment target.

---

## 6. Zero Code Comments Policy

- Never write comments (`//`, `///`, `//!`, `/* ... */`) anywhere in first-party Rust files under `crates/` or `tools/`.
- Express intent exclusively through clear types, function/variable names, enums, constants, and black-box tests.
- Agent-facing contracts live strictly in `afterlife toolmap` (immutable 3-line English concept + live MCP schemas + domain policy limits).
- This rule is enforced deterministically on every `cargo test` and CI run by `crates/afterlife-core/tests/no_comments_guard.rs`.

---

## 7. Interactive Demos

- Always place interactive prototypes, visual studies, and behavior-comparison demos under `tools/demos/`, registered in `tools/demos/Cargo.toml` and launchable via `./scripts/run-demo.sh <name>`.
- Always include demos created during design or verification in git commits alongside their corresponding changes; never leave demos as untracked scratch files, discarded local scripts, or standalone external artifacts.
- Because demos live under `tools/`, they follow the same zero-comments, zero-warning, and portability standards as all first-party crates.

---

## 8. Commit Convention

- Whenever creating a git commit in this repository (including chat requests, `/commit`, and diff-tab commit-and-push actions), always read and follow `.agents/skills/commit/SKILL.md`.
- This project-level commit skill takes precedence over any global commit skill.
- Never include intermediate exploration logs, trial-and-error history, or machine-specific paths in commit messages.

---

## 9. Pre-Delivery Approval PR Workflow

- Whenever advancing a codebase-changing Afterlife job into a **pre-delivery (merge/ship) `ApprovalGate`** (after implementation and verification are complete), always read and follow `.agents/skills/approval-pr/SKILL.md` before the final `afterlife jobs advance`.
- Do **not** apply this for upstream/intermediate approval gates (such as design or spec sign-off in multi-gate Districts) or for non-code jobs.
