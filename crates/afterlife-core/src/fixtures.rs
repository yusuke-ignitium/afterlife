use std::collections::HashMap;

use crate::{
    ids::{
        DistrictId, OwnerId, PhilosophyVersion, SpecVersion, StageId, SubMachineTemplateId,
        SubStageId,
    },
    model::{
        Actor, ArtifactKind, District, GateRubric, Job, Philosophy, Principle, ProcedureSpec,
        RunnerKind, StageDef, StageKind, StepFieldDef, SubMachineTemplate, SubStageDef,
    },
};

pub const LOCAL_OWNER_ID: &str = "local-owner";
const SEED_TS: u64 = 1_763_600_000_000;

pub struct WorkspaceSeed {
    pub templates: HashMap<SubMachineTemplateId, SubMachineTemplate>,
    pub template_order: Vec<SubMachineTemplateId>,
    pub districts: Vec<District>,
    pub jobs: Vec<Job>,
    pub next_job_seq: u32,
    pub next_fencing_token: u64,
}

pub fn default_workspace(owner_id: &OwnerId) -> WorkspaceSeed {
    let seed_actor = Actor::local_human(owner_id.clone());

    let mut templates = HashMap::new();

    let tmpl_evidence = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-evidence"),
        name: "Evidence & Probe".into(),
        description:
            "Trace primary specs and existing implementation, validate uncertainties in isolation, and lock criteria and assumptions."
                .into(),
        philosophy_notes:
            "Never design on guesswork. Establish facts from primary sources and isolated probes before committing to work."
                .into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("trace_sources"),
                label: "Trace Sources".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "Review existing implementation, related symbols, official documentation, and constraints exhaustively from primary sources."
                        .into(),
                gate_checks: vec![
                    "Related symbols and impact scope are confirmed from primary sources".into(),
                    "External specs and dependency constraints are backed by primary references"
                        .into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "sources_read",
                        "Primary Sources Reviewed",
                        "Record the exact implementation locations, specifications, and contracts inspected.",
                    ),
                    StepFieldDef::required(
                        "findings",
                        "Confirmed Facts & Constraints",
                        "Record the structures, dependencies, and invariants confirmed from primary sources.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
            SubStageDef {
                id: SubStageId::from("isolate_probe"),
                label: "Isolate Probe".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "Verify uncertain behavior, compatibility, or performance hypotheses in a temporary environment or minimal reproduction to eliminate assumptions."
                        .into(),
                gate_checks: vec![
                    "Uncertain points are confirmed through live probing or empirical measurement"
                        .into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "probe_method",
                        "Probe Method & Conditions",
                        "Record the isolated verification or minimal reproduction procedure and execution conditions.",
                    ),
                    StepFieldDef::required(
                        "observed_behavior",
                        "Observed Behavior",
                        "Record empirically observed behavior and conclusions rather than speculation.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
            SubStageDef {
                id: SubStageId::from("lock_criteria"),
                label: "Lock Criteria".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "Concisely lock in-scope items, non-goals, assumptions, acceptance criteria, and non-negotiable invariants."
                        .into(),
                gate_checks: vec![
                    "Scope, assumptions, acceptance criteria, and invariants are explicitly stated"
                        .into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "decision",
                        "Locked Scope & Assumptions",
                        "Record in-scope items, non-goals, assumptions, and rationale.",
                    ),
                    StepFieldDef::required(
                        "acceptance_criteria",
                        "Acceptance Criteria & Invariants",
                        "Record completion criteria and invariants that must never be broken.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
        ],
    };

    let tmpl_observe = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-observe"),
        name: "Observe & Instrument".into(),
        description:
            "Map target execution paths, instrument unobservable segments, and lock a reproducible evaluation metric specification."
                .into(),
        philosophy_notes:
            "Never guess about unmeasured segments. Make them reproducibly measurable before capturing baselines."
                .into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("map_pipeline"),
                label: "Map Pipeline".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "Inventory the target workload's execution paths, state transitions, and resource consumption points (both active and idle) from the primary implementation."
                        .into(),
                gate_checks: vec![
                    "Target paths and unobservable segments are identified from primary code"
                        .into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "pipeline_map",
                        "Execution Paths & Transitions",
                        "Record the processing paths and active/idle behavior of the target workload.",
                    ),
                    StepFieldDef::required(
                        "hot_paths",
                        "Cost Points & Blind Spots",
                        "Identify major resource consumption points and segments that cannot yet be measured.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
            SubStageDef {
                id: SubStageId::from("add_instrumentation"),
                label: "Add Instrumentation".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "For any unobservable bottleneck segment, add reproducible instrumentation or a measurement harness without degrading production behavior or quality."
                        .into(),
                gate_checks: vec![
                    "Previously unobservable segments can be measured repeatedly under identical conditions"
                        .into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "execution",
                        "Instrumentation & Harness Work",
                        "Record the measurement setup added or verified and confirm zero interference with production quality.",
                    ),
                    StepFieldDef::required(
                        "metric_hooks",
                        "Exposed Metrics",
                        "Record which quantitative metrics can now be captured reproducibly at each segment.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
            SubStageDef {
                id: SubStageId::from("lock_metric_spec"),
                label: "Lock Metric Spec".into(),
                runner: "Verifier".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure:
                    "Define and lock the evaluation metrics, benchmark workload, environmental conditions, and zero-regression quality floor."
                        .into(),
                gate_checks: vec![
                    "Evaluation metrics, measurement conditions, and non-regression quality floors are documented"
                        .into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "decision",
                        "Target Metrics & Quality Floor",
                        "Record the optimization target metrics and the quality/accuracy floor that must never degrade.",
                    ),
                    StepFieldDef::required(
                        "measurement_protocol",
                        "Reproducible Measurement Protocol",
                        "Record the fixed workload, inputs, and execution conditions used for Before/After comparison.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
        ],
    };

    let tmpl_baseline_guard = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-baseline-guard"),
        name: "Baseline & Guard".into(),
        description:
            "Capture Before baselines and lock deterministic regression guards prior to starting primary changes."
                .into(),
        philosophy_notes:
            "Lock Before baselines and regression guards ahead of the primary change to structurally prevent weak after-the-fact validation."
                .into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("capture_baseline"),
                label: "Capture Baseline".into(),
                runner: "Verifier".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure:
                    "Record reproducible steps, quality indicators, and performance or output Before baselines on the pre-change state so After results can be compared under identical conditions."
                        .into(),
                gate_checks: vec![
                    "Pre-change (Before) comparison baselines and reproduction conditions are recorded"
                        .into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "baseline_metrics",
                        "Before Baseline Measurements",
                        "Record the quantitative measurements, outputs, and observed behavior captured before changes.",
                    ),
                    StepFieldDef::required(
                        "conditions",
                        "Reproduction Conditions & Steps",
                        "Record exact commands, inputs, and environment assumptions required for an identical After comparison.",
                    ),
                ],
                required_artifact_kinds: vec![
                    ArtifactKind::Snapshot,
                    ArtifactKind::VerificationReport,
                ],
            },
            SubStageDef {
                id: SubStageId::from("lock_regression_guards"),
                label: "Lock Guards".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "Before modifying the primary scope, implement deterministic regression guards (automated tests or verifiers) covering existing quality, invariants, and boundaries, and confirm they pass on the current state."
                        .into(),
                gate_checks: vec![
                    "Regression guards are implemented ahead of primary changes and pass on the current state"
                        .into(),
                    "Guards detect intentional regressions and boundary violations".into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "guard_scope",
                        "Locked Regression Guard Scope",
                        "Record the invariants, boundary conditions, and existing behaviors protected ahead of change.",
                    ),
                    StepFieldDef::required(
                        "execution",
                        "Guard Implementation & Pre-Change Pass",
                        "Record the automated checks added or verified and their passing results on the current state.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
        ],
    };

    let tmpl_red_repro = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-red-repro"),
        name: "Red Repro & Guard".into(),
        description:
            "Lock a deterministic failing reproduction test (Red) and verify adjacent regression guards before applying any fix."
                .into(),
        philosophy_notes:
            "A bug that cannot be reproduced cannot be reliably fixed. Lock a failing deterministic test and surrounding guards before touching production code."
                .into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("capture_failure"),
                label: "Capture Failure".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "Reproduce and record the minimal trigger conditions, inputs, environment preconditions, and observed failure behavior (Before)."
                        .into(),
                gate_checks: vec![
                    "Minimal reproduction conditions and current failure behavior are recorded"
                        .into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "repro_steps",
                        "Minimal Reproduction Steps",
                        "Record the minimal inputs, state, and steps that deterministically reproduce the defect.",
                    ),
                    StepFieldDef::required(
                        "observed_failure",
                        "Observed Failure Behavior",
                        "Record factual differences from expected behavior, exact errors, and anomalous outputs.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
            SubStageDef {
                id: SubStageId::from("lock_red_test"),
                label: "Lock Red Test".into(),
                runner: "Verifier".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure:
                    "Implement a deterministic automated reproduction test that reliably fails (Red) prior to the fix, and verify it fails for the expected reason."
                        .into(),
                gate_checks: vec![
                    "Reproduction test fails (Red) for the expected reason on pre-fix code".into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "execution",
                        "Added Red Reproduction Test",
                        "Record the test location, assertions, and command to execute it.",
                    ),
                    StepFieldDef::required(
                        "red_test_result",
                        "Pre-Fix Red Failure Output",
                        "Record a concise summary of the expected failure observed on current code.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
            SubStageDef {
                id: SubStageId::from("lock_boundary_guards"),
                label: "Boundary Guards".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "Inspect adjacent happy-path and compatibility boundary guards at risk from the fix; reinforce any gaps and confirm they pass on current code."
                        .into(),
                gate_checks: vec![
                    "Adjacent happy-path and compatibility guards pass on the current state".into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "guard_scope",
                        "Adjacent Guard Scope",
                        "Record nearby behaviors and compatibility boundaries sensitive to the planned fix.",
                    ),
                    StepFieldDef::required(
                        "execution",
                        "Guard Check & Pre-Fix Pass",
                        "Record existing or newly added guards and their passing pre-fix results.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
        ],
    };

    let tmpl_build_loop = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-build-loop"),
        name: "Build & Hygiene".into(),
        description:
            "Apply cohesive foundational changes under pre-locked guards, strengthen edge/error tests, and iterate until all checks pass with zero warnings."
                .into(),
        philosophy_notes:
            "Refine the foundation without breaking pre-locked guards, and never settle until every check passes with zero warnings."
                .into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("apply_change"),
                label: "Apply Change".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "Implement the primary change cohesively according to the design, diagnosis, or pruning plan while preserving architecture, tokens, and persistence compatibility. Avoid appending unrelated logic to oversized files; extract focused modules or service boundaries first."
                        .into(),
                gate_checks: vec![
                    "Changes integrate cleanly with the whole system without patchwork or ad-hoc overrides"
                        .into(),
                    "No blind accretion onto oversized files; module responsibilities stay cleanly separated"
                        .into(),
                    "No machine-specific paths or secrets are introduced".into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "decision",
                        "Implementation Approach & Module Boundaries",
                        "Record the cohesive structure chosen over ad-hoc patches, module/service splits considered, and how invariants are preserved.",
                    ),
                    StepFieldDef::required(
                        "execution",
                        "Changes Applied",
                        "Record modified targets, key structural changes, and compatibility considerations.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
            SubStageDef {
                id: SubStageId::from("edge_guard_tests"),
                label: "Edge & Guard Tests".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "Add tests covering happy paths, error paths, and boundary conditions for the changed scope, and verify pre-locked reproduction tests and regression guards pass unmodified."
                        .into(),
                gate_checks: vec![
                    "Automated tests cover happy paths, error paths, and boundary cases for the change"
                        .into(),
                    "Pre-locked reproduction tests and regression guards pass without modification"
                        .into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "cases_added",
                        "Edge & Error Tests Added",
                        "Record added or verified happy-path, error, and boundary cases alongside pre-locked guard status.",
                    ),
                    StepFieldDef::required(
                        "test_results",
                        "Test Execution Results",
                        "Record execution results for both pre-locked guards and newly added tests.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
            SubStageDef {
                id: SubStageId::from("hygiene_gate"),
                label: "Hygiene Gate".into(),
                runner: "Build & Test".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure:
                    "Run all tests (including pre-locked guards and module-size/policy guards), builds, and linters, iterating until every check is green with zero warnings."
                        .into(),
                gate_checks: vec![
                    "All tests, builds, lint checks, and structural/policy guards pass with zero warnings"
                        .into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "checks_run",
                        "Verification Suite Executed",
                        "Record the full list of tests, builds, static analyses, and policy guards executed.",
                    ),
                    StepFieldDef::required(
                        "results",
                        "Zero-Warning Green Confirmation",
                        "Record zero-failure, zero-warning results and the outcome of any fix iterations.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
        ],
    };

    let tmpl_perf_loop = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-perf-loop"),
        name: "Measure & Optimize".into(),
        description:
            "Identify dominant bottlenecks from profiles, apply principled structural improvements, and remeasure under identical conditions."
                .into(),
        philosophy_notes:
            "Forbid blind tuning and quality-cutting shortcuts; iterate on principled structural improvements targeting dominant bottlenecks with same-condition remeasurement."
                .into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("profile_bottleneck"),
                label: "Profile Hotspots".into(),
                runner: "Verifier".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure:
                    "Pinpoint dominant bottlenecks from empirical profiles and instrumentation data, and document the target and expected gain hypothesis."
                        .into(),
                gate_checks: vec![
                    "Dominant bottlenecks are identified from empirical profile data".into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "bottleneck",
                        "Dominant Bottleneck Identified",
                        "Record the primary cost centers and their shares from profile and instrumentation data.",
                    ),
                    StepFieldDef::required(
                        "hypothesis",
                        "Optimization Hypothesis & Rationale",
                        "Record why improving this hotspot moves the metric and how quality remains uncompromised.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
            SubStageDef {
                id: SubStageId::from("apply_principled_change"),
                label: "Optimize Core".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "Implement structural or algorithmic improvements with zero degradation to accuracy, visual quality, UX, or safety."
                        .into(),
                gate_checks: vec![
                    "No ad-hoc shortcuts that sacrifice quality, functionality, or accuracy".into(),
                    "No machine-specific paths or leftover profiling scaffolding".into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "decision",
                        "Structural Optimization & Quality Guard",
                        "Record the chosen approach for gaining performance without losing quality, plus shortcuts rejected.",
                    ),
                    StepFieldDef::required(
                        "execution",
                        "Optimization Implemented",
                        "Record modified components and algorithmic or data-structure improvements.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
            SubStageDef {
                id: SubStageId::from("remeasure_and_guard"),
                label: "Remeasure & Guard".into(),
                runner: "Build & Test".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure:
                    "Verify all pre-locked quality guards, tests, and linters pass with zero warnings, then remeasure under the exact Before conditions to quantify the gain."
                        .into(),
                gate_checks: vec![
                    "Pre-locked quality guards, full tests, builds, and lints pass with zero warnings"
                        .into(),
                    "Same-condition After remeasurement results are recorded".into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "before_after_metrics",
                        "Same-Condition Remeasurement",
                        "Record After values measured under identical Before conditions and the resulting delta.",
                    ),
                    StepFieldDef::required(
                        "guard_results",
                        "Quality Guard & Suite Results",
                        "Record passing zero-warning results for quality guards, full tests, and static checks.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
        ],
    };

    let tmpl_verify_ship = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-verify-ship"),
        name: "Verify & Diff".into(),
        description:
            "Run live production-equivalent verification, compare Before/After baselines, and package evidence and Why for human review."
                .into(),
        philosophy_notes:
            "Never ship on 'it should work.' Prove zero regression with live checks and same-condition Before/After diffs, then hand humans only the evidence needed to decide."
                .into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("live_verify"),
                label: "Live Run".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "Verify real user flows, persistence, and restart behavior on a live or production-equivalent build and E2E path."
                        .into(),
                gate_checks: vec![
                    "Live or production-equivalent verification is complete".into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "execution",
                        "Live Verification Procedure",
                        "Record the build, launch, user flow, and persistence verification steps executed.",
                    ),
                    StepFieldDef::required(
                        "observed_behavior",
                        "Observed Live Behavior",
                        "Record verified runtime behavior and factual confirmation of zero anomalies.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
            SubStageDef {
                id: SubStageId::from("compare_baseline"),
                label: "Before/After Diff".into(),
                runner: "Verifier".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure:
                    "Compare After measurements against the recorded Before baseline under identical conditions to prove zero regression in quality, performance, and compatibility."
                        .into(),
                gate_checks: vec![
                    "Same-condition Before/After comparison confirms zero quality, performance, or compatibility regressions"
                        .into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "before_after_diff",
                        "Same-Condition Before/After Diff",
                        "Record side-by-side comparison and deltas between recorded Before baselines and After measurements.",
                    ),
                    StepFieldDef::required(
                        "regression_check",
                        "Zero-Regression Justification",
                        "Record the concrete basis confirming no regressions across quality, performance, or compatibility.",
                    ),
                ],
                required_artifact_kinds: vec![],
            },
            SubStageDef {
                id: SubStageId::from("bundle_evidence"),
                label: "Bundle Evidence".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "Package the Before/After comparison, evidence artifacts, and Why summary (design decisions, compatibility, invariants) for handoff to the human approval gate."
                        .into(),
                gate_checks: vec![
                    "Before/After evidence artifacts and Why summary are ready for human approval"
                        .into(),
                ],
                record_fields: vec![
                    StepFieldDef::required(
                        "summary",
                        "Why Summary for Human Review",
                        "Summarize intent, key design decisions, compatibility impact, and preserved invariants.",
                    ),
                    StepFieldDef::required(
                        "evidence_refs",
                        "Attached Evidence & Review Points",
                        "List attached evidence artifacts and the key points for the human owner to review.",
                    ),
                ],
                required_artifact_kinds: vec![ArtifactKind::VerificationReport],
            },
        ],
    };

    let template_order = vec![
        tmpl_evidence.id.clone(),
        tmpl_observe.id.clone(),
        tmpl_baseline_guard.id.clone(),
        tmpl_red_repro.id.clone(),
        tmpl_build_loop.id.clone(),
        tmpl_perf_loop.id.clone(),
        tmpl_verify_ship.id.clone(),
    ];
    templates.insert(tmpl_evidence.id.clone(), tmpl_evidence);
    templates.insert(tmpl_observe.id.clone(), tmpl_observe);
    templates.insert(tmpl_baseline_guard.id.clone(), tmpl_baseline_guard);
    templates.insert(tmpl_red_repro.id.clone(), tmpl_red_repro);
    templates.insert(tmpl_build_loop.id.clone(), tmpl_build_loop);
    templates.insert(tmpl_perf_loop.id.clone(), tmpl_perf_loop);
    templates.insert(tmpl_verify_ship.id.clone(), tmpl_verify_ship);

    let delivery_philosophy = Philosophy {
        version: PhilosophyVersion::new(1),
        summary:
            "General-purpose product delivery district anchored in primary evidence, isolated probes, pre-change baselines and regression guards, zero patchwork, and commercial-grade completeness at every human approval boundary."
                .into(),
        principles: vec![
            Principle {
                id: "P-DEV-1".into(),
                title: "Ship Every State as a Whole".into(),
                statement:
                    "Never stack quick fixes or ad-hoc hacks; align the foundation when requirements evolve so the product stays commercially complete at every state."
                        .into(),
                rationale:
                    "Accumulated local patches erode architecture into unmaintainable branches, degrading quality and velocity together."
                        .into(),
            },
            Principle {
                id: "P-DEV-2".into(),
                title: "Primary Evidence & Live Probe First".into(),
                statement:
                    "Never proceed on guesswork. Verify official specs and primary code first, then validate uncertainties via isolated probes and quantitative measurement."
                        .into(),
                rationale:
                    "Designing on flawed assumptions or tuning by intuition causes costly rework and hidden regressions."
                        .into(),
            },
            Principle {
                id: "P-DEV-3".into(),
                title: "Baseline & Regression Guards Before Implementation".into(),
                statement:
                    "Record Before baselines and lock deterministic regression guards before touching the primary implementation scope."
                        .into(),
                rationale:
                    "After-the-fact tests tend to rubber-stamp new behavior, undermining objective Before/After comparison and regression detection."
                        .into(),
            },
            Principle {
                id: "P-DEV-4".into(),
                title: "Minimal Invariants, Single Source of Truth".into(),
                statement:
                    "Eliminate redundant manuals and duplicated definitions; keep minimal invariants in a single source of truth to prevent drift across specs, code, and contracts."
                        .into(),
                rationale:
                    "Bloated prose and duplicated configuration rot quickly and mislead both agents and human operators."
                        .into(),
            },
            Principle {
                id: "P-DEV-5".into(),
                title: "Fail-Closed Governance & Resilient State".into(),
                statement:
                    "Delegate execution aggressively to AI while stopping final approvals and destructive decisions at human gates; enforce atomic writes, input sanitization, and backward compatibility in persistence."
                        .into(),
                rationale:
                    "Maximizes autonomous execution speed while structurally preventing data corruption, authority bypass, and leakage of local paths or secrets."
                        .into(),
            },
        ],
        anti_patterns: vec![
            "Leaving patchwork or ad-hoc overrides that only mask symptoms".into(),
            "Deciding architecture or optimizations without primary sources or empirical backing"
                .into(),
            "Starting primary implementation before locking Before baselines and regression guards"
                .into(),
            "Advancing or requesting approval without passing builds, tests, lints, and Before/After checks"
                .into(),
            "Bypassing a human approval gate on an agent's own authority".into(),
            "Leaving trial-and-error logs, machine-specific paths, or secrets in artifacts or commits"
                .into(),
        ],
        decision_heuristics: vec![
            "When specs or behavior are ambiguous, confirm facts in an isolated probe (temporary directory or minimal reproduction) before proceeding"
                .into(),
            "Lock Before baselines and deterministic regression guards prior to primary edits, then prove zero regression via same-condition comparison"
                .into(),
            "When requirements shift, refine the shared foundation, tokens, or abstraction boundary rather than patching the surface"
                .into(),
            "Keep changes in cohesive logical units and capture the non-obvious Why (design tradeoffs, compatibility, invariants) in commits"
                .into(),
        ],
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
    };

    let delivery_spec = ProcedureSpec {
        version: SpecVersion::new(1),
        changelog: "Initial general-purpose product delivery procedure spec".into(),
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
        stages: vec![
            StageDef {
                id: StageId::from("discover"),
                label: "Discover & Probe".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Trace existing code, primary specifications, and impact scope; validate open questions in isolation, then lock scope and invariants."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Related symbols, impact scope, and primary specifications are confirmed"
                            .into(),
                        "In-scope targets, non-goals, and non-negotiable invariants are clearly stated"
                            .into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-evidence")),
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("design")],
                rework_to: None,
            },
            StageDef {
                id: StageId::from("design"),
                label: "Design Whole".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Define a cohesive approach aligned with product architecture, module boundaries, types, backward compatibility, and UX—extracting focused modules rather than appending to god files—along with a Before/After and regression-guard plan."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Design fits the whole system cohesively with clear module boundaries rather than accretion or patchwork"
                            .into(),
                        "Persistence, backward compatibility, error paths, and boundaries are addressed"
                            .into(),
                        "Concrete plan exists for Before baselines and pre-change regression guards"
                            .into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: None,
                record_fields: vec![
                    StepFieldDef::required(
                        "decision",
                        "Chosen Design & Module Boundaries",
                        "Record the chosen architecture, responsibility boundaries or splits, and why it fits overall types, UX, and compatibility.",
                    ),
                    StepFieldDef::required(
                        "alternatives_rejected",
                        "Rejected Alternatives & Rationale",
                        "Record considered alternatives or patchwork options and why they were rejected.",
                    ),
                    StepFieldDef::required(
                        "invariants",
                        "Invariants, Compatibility & Guard Plan",
                        "Record invariants to preserve, backward compatibility rules, and the baseline/guard plan.",
                    ),
                ],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("baseline_guard")],
                rework_to: Some(StageId::from("discover")),
            },
            StageDef {
                id: StageId::from("baseline_guard"),
                label: "Baseline & Guard".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Before primary implementation, capture pre-change (Before) baselines and implement deterministic regression guards protecting existing quality and invariants."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Pre-change (Before) comparison baselines and reproduction conditions are recorded"
                            .into(),
                        "Pre-change regression guards are implemented and pass on the current state"
                            .into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-baseline-guard")),
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("build")],
                rework_to: Some(StageId::from("design")),
            },
            StageDef {
                id: StageId::from("build"),
                label: "Build Core".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Implement the primary scope cohesively without breaking pre-locked guards, add tests for new behavior, and pass builds, full tests, lints with zero warnings, and portability checks."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Pre-locked regression guards and new specification tests all pass".into(),
                        "Builds, full tests, and lints pass with zero warnings".into(),
                        "No local-only paths, secrets, or unrelated diffs are present".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-build-loop")),
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("verify")],
                rework_to: Some(StageId::from("design")),
            },
            StageDef {
                id: StageId::from("verify"),
                label: "Verify & Diff".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Verify on a live or production-equivalent build and E2E path, prove zero regression against the Before baseline under identical conditions, and attach evidence artifacts and a Why summary."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Evidence exists for live or production-equivalent verification".into(),
                        "Same-condition Before/After comparison confirms zero quality, performance, or compatibility regressions"
                            .into(),
                        "Evidence artifacts and Why summary are complete".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-verify-ship")),
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("approve")],
                rework_to: Some(StageId::from("build")),
            },
            StageDef {
                id: StageId::from("approve"),
                label: "Human Gate".into(),
                runner: "You".into(),
                runner_kind: RunnerKind::Human,
                kind: StageKind::ApprovalGate,
                procedure:
                    "Human owner reviews Before/After results, evidence artifacts, and change soundness, then either Approves or triggers Rework with a concrete reason."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Satisfies District Philosophy principles and commercial-grade quality"
                            .into(),
                        "Release, exposure, and breaking-change risks are within acceptable bounds"
                            .into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: true,
                }),
                sub_template_id: None,
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("shipped")],
                rework_to: Some(StageId::from("build")),
            },
            StageDef {
                id: StageId::from("shipped"),
                label: "Shipped".into(),
                runner: "Done".into(),
                runner_kind: RunnerKind::Deterministic,
                kind: StageKind::Terminal,
                procedure:
                    "Human-approved, cleanly committed with Why rationale, and shipped. Lessons remain available as Feedback for future versions."
                        .into(),
                gate_rubric: None,
                sub_template_id: None,
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![],
                rework_to: None,
            },
        ],
    };

    let discovery_philosophy = Philosophy {
        version: PhilosophyVersion::new(1),
        summary:
            "Discovery and shaping district that turns ambiguous problems into evidence-backed options, UX designs, and build-ready requirements through domain research, implementation audits, and isolated technical probes."
                .into(),
        principles: vec![
            Principle {
                id: "P-DSC-1".into(),
                title: "Primary Evidence, Domain & Prior Art First".into(),
                statement:
                    "Ground every analysis in primary code, official specs, domain-specific knowledge, and established industry best practices; flag any unverified gaps explicitly."
                        .into(),
                rationale:
                    "Framing solutions from hearsay or superficial assumptions causes costly misdirection and rework during implementation."
                        .into(),
            },
            Principle {
                id: "P-DSC-2".into(),
                title: "Empirical Isolation Before Opinion".into(),
                statement:
                    "Resolve uncertain technical behavior, compatibility, or operational constraints via minimal live probes in an isolated environment without dirtying production state."
                        .into(),
                rationale:
                    "Unwritten nuances and unguaranteed contracts cannot be settled on paper; minimal empirical execution yields decisive evidence."
                        .into(),
            },
            Principle {
                id: "P-DSC-3".into(),
                title: "Diverge Options, Converge with Explicit Tradeoffs".into(),
                statement:
                    "Avoid locking onto the first idea; frame 2-3 genuinely distinct approaches with clear evaluation criteria, a recommendation, rejection rationales, and exit conditions."
                        .into(),
                rationale:
                    "Exploring contrasting options prevents local optima and makes risk boundaries and accountability explicit."
                        .into(),
            },
            Principle {
                id: "P-DSC-4".into(),
                title: "Shape UX & Build-Ready Requirements".into(),
                statement:
                    "Never stop at a technology comparison; design the end-to-end user experience (flows, states, failure modes, and safety controls) and crystallize scope, non-goals, acceptance criteria, and invariants into a concrete Spec artifact ready for implementation handoff."
                        .into(),
                rationale:
                    "Unshaped decisions stall handoff and force downstream builders to guess UX and edge-case behavior."
                        .into(),
            },
            Principle {
                id: "P-DSC-5".into(),
                title: "Clean Teardown & Zero Residue".into(),
                statement:
                    "Remove all temporary directories, experimental configs, and probe scripts after verification, leaving zero investigative residue in the production setup."
                        .into(),
                rationale:
                    "Leftover experiment artifacts contaminate future development assumptions and break reproducibility and portability."
                        .into(),
            },
        ],
        anti_patterns: vec![
            "Jumping straight to one implementation idea without exploring domain context or alternative approaches"
                .into(),
            "Concluding 'it should work' without primary sources or isolated empirical verification"
                .into(),
            "Running experiments that dirty the production workspace or host environment".into(),
            "Delivering a fact dump or tech comparison with no UX design, non-goals, or build-ready requirements"
                .into(),
            "Hiding failed probes, operational caveats, or unresolved risks to make one option look cleaner"
                .into(),
        ],
        decision_heuristics: vec![
            "Investigate three lenses before proposing options: domain knowledge & best practices, existing system/codebase assets, and external technical specifications"
                .into(),
            "Whenever official documentation and runtime behavior might diverge, run an isolated canary probe to settle the question empirically"
                .into(),
            "Design failure handling, interruption paths, and human control boundaries before polishing the happy path"
                .into(),
            "Weight long-term simplicity, drift resistance, and daily ergonomics over quick prototype convenience"
                .into(),
        ],
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
    };

    let discovery_spec = ProcedureSpec {
        version: SpecVersion::new(1),
        changelog: "General-purpose discovery, technical validation, UX shaping, and requirements spec"
            .into(),
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
        stages: vec![
            StageDef {
                id: StageId::from("recon"),
                label: "Domain, Code & Tech Recon".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Investigate the problem across three primary lenses—domain knowledge & industry best practices, existing codebase/system realities, and external technical specifications—then empirically probe uncertain technical assumptions in isolation, tear down temporary environments cleanly, and lock evaluation criteria."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Domain practices, existing implementation realities, and external technical specs are grounded in primary sources"
                            .into(),
                        "Uncertain technical assumptions were validated in isolation and all temporary probe environments were cleanly torn down"
                            .into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-evidence")),
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("options")],
                rework_to: None,
            },
            StageDef {
                id: StageId::from("options"),
                label: "Options & Tradeoffs".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Synthesize recon and probe evidence into 2-3 genuinely distinct approaches with explicit tradeoffs, a clear recommendation, rejection rationales, and technical constraints or exit conditions."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "At least 2-3 distinct options are evaluated with primary evidence, clear recommendation, and rejection rationales"
                            .into(),
                        "Technical constraints, unresolved risks, and exit conditions are surfaced explicitly"
                            .into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: None,
                record_fields: vec![
                    StepFieldDef::required(
                        "options_compared",
                        "Options Compared & Primary Evidence",
                        "Record 2-3 genuinely distinct options along with comparison facts grounded in domain practices, primary code, and live probes.",
                    ),
                    StepFieldDef::required(
                        "recommendation",
                        "Recommended Direction & Rationale",
                        "Record the recommended direction, supporting rationale, rejection reasons for alternatives, and required preconditions.",
                    ),
                    StepFieldDef::required(
                        "constraints_and_risks",
                        "Constraints, Residual Risks & Exit Conditions",
                        "Record validated technical constraints, unresolved risks, and explicit exit or fallback conditions.",
                    ),
                ],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("shape")],
                rework_to: Some(StageId::from("recon")),
            },
            StageDef {
                id: StageId::from("shape"),
                label: "UX & Requirements Spec".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Translate the recommended direction into a cohesive UX design (user flows, states, failure/interruption handling, and human controls) and build-ready requirements (scope, non-goals, acceptance criteria, invariants, and downstream delivery plan), and attach the full Spec document as a verifiable Artifact."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "UX flows, state transitions, failure modes, and human control boundaries are concretely designed"
                            .into(),
                        "Scope, non-goals, acceptance criteria, invariants, and implementation handoff are captured in an attached Spec artifact"
                            .into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: None,
                record_fields: vec![
                    StepFieldDef::required(
                        "ux_design",
                        "UX Flows, States & Failure Handling",
                        "Record the end-to-end user experience, interaction model, state visibility, failure/interruption behavior, and human safety controls.",
                    ),
                    StepFieldDef::required(
                        "requirements",
                        "Scope, Non-Goals & Acceptance Criteria",
                        "Record concrete functional/non-functional requirements, explicit non-goals, and verifiable acceptance criteria.",
                    ),
                    StepFieldDef::required(
                        "handoff_plan",
                        "Invariants & Downstream Delivery Plan",
                        "Record non-negotiable invariants and how to split or queue concrete delivery jobs in downstream districts.",
                    ),
                ],
                required_artifact_kinds: vec![ArtifactKind::Spec],
                next: vec![StageId::from("approve")],
                rework_to: Some(StageId::from("options")),
            },
            StageDef {
                id: StageId::from("approve"),
                label: "Human Gate".into(),
                runner: "You".into(),
                runner_kind: RunnerKind::Human,
                kind: StageKind::ApprovalGate,
                procedure:
                    "Human owner reviews the recon evidence, option tradeoffs, UX design, and attached Spec artifact, then approves the shaped direction or sends it back for refinement."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Research evidence and option tradeoffs convincingly justify the recommended direction"
                            .into(),
                        "UX design and requirements Spec are clear enough to queue directly for downstream delivery"
                            .into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: true,
                }),
                sub_template_id: None,
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("shaped")],
                rework_to: Some(StageId::from("shape")),
            },
            StageDef {
                id: StageId::from("shaped"),
                label: "Shaped".into(),
                runner: "Done".into(),
                runner_kind: RunnerKind::Deterministic,
                kind: StageKind::Terminal,
                procedure:
                    "Direction, UX design, and requirements Spec locked. Ready to queue concrete delivery jobs in downstream districts."
                        .into(),
                gate_rubric: None,
                sub_template_id: None,
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![],
                rework_to: None,
            },
        ],
    };

    let root_cause_philosophy = Philosophy {
        version: PhilosophyVersion::new(1),
        summary:
            "Defect-resolution district that locks a failing reproduction test (Red) prior to fixing, eliminates symptom-masking workarounds, and proves root-cause resolution and recurrence prevention."
                .into(),
        principles: vec![
            Principle {
                id: "P-FIX-1".into(),
                title: "Failing Reproduction First (Red Before Green)".into(),
                statement:
                    "Before editing production code, lock a deterministic test that reproduces and fails on the defect along with its Before trigger conditions."
                        .into(),
                rationale:
                    "Fixing without a failing reproduction hides misdiagnosis and leaves no durable guarantee against recurrence."
                        .into(),
            },
            Principle {
                id: "P-FIX-2".into(),
                title: "Fix the Root Cause, Never Mask Symptoms".into(),
                statement:
                    "Do not paper over surface symptoms with local conditionals or ad-hoc guards; repair the broken invariant, state transition, or data boundary at its source."
                        .into(),
                rationale:
                    "Symptom-masking lets the same defect recur through alternate input paths while accumulating inexplicable special cases."
                        .into(),
            },
            Principle {
                id: "P-FIX-3".into(),
                title: "Zero Collateral Regression".into(),
                statement:
                    "Allow no unintended side effects. Turning the reproduction test Green must coincide with all adjacent behavior guards, full tests, and lints passing with zero warnings."
                        .into(),
                rationale:
                    "A fix that breaks an adjacent boundary or compatibility contract reduces overall system reliability."
                        .into(),
            },
            Principle {
                id: "P-FIX-4".into(),
                title: "Minimal Blast Radius & Durable Why".into(),
                statement:
                    "Keep edits scoped to the minimal root-cause repair and capture the failure mechanism, recurrence guard, and Why in evidence and commits."
                        .into(),
                rationale:
                    "Prevents unrelated churn and ensures future contributors do not reintroduce the same failure mode."
                        .into(),
            },
        ],
        anti_patterns: vec![
            "Guessing at fixes before locking a failing reproduction test (Red)".into(),
            "Swallowing errors or hiding symptoms behind localized if-branches".into(),
            "Weakening a reproduction test to fit post-fix behavior".into(),
            "Requesting approval without checking adjacent regressions, builds, and lint warnings"
                .into(),
        ],
        decision_heuristics: vec![
            "Do not enter implementation until the reproduction test fails on current code for the exact expected reason"
                .into(),
            "Audit sibling call sites and persistence layers to check whether the same root cause exists elsewhere"
                .into(),
            "Present a clear Before (Red) vs After (Green) contrast as verification evidence".into(),
        ],
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
    };

    let root_cause_spec = ProcedureSpec {
        version: SpecVersion::new(1),
        changelog: "Initial general-purpose root-cause bug fix procedure spec".into(),
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
        stages: vec![
            StageDef {
                id: StageId::from("reproduce"),
                label: "Reproduce (Red)".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Before editing production code, record minimal failure conditions and lock both a deterministic failing reproduction test (Red) and passing adjacent boundary guards."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Minimal reproduction conditions and pre-fix failure behavior (Before) are recorded"
                            .into(),
                        "Reproduction test fails (Red) for the expected reason on pre-fix code"
                            .into(),
                        "Adjacent existing behavior guards pass on the current state".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-red-repro")),
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("diagnose")],
                rework_to: None,
            },
            StageDef {
                id: StageId::from("diagnose"),
                label: "Root Cause".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Identify the broken invariant, state transition, or boundary condition rather than surface symptoms; audit sibling paths and lock a root-cause fix plan."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Root cause and failure mechanism are identified from primary code, not just symptoms"
                            .into(),
                        "Root-cause fix plan avoids symptom masking and accounts for sibling impact"
                            .into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: None,
                record_fields: vec![
                    StepFieldDef::required(
                        "root_cause",
                        "Identified Root Cause",
                        "Record the exact underlying defect location and failure mechanism rather than surface symptoms.",
                    ),
                    StepFieldDef::required(
                        "evidence",
                        "Diagnostic Evidence",
                        "Record the trace and reproduction facts that prove the root cause.",
                    ),
                    StepFieldDef::required(
                        "fix_plan",
                        "Root-Cause Fix Plan & Impact",
                        "Record the minimal, principled fix plan and any sibling paths affected.",
                    ),
                ],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("fix")],
                rework_to: Some(StageId::from("reproduce")),
            },
            StageDef {
                id: StageId::from("fix"),
                label: "Fix & Lock (Green)".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Fix the root cause with a minimal principled change so the pre-locked reproduction test turns Green unmodified; add boundary tests and pass all builds, tests, and lints with zero warnings."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Pre-locked reproduction test passes (Green) without modification".into(),
                        "All tests, builds, and lints pass with zero warnings".into(),
                        "No local-only paths, secrets, or unrelated edits are included".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-build-loop")),
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("verify")],
                rework_to: Some(StageId::from("diagnose")),
            },
            StageDef {
                id: StageId::from("verify"),
                label: "Verify & Diff".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Verify defect resolution on a live or production-equivalent build, compare Before (Red) vs After (Green) with zero adjacent regressions, and attach evidence artifacts and a Why summary."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Defect resolution is confirmed in a live or production-equivalent environment"
                            .into(),
                        "Before/After comparison demonstrates defect resolution and zero adjacent regressions"
                            .into(),
                        "Evidence artifacts and root-cause Why summary are complete".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-verify-ship")),
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("approve")],
                rework_to: Some(StageId::from("fix")),
            },
            StageDef {
                id: StageId::from("approve"),
                label: "Human Gate".into(),
                runner: "You".into(),
                runner_kind: RunnerKind::Human,
                kind: StageKind::ApprovalGate,
                procedure:
                    "Human owner reviews root-cause analysis, reproduction tests, and Before/After evidence, then Approves or triggers Rework with a concrete reason."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Root cause is resolved rather than masked".into(),
                        "Blast radius and release risk are within acceptable bounds".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: true,
                }),
                sub_template_id: None,
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("resolved")],
                rework_to: Some(StageId::from("fix")),
            },
            StageDef {
                id: StageId::from("resolved"),
                label: "Resolved".into(),
                runner: "Done".into(),
                runner_kind: RunnerKind::Deterministic,
                kind: StageKind::Terminal,
                procedure:
                    "Root cause fixed, recurrence test locked, and Why-documented commit completed. Prevention lessons remain ready as Feedback."
                        .into(),
                gate_rubric: None,
                sub_template_id: None,
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![],
                rework_to: None,
            },
        ],
    };

    let perf_philosophy = Philosophy {
        version: PhilosophyVersion::new(1),
        summary:
            "General-purpose performance and efficiency district built on instrumenting blind spots, locking Before baselines and zero-quality-loss guards first, and winning through principled structural optimization with same-condition proofs."
                .into(),
        principles: vec![
            Principle {
                id: "P-PRF-1".into(),
                title: "Make Unobservable Stages Observable First".into(),
                statement:
                    "Never tune unmeasured paths by intuition. Autonomously instrument unobservable bottlenecks and expose clear metrics before optimizing."
                        .into(),
                rationale:
                    "Optimizing blind adds complexity to non-dominant paths without improving real-world performance."
                        .into(),
            },
            Principle {
                id: "P-PRF-2".into(),
                title: "Baseline & Quality Guard Before Tuning".into(),
                statement:
                    "Before optimizing, lock reproducible same-condition Before metrics and deterministic guards that catch accuracy, visual, or UX degradation."
                        .into(),
                rationale:
                    "Optimizations that silently degrade quality or rely on incomparable benchmarks erode product value."
                        .into(),
            },
            Principle {
                id: "P-PRF-3".into(),
                title: "Zero Quality Degradation".into(),
                statement:
                    "Forbid shortcuts that trade accuracy, responsiveness, visual clarity, features, or safety for faster numbers; win through data structures, algorithmic complexity, I/O, caching, and idle elimination."
                        .into(),
                rationale:
                    "Speed bought at the expense of user experience or correctness is regression, not commercial quality."
                        .into(),
            },
            Principle {
                id: "P-PRF-4".into(),
                title: "Same-Condition Before/After Proof".into(),
                statement:
                    "Prove every gain through Before/After comparisons under identical inputs, environment, and settings, preserving reproducible steps and measurements as evidence."
                        .into(),
                rationale:
                    "Mismatched conditions or cherry-picked runs cannot separate real gains from measurement noise."
                        .into(),
            },
        ],
        anti_patterns: vec![
            "Adding optimizations by intuition without profiles or measurements".into(),
            "Starting tuning before locking Before baselines and quality guards".into(),
            "Sacrificing quality, accuracy, functionality, or idle power efficiency for headline numbers"
                .into(),
            "Comparing Before and After runs collected under different loads or environments"
                .into(),
            "Keeping complex code whose performance gain cannot be empirically proven".into(),
        ],
        decision_heuristics: vec![
            "Prioritize eliminating idle work, redundant recomputation, and polling ahead of micro-optimizations"
                .into(),
            "Discard changes whose gains remain within measurement noise and focus on dominant bottlenecks"
                .into(),
            "Keep measurement harnesses and quality guards reusable rather than one-off throwaway scripts"
                .into(),
        ],
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
    };

    let perf_spec = ProcedureSpec {
        version: SpecVersion::new(1),
        changelog: "Initial general-purpose performance and efficiency procedure spec".into(),
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
        stages: vec![
            StageDef {
                id: StageId::from("instrument"),
                label: "Observe & Scope".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Define target workloads and metrics (latency, throughput, memory, CPU/power) and add reproducible instrumentation for any unobservable segments."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Target workloads, evaluation metrics, and non-regression quality floors are documented"
                            .into(),
                        "Previously unobservable segments can be measured reproducibly".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-observe")),
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("baseline_guard")],
                rework_to: None,
            },
            StageDef {
                id: StageId::from("baseline_guard"),
                label: "Baseline & Quality Guard".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Before tuning, capture quantitative Before baselines and lock deterministic guards that automatically detect regressions in correctness, output quality, and UX."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Reproducible same-condition Before quantitative baselines are recorded"
                            .into(),
                        "Deterministic quality and correctness guards are locked and passing on current code"
                            .into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-baseline-guard")),
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("optimize")],
                rework_to: Some(StageId::from("instrument")),
            },
            StageDef {
                id: StageId::from("optimize"),
                label: "Optimize Loop".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Iterate on principled structural improvements targeting profile-proven bottlenecks with zero quality loss, remeasuring under identical conditions and validating quality guards."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Improvements are structural and preserve full quality and accuracy".into(),
                        "Pre-locked quality guards, full tests, builds, and lints pass with zero warnings"
                            .into(),
                        "No unproven complexity or machine-specific paths remain".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-perf-loop")),
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("verify")],
                rework_to: Some(StageId::from("baseline_guard")),
            },
            StageDef {
                id: StageId::from("verify"),
                label: "Before/After Proof".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Verify active and idle behavior on a live or production-equivalent build, and assemble a same-condition Before/After quantitative comparison table, evidence artifacts, and Why summary."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Live or production-equivalent verification (including idle behavior) is complete"
                            .into(),
                        "Same-condition Before/After quantitative comparison proves gains with zero quality regression"
                            .into(),
                        "Measurement evidence artifacts and Why summary are complete".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-verify-ship")),
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("approve")],
                rework_to: Some(StageId::from("optimize")),
            },
            StageDef {
                id: StageId::from("approve"),
                label: "Human Gate".into(),
                runner: "You".into(),
                runner_kind: RunnerKind::Human,
                kind: StageKind::ApprovalGate,
                procedure:
                    "Human owner reviews Before/After measurements, quality guard results, and complexity tradeoffs, then Approves or triggers Rework with a concrete reason."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Meaningful performance gains achieved without harming quality, readability, or maintainability"
                            .into(),
                        "Regression risk and complexity cost are within acceptable bounds".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: true,
                }),
                sub_template_id: None,
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("shipped")],
                rework_to: Some(StageId::from("optimize")),
            },
            StageDef {
                id: StageId::from("shipped"),
                label: "Shipped".into(),
                runner: "Done".into(),
                runner_kind: RunnerKind::Deterministic,
                kind: StageKind::Terminal,
                procedure:
                    "Optimization approved and committed with Before/After proof. Quality guards and measurement harnesses remain intact."
                        .into(),
                gate_rubric: None,
                sub_template_id: None,
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![],
                rework_to: None,
            },
        ],
    };

    let prune_philosophy = Philosophy {
        version: PhilosophyVersion::new(1),
        summary:
            "Simplification and hardening district that strips duplication, bloat, contract drift, and environment coupling under pre-locked behavior guards toward a single source of truth and clean portability."
                .into(),
        principles: vec![
            Principle {
                id: "P-PRN-1".into(),
                title: "Subtract Before Adding".into(),
                statement:
                    "Reduce complexity by deleting unnecessary branches, duplication, dead code, and redundant config rather than adding new wrapper layers or manuals."
                        .into(),
                rationale:
                    "Additive cleanup expands maintenance surface; genuine clarity and resilience come from subtraction."
                        .into(),
            },
            Principle {
                id: "P-PRN-2".into(),
                title: "Lock Behavior Guards Before Deletion".into(),
                statement:
                    "Before deleting, consolidating, or refactoring, lock baselines and automated guards covering preserved external behavior, persistence compatibility, and contracts."
                        .into(),
                rationale:
                    "Deleting without guards risks silently breaking hidden dependencies and backward compatibility."
                        .into(),
            },
            Principle {
                id: "P-PRN-3".into(),
                title: "Single Source of Truth, Zero Drift".into(),
                statement:
                    "Eliminate dual maintenance across specs, schemas, design tokens, rules, and code; prevent drift structurally with a single source of truth and deterministic sync tests."
                        .into(),
                rationale:
                    "Parallel configs and duplicated definitions inevitably diverge, breeding defects and operational friction."
                        .into(),
            },
            Principle {
                id: "P-PRN-4".into(),
                title: "Clean Portability & Strict Hygiene".into(),
                statement:
                    "Remove machine-specific paths, host assumptions, secrets, and environment-dependent tests so the project builds, tests, and runs identically from any machine or directory."
                        .into(),
                rationale:
                    "Environment coupling breaks team onboarding, CI reliability, and clean reproducible builds."
                        .into(),
            },
        ],
        anti_patterns: vec![
            "Running large deletions or consolidations without pre-locked behavior guards".into(),
            "Adding new wrappers, redundant prose manuals, or duplicate configs in the name of cleanup"
                .into(),
            "Silently breaking backward compatibility during refactoring".into(),
            "Mixing new feature additions with large-scale pruning in one ambiguous change".into(),
        ],
        decision_heuristics: vec![
            "Measure success by zero external regression (all guards green) combined with a net reduction in complexity, duplication, and coupling"
                .into(),
            "Replace conventions that humans or agents must manually remember with a single source of truth and automated sync checks"
                .into(),
            "Once primary references are traced, remove unused configs, dead branches, and obsolete compatibility code decisively"
                .into(),
        ],
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
    };

    let prune_spec = ProcedureSpec {
        version: SpecVersion::new(1),
        changelog: "Initial general-purpose prune and hardening procedure spec".into(),
        source_feedback_ids: vec![],
        updated_by: seed_actor,
        updated_at: SEED_TS,
        stages: vec![
            StageDef {
                id: StageId::from("audit"),
                label: "Audit Bloat & Drift".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Inventory duplicated definitions, contract drift, dead branches, environment coupling, and redundant config; lock pruning targets and external compatibility contracts to preserve."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "All pruning/consolidation targets and references are exhaustively traced"
                            .into(),
                        "Preserved external behavior, backward compatibility, and single-source strategy are clearly defined"
                            .into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-evidence")),
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("baseline_guard")],
                rework_to: None,
            },
            StageDef {
                id: StageId::from("baseline_guard"),
                label: "Lock Behavior Guards".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Before deleting or consolidating, record Before baselines and lock deterministic guards covering preserved external behavior, compatible formats, and contract synchronization."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Pre-change (Before) external behavior, contract, and complexity metrics are recorded"
                            .into(),
                        "Automated guards protecting external behavior and contract sync are locked and passing on current code"
                            .into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-baseline-guard")),
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("simplify")],
                rework_to: Some(StageId::from("audit")),
            },
            StageDef {
                id: StageId::from("simplify"),
                label: "Prune & Unify".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Remove duplication, dead branches, and environment coupling and unify onto a single source of truth without breaking pre-locked guards; pass all builds, tests, and lints with zero warnings."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Complexity and duplication show a net reduction with no new redundant wrappers"
                            .into(),
                        "All tests (including pre-locked guards), builds, and lints pass with zero warnings"
                            .into(),
                        "Zero local-only paths, environment assumptions, or secrets remain".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-build-loop")),
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("verify")],
                rework_to: Some(StageId::from("audit")),
            },
            StageDef {
                id: StageId::from("verify"),
                label: "Verify Zero Diff".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "Verify on a clean environment and live build, compare against Before baselines to confirm zero external behavior change, zero contract drift, and net reduction, and package evidence artifacts with a Why summary."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Live or clean-environment verification is complete".into(),
                        "Before/After comparison confirms zero external regression and net reduction in complexity or duplication"
                            .into(),
                        "Evidence artifacts and Why summary are complete".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-verify-ship")),
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("approve")],
                rework_to: Some(StageId::from("simplify")),
            },
            StageDef {
                id: StageId::from("approve"),
                label: "Human Gate".into(),
                runner: "You".into(),
                runner_kind: RunnerKind::Human,
                kind: StageKind::ApprovalGate,
                procedure:
                    "Human owner reviews pruning scope, preserved compatibility, and single-source consolidation, then Approves or triggers Rework with a concrete reason."
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "Simplified cleanly without compromising required behavior or backward compatibility"
                            .into(),
                        "Aligns with single-source-of-truth and portability principles".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: true,
                }),
                sub_template_id: None,
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![StageId::from("hardened")],
                rework_to: Some(StageId::from("simplify")),
            },
            StageDef {
                id: StageId::from("hardened"),
                label: "Hardened".into(),
                runner: "Done".into(),
                runner_kind: RunnerKind::Deterministic,
                kind: StageKind::Terminal,
                procedure:
                    "Pruned, consolidated to a single source of truth, guarded, and committed with Why rationale. Clean and drift-free."
                        .into(),
                gate_rubric: None,
                sub_template_id: None,
                record_fields: vec![],
                required_artifact_kinds: vec![],
                next: vec![],
                rework_to: None,
            },
        ],
    };

    let districts = vec![
        District {
            id: DistrictId::from("DIST-001"),
            owner_id: owner_id.clone(),
            name: "DISTRICT 01 - Product Delivery".into(),
            setup_by: "General Blueprint".into(),
            active_philosophy_version: PhilosophyVersion::new(1),
            philosophy_history: vec![delivery_philosophy],
            active_spec_version: SpecVersion::new(1),
            spec_history: vec![delivery_spec],
            archived: false,
            archived_at: None,
        },
        District {
            id: DistrictId::from("DIST-002"),
            owner_id: owner_id.clone(),
            name: "DISTRICT 02 - Discovery & Shaping".into(),
            setup_by: "General Blueprint".into(),
            active_philosophy_version: PhilosophyVersion::new(1),
            philosophy_history: vec![discovery_philosophy],
            active_spec_version: SpecVersion::new(1),
            spec_history: vec![discovery_spec],
            archived: false,
            archived_at: None,
        },
        District {
            id: DistrictId::from("DIST-003"),
            owner_id: owner_id.clone(),
            name: "DISTRICT 03 - Root-Cause Fix".into(),
            setup_by: "General Blueprint".into(),
            active_philosophy_version: PhilosophyVersion::new(1),
            philosophy_history: vec![root_cause_philosophy],
            active_spec_version: SpecVersion::new(1),
            spec_history: vec![root_cause_spec],
            archived: false,
            archived_at: None,
        },
        District {
            id: DistrictId::from("DIST-004"),
            owner_id: owner_id.clone(),
            name: "DISTRICT 04 - Perf & Efficiency".into(),
            setup_by: "General Blueprint".into(),
            active_philosophy_version: PhilosophyVersion::new(1),
            philosophy_history: vec![perf_philosophy],
            active_spec_version: SpecVersion::new(1),
            spec_history: vec![perf_spec],
            archived: false,
            archived_at: None,
        },
        District {
            id: DistrictId::from("DIST-005"),
            owner_id: owner_id.clone(),
            name: "DISTRICT 05 - Prune & Hardening".into(),
            setup_by: "General Blueprint".into(),
            active_philosophy_version: PhilosophyVersion::new(1),
            philosophy_history: vec![prune_philosophy],
            active_spec_version: SpecVersion::new(1),
            spec_history: vec![prune_spec],
            archived: false,
            archived_at: None,
        },
    ];

    WorkspaceSeed {
        templates,
        template_order,
        districts,
        jobs: Vec::new(),
        next_job_seq: 1,
        next_fencing_token: 1,
    }
}
