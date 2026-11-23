use crate::{
    ids::{SubMachineTemplateId, SubStageId},
    model::{ArtifactKind, RunnerKind, StepFieldDef, SubMachineTemplate, SubStageDef},
};

pub(super) fn templates() -> Vec<SubMachineTemplate> {
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

    vec![tmpl_evidence, tmpl_observe, tmpl_baseline_guard, tmpl_red_repro]
}
