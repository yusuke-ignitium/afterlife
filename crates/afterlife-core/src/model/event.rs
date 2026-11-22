use serde::{Deserialize, Serialize};

use crate::{
    ids::{
        ArtifactId, DistrictId, FeedbackId, FencingToken, JobId, LeaseId, OwnerId,
        PhilosophyVersion, ProposalId, SpecVersion, StageId, SubMachineTemplateId, SubStageId,
        TimestampMs,
    },
    model::{
        actor::Actor,
        feedback::{FeedbackKind, GateVerdict},
        job::{ArtifactKind, JobLocation},
        step::StepRecordOutcome,
    },
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobEvent {
    JobQueued {
        owner_id: OwnerId,
        actor: Actor,
        job_id: JobId,
        revision: u64,
        title: String,
        at: TimestampMs,
    },
    JobUpdated {
        owner_id: OwnerId,
        actor: Actor,
        job_id: JobId,
        revision: u64,
        at: TimestampMs,
    },
    JobArchived {
        owner_id: OwnerId,
        actor: Actor,
        job_id: JobId,
        revision: u64,
        from: JobLocation,
        reason: Option<String>,
        at: TimestampMs,
    },
    JobTransitioned {
        owner_id: OwnerId,
        actor: Actor,
        job_id: JobId,
        revision: u64,
        from: JobLocation,
        to: JobLocation,
        claimed_by: Option<String>,
        fencing_token: Option<FencingToken>,
        at: TimestampMs,
    },
    JobReworked {
        owner_id: OwnerId,
        actor: Actor,
        job_id: JobId,
        revision: u64,
        from: JobLocation,
        to: JobLocation,
        reason: Option<String>,
        at: TimestampMs,
    },
    JobClaimed {
        owner_id: OwnerId,
        actor: Actor,
        job_id: JobId,
        revision: u64,
        lease_id: LeaseId,
        fencing_token: FencingToken,
        runner_label: String,
        expires_at: TimestampMs,
        at: TimestampMs,
    },
    JobLeaseRenewed {
        owner_id: OwnerId,
        actor: Actor,
        job_id: JobId,
        lease_id: LeaseId,
        fencing_token: FencingToken,
        expires_at: TimestampMs,
        at: TimestampMs,
    },
    JobLeaseReleased {
        owner_id: OwnerId,
        actor: Actor,
        job_id: JobId,
        lease_id: LeaseId,
        fencing_token: FencingToken,
        at: TimestampMs,
    },
    ArtifactAttached {
        owner_id: OwnerId,
        actor: Actor,
        job_id: JobId,
        revision: u64,
        artifact_id: ArtifactId,
        kind: ArtifactKind,
        label: String,
        uri: String,
        at: TimestampMs,
    },
    GateVerificationRecorded {
        owner_id: OwnerId,
        actor: Actor,
        job_id: JobId,
        stage_id: StageId,
        verdict: GateVerdict,
        summary: String,
        at: TimestampMs,
    },
    StepRecorded {
        owner_id: OwnerId,
        actor: Actor,
        job_id: JobId,
        district_id: DistrictId,
        stage_id: StageId,
        sub_stage_id: Option<SubStageId>,
        revision: u64,
        outcome: StepRecordOutcome,
        at: TimestampMs,
    },
    FeedbackSubmitted {
        owner_id: OwnerId,
        actor: Actor,
        feedback_id: FeedbackId,
        district_id: DistrictId,
        job_id: Option<JobId>,
        stage_id: Option<StageId>,
        kind: FeedbackKind,
        at: TimestampMs,
    },
    PhilosophyProposed {
        owner_id: OwnerId,
        actor: Actor,
        proposal_id: ProposalId,
        district_id: DistrictId,
        base_version: PhilosophyVersion,
        proposed_version: PhilosophyVersion,
        at: TimestampMs,
    },
    PhilosophyUpdated {
        owner_id: OwnerId,
        actor: Actor,
        proposal_id: ProposalId,
        district_id: DistrictId,
        new_version: PhilosophyVersion,
        at: TimestampMs,
    },
    SpecProposed {
        owner_id: OwnerId,
        actor: Actor,
        proposal_id: ProposalId,
        district_id: DistrictId,
        base_version: SpecVersion,
        proposed_version: SpecVersion,
        at: TimestampMs,
    },
    SpecUpdated {
        owner_id: OwnerId,
        actor: Actor,
        proposal_id: ProposalId,
        district_id: DistrictId,
        new_version: SpecVersion,
        at: TimestampMs,
    },
    StageSubTemplateChanged {
        owner_id: OwnerId,
        actor: Actor,
        district_id: DistrictId,
        stage_id: StageId,
        template_id: Option<SubMachineTemplateId>,
        at: TimestampMs,
    },
}

impl JobEvent {
    pub fn owner_id(&self) -> &OwnerId {
        match self {
            Self::JobQueued { owner_id, .. }
            | Self::JobUpdated { owner_id, .. }
            | Self::JobArchived { owner_id, .. }
            | Self::JobTransitioned { owner_id, .. }
            | Self::JobReworked { owner_id, .. }
            | Self::JobClaimed { owner_id, .. }
            | Self::JobLeaseRenewed { owner_id, .. }
            | Self::JobLeaseReleased { owner_id, .. }
            | Self::ArtifactAttached { owner_id, .. }
            | Self::GateVerificationRecorded { owner_id, .. }
            | Self::StepRecorded { owner_id, .. }
            | Self::FeedbackSubmitted { owner_id, .. }
            | Self::PhilosophyProposed { owner_id, .. }
            | Self::PhilosophyUpdated { owner_id, .. }
            | Self::SpecProposed { owner_id, .. }
            | Self::SpecUpdated { owner_id, .. }
            | Self::StageSubTemplateChanged { owner_id, .. } => owner_id,
        }
    }

    pub fn timestamp_ms(&self) -> TimestampMs {
        match self {
            Self::JobQueued { at, .. }
            | Self::JobUpdated { at, .. }
            | Self::JobArchived { at, .. }
            | Self::JobTransitioned { at, .. }
            | Self::JobReworked { at, .. }
            | Self::JobClaimed { at, .. }
            | Self::JobLeaseRenewed { at, .. }
            | Self::JobLeaseReleased { at, .. }
            | Self::ArtifactAttached { at, .. }
            | Self::GateVerificationRecorded { at, .. }
            | Self::StepRecorded { at, .. }
            | Self::FeedbackSubmitted { at, .. }
            | Self::PhilosophyProposed { at, .. }
            | Self::PhilosophyUpdated { at, .. }
            | Self::SpecProposed { at, .. }
            | Self::SpecUpdated { at, .. }
            | Self::StageSubTemplateChanged { at, .. } => *at,
        }
    }
}
