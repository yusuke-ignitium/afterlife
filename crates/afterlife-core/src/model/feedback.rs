use serde::{Deserialize, Serialize};

use crate::{
    ids::{DistrictId, FeedbackId, JobId, OwnerId, StageId, TimestampMs},
    model::actor::Actor,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackKind {
    ApprovalNote,
    ReworkReason,
    PhilosophyInsight,
    ProcedureFix,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feedback {
    pub id: FeedbackId,
    pub owner_id: OwnerId,
    pub district_id: DistrictId,
    pub job_id: Option<JobId>,
    pub stage_id: Option<StageId>,
    pub kind: FeedbackKind,
    pub body: String,
    pub author: Actor,
    pub created_at: TimestampMs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateVerdict {
    Pass,
    NeedsWork,
    Fail,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateVerification {
    pub stage_id: StageId,
    pub verdict: GateVerdict,
    pub summary: String,
    pub checklist_results: Vec<(String, bool)>,
    pub verifier: Actor,
    pub verified_at: TimestampMs,
}
