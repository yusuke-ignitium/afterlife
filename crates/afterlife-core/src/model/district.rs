use serde::{Deserialize, Serialize};

use crate::{
    ids::{
        DistrictId, FeedbackId, OwnerId, PhilosophyVersion, ProposalId, SpecVersion, StageId,
        SubMachineTemplateId, TimestampMs,
    },
    model::{
        actor::Actor,
        job::ArtifactKind,
        philosophy::{Philosophy, ProposalStatus},
        runner::RunnerKind,
        step::StepFieldDef,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageKind {
    Work,
    ApprovalGate,
    Terminal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateRubric {
    pub checks: Vec<String>,
    pub auto_verify_by_llm: bool,
    pub requires_human_approval: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageDef {
    pub id: StageId,
    pub label: String,
    pub runner: String,
    pub runner_kind: RunnerKind,
    pub kind: StageKind,
    pub procedure: String,
    pub gate_rubric: Option<GateRubric>,
    pub sub_template_id: Option<SubMachineTemplateId>,
    #[serde(default)]
    pub record_fields: Vec<StepFieldDef>,
    #[serde(default)]
    pub required_artifact_kinds: Vec<ArtifactKind>,
    pub next: Vec<StageId>,
    pub rework_to: Option<StageId>,
}

impl StageDef {
    pub fn is_gate(&self) -> bool {
        self.kind == StageKind::ApprovalGate
    }

    pub fn is_terminal(&self) -> bool {
        self.kind == StageKind::Terminal
    }

    pub fn supports_sub_machine(&self) -> bool {
        self.kind == StageKind::Work
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureSpec {
    pub version: SpecVersion,
    pub stages: Vec<StageDef>,
    pub changelog: String,
    pub source_feedback_ids: Vec<FeedbackId>,
    pub updated_by: Actor,
    pub updated_at: TimestampMs,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecProposal {
    pub id: ProposalId,
    pub owner_id: OwnerId,
    pub district_id: DistrictId,
    pub base_version: SpecVersion,
    pub proposed: ProcedureSpec,
    pub rationale: String,
    pub source_feedback_ids: Vec<FeedbackId>,
    pub proposed_by: Actor,
    pub status: ProposalStatus,
    pub created_at: TimestampMs,
    pub resolved_at: Option<TimestampMs>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct District {
    pub id: DistrictId,
    pub owner_id: OwnerId,
    pub name: String,
    pub setup_by: String,
    pub active_philosophy_version: PhilosophyVersion,
    pub philosophy_history: Vec<Philosophy>,
    pub active_spec_version: SpecVersion,
    pub spec_history: Vec<ProcedureSpec>,
}

impl District {
    pub fn active_philosophy(&self) -> &Philosophy {
        self.philosophy_history
            .iter()
            .find(|p| p.version == self.active_philosophy_version)
            .or_else(|| self.philosophy_history.last())
            .expect("district must have at least one philosophy version")
    }

    pub fn active_spec(&self) -> &ProcedureSpec {
        self.spec_history
            .iter()
            .find(|s| s.version == self.active_spec_version)
            .or_else(|| self.spec_history.last())
            .expect("district must have at least one procedure spec version")
    }

    pub fn active_spec_mut(&mut self) -> &mut ProcedureSpec {
        let active = self.active_spec_version;
        if let Some(idx) = self.spec_history.iter().position(|s| s.version == active) {
            return &mut self.spec_history[idx];
        }
        self.spec_history
            .last_mut()
            .expect("district must have at least one procedure spec version")
    }

    pub fn philosophy_at(&self, version: PhilosophyVersion) -> Option<&Philosophy> {
        self.philosophy_history
            .iter()
            .find(|p| p.version == version)
    }

    pub fn spec_at(&self, version: SpecVersion) -> Option<&ProcedureSpec> {
        self.spec_history.iter().find(|s| s.version == version)
    }

    pub fn stages(&self) -> &[StageDef] {
        &self.active_spec().stages
    }
}
