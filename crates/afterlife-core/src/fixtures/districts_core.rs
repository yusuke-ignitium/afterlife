use crate::{
    ids::{DistrictId, OwnerId, PhilosophyVersion, SpecVersion, StageId, SubMachineTemplateId},
    model::{
        Actor, ArtifactKind, District, GateRubric, Philosophy, Principle, ProcedureSpec,
        RunnerKind, StageDef, StageKind, StepFieldDef,
    },
};

use super::SEED_TS;

pub(super) fn districts(owner_id: &OwnerId, seed_actor: &Actor) -> Vec<District> {
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

    vec![
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
    ]
}
