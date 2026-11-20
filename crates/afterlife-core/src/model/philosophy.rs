use serde::{Deserialize, Serialize};

use crate::{
    ids::{DistrictId, FeedbackId, OwnerId, PhilosophyVersion, ProposalId, TimestampMs},
    model::actor::Actor,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principle {
    pub id: String,
    pub title: String,
    pub statement: String,
    pub rationale: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Philosophy {
    pub version: PhilosophyVersion,
    pub summary: String,
    pub principles: Vec<Principle>,
    pub anti_patterns: Vec<String>,
    pub decision_heuristics: Vec<String>,
    pub source_feedback_ids: Vec<FeedbackId>,
    pub updated_by: Actor,
    pub updated_at: TimestampMs,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhilosophyProposal {
    pub id: ProposalId,
    pub owner_id: OwnerId,
    pub district_id: DistrictId,
    pub base_version: PhilosophyVersion,
    pub proposed: Philosophy,
    pub rationale: String,
    pub source_feedback_ids: Vec<FeedbackId>,
    pub proposed_by: Actor,
    pub status: ProposalStatus,
    pub created_at: TimestampMs,
    pub resolved_at: Option<TimestampMs>,
}
