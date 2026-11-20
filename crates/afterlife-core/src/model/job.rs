use serde::{Deserialize, Serialize};

use crate::{
    ids::{
        ArtifactId, DistrictId, FeedbackId, FencingToken, JobId, LeaseId, OwnerId,
        PhilosophyVersion, RunnerId, SpecVersion, StageId, SubStageId, TimestampMs,
    },
    model::{
        district::{GateRubric, StageDef},
        feedback::{Feedback, GateVerification},
        philosophy::Philosophy,
        sub_machine::SubStageDef,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    RawNote,
    Issue,
    Spec,
    SourceCode,
    PullRequest,
    CiReport,
    Snapshot,
    VerificationReport,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub id: ArtifactId,
    pub kind: ArtifactKind,
    pub label: String,
    pub uri: String,
    pub sha256: Option<String>,
    pub produced_at_stage: Option<StageId>,
    pub revision_at_capture: u64,
}

impl Artifact {
    pub fn simple(id: impl Into<ArtifactId>, label: impl Into<String>, kind: ArtifactKind) -> Self {
        let label = label.into();
        Self {
            id: id.into(),
            kind,
            uri: label.clone(),
            label,
            sha256: None,
            produced_at_stage: None,
            revision_at_capture: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobLease {
    pub lease_id: LeaseId,
    pub fencing_token: FencingToken,
    pub runner_id: RunnerId,
    pub runner_label: String,
    pub holder_client_id: Option<String>,
    pub acquired_at: TimestampMs,
    pub expires_at: TimestampMs,
}

impl JobLease {
    pub fn is_active_at(&self, now: TimestampMs) -> bool {
        self.expires_at > now
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum JobLocation {
    Queue,
    InDistrict {
        district_id: DistrictId,
        stage_id: StageId,
        sub_stage_id: Option<SubStageId>,
    },
    Completed {
        district_id: DistrictId,
        terminal_stage_id: StageId,
        completed_at: TimestampMs,
    },
    Archived,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub owner_id: OwnerId,
    pub revision: u64,
    pub title: String,
    pub raw_input: String,
    pub spec_summary: String,
    pub location: JobLocation,
    pub target_district_hint: Option<DistrictId>,
    pub pinned_philosophy_version: Option<PhilosophyVersion>,
    pub pinned_spec_version: Option<SpecVersion>,
    pub lease: Option<JobLease>,
    pub artifacts: Vec<Artifact>,
    pub gate_verifications: Vec<GateVerification>,
    pub feedback_ids: Vec<FeedbackId>,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
}

impl Job {
    pub fn claimed_by(&self) -> Option<&str> {
        self.lease.as_ref().map(|l| l.runner_label.as_str())
    }

    pub fn is_in_stage(&self, target_district: &DistrictId, target_stage: &StageId) -> bool {
        match &self.location {
            JobLocation::InDistrict {
                district_id,
                stage_id,
                ..
            } => district_id == target_district && stage_id == target_stage,
            JobLocation::Completed {
                district_id,
                terminal_stage_id,
                ..
            } => district_id == target_district && terminal_stage_id == target_stage,
            _ => false,
        }
    }

    pub fn is_in_district(&self, target_district: &DistrictId) -> bool {
        match &self.location {
            JobLocation::InDistrict { district_id, .. } => district_id == target_district,
            JobLocation::Completed { district_id, .. } => district_id == target_district,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub job_id: JobId,
    pub owner_id: OwnerId,
    pub district_id: DistrictId,
    pub district_name: String,
    pub philosophy: Philosophy,
    pub stage: StageDef,
    pub sub_stage: Option<SubStageDef>,
    pub gate_rubric: Option<GateRubric>,
    pub recent_feedbacks: Vec<Feedback>,
}
