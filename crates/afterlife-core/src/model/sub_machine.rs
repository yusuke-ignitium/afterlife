use serde::{Deserialize, Serialize};

use crate::{
    ids::{FeedbackId, OwnerId, ProposalId, SubMachineTemplateId, SubStageId, TimestampMs},
    model::{
        actor::Actor,
        job::ArtifactKind,
        philosophy::ProposalStatus,
        runner::RunnerKind,
        step::StepFieldDef,
    },
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubStageDef {
    pub id: SubStageId,
    pub label: String,
    pub runner: String,
    pub runner_kind: RunnerKind,
    pub procedure: String,
    pub gate_checks: Vec<String>,
    #[serde(default)]
    pub record_fields: Vec<StepFieldDef>,
    #[serde(default)]
    pub required_artifact_kinds: Vec<ArtifactKind>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubMachineTemplate {
    pub id: SubMachineTemplateId,
    pub name: String,
    pub description: String,
    pub philosophy_notes: String,
    pub stages: Vec<SubStageDef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateProposal {
    pub id: ProposalId,
    pub owner_id: OwnerId,
    pub template_id: SubMachineTemplateId,
    pub proposed: SubMachineTemplate,
    pub rationale: String,
    #[serde(default)]
    pub source_feedback_ids: Vec<FeedbackId>,
    pub proposed_by: Actor,
    pub status: ProposalStatus,
    pub created_at: TimestampMs,
    pub resolved_at: Option<TimestampMs>,
}
