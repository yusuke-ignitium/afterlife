use crate::{
    ids::{SubMachineTemplateId, SubStageId},
    model::{ArtifactKind, RunnerKind, StepFieldDef, SubMachineTemplate, SubStageDef},
};

pub(super) fn templates() -> Vec<SubMachineTemplate> {
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

    vec![tmpl_build_loop, tmpl_perf_loop, tmpl_verify_ship]
}
