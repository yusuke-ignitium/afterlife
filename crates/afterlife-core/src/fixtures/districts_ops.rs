use crate::{
    ids::{DistrictId, OwnerId, PhilosophyVersion, SpecVersion, StageId, SubMachineTemplateId},
    model::{
        Actor, District, GateRubric, Philosophy, Principle, ProcedureSpec, RunnerKind, StageDef,
        StageKind, StepFieldDef,
    },
};

use super::SEED_TS;

pub(super) fn districts(owner_id: &OwnerId, seed_actor: &Actor) -> Vec<District> {
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

    vec![
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
    ]
}
