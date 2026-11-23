use crate::{
    ids::{DistrictId, OwnerId, PhilosophyVersion, SpecVersion, StageId, SubMachineTemplateId},
    model::{
        Actor, District, GateRubric, Philosophy, Principle, ProcedureSpec, RunnerKind, StageDef,
        StageKind,
    },
};

use super::SEED_TS;

pub(super) fn districts(owner_id: &OwnerId, seed_actor: &Actor) -> Vec<District> {
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
        updated_by: seed_actor.clone(),
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

    vec![District {
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
    }]
}
