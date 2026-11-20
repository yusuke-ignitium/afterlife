use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::{
    DistrictId, FencingToken, JobId, OwnerId, PhilosophyVersion, ProposalId, SpecVersion, StageId,
    SubMachineTemplateId,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", content = "data", rename_all = "snake_case")]
pub enum TransitionError {
    OwnerMismatch {
        expected: OwnerId,
        actual: OwnerId,
    },
    HumanApprovalRequired {
        action: String,
    },
    JobNotFound(JobId),
    DistrictNotFound(DistrictId),
    StageNotFound {
        district_id: DistrictId,
        stage_id: StageId,
    },
    TemplateNotFound(SubMachineTemplateId),
    ProposalNotFound(ProposalId),
    ProposalAlreadyResolved(ProposalId),
    QueueEmpty,
    DistrictHasNoStages(DistrictId),
    RevisionConflict {
        job_id: JobId,
        expected: u64,
        actual: u64,
    },
    AlreadyLeased {
        job_id: JobId,
        holder: String,
        expires_at: u64,
    },
    LeaseMissing {
        job_id: JobId,
    },
    LeaseExpired {
        job_id: JobId,
        expired_at: u64,
    },
    StaleFencingToken {
        job_id: JobId,
        provided: FencingToken,
        current: FencingToken,
    },
    InvalidLocation {
        job_id: JobId,
        reason: String,
    },
    NoNextStage {
        district_id: DistrictId,
        stage_id: StageId,
    },
    NoReworkTarget {
        district_id: DistrictId,
        stage_id: StageId,
    },
    SubTemplateNotAllowed {
        district_id: DistrictId,
        stage_id: StageId,
    },
    PhilosophyVersionConflict {
        district_id: DistrictId,
        expected: PhilosophyVersion,
        actual: PhilosophyVersion,
    },
    SpecVersionConflict {
        district_id: DistrictId,
        expected: SpecVersion,
        actual: SpecVersion,
    },
    InvalidProposedSpec {
        district_id: DistrictId,
        reasons: Vec<String>,
    },
    InvalidInput {
        field: String,
        reason: String,
    },
    JobAlreadyInDistrict {
        job_id: JobId,
        district_id: DistrictId,
        stage_id: StageId,
    },
    GateVerificationRequired {
        job_id: JobId,
        district_id: DistrictId,
        stage_id: StageId,
        missing_checks: Vec<String>,
    },
}

impl TransitionError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::OwnerMismatch { .. } => "owner_mismatch",
            Self::HumanApprovalRequired { .. } => "human_approval_required",
            Self::JobNotFound(_) => "job_not_found",
            Self::DistrictNotFound(_) => "district_not_found",
            Self::StageNotFound { .. } => "stage_not_found",
            Self::TemplateNotFound(_) => "template_not_found",
            Self::ProposalNotFound(_) => "proposal_not_found",
            Self::ProposalAlreadyResolved(_) => "proposal_already_resolved",
            Self::QueueEmpty => "queue_empty",
            Self::DistrictHasNoStages(_) => "district_has_no_stages",
            Self::RevisionConflict { .. } => "revision_conflict",
            Self::AlreadyLeased { .. } => "already_leased",
            Self::LeaseMissing { .. } => "lease_missing",
            Self::LeaseExpired { .. } => "lease_expired",
            Self::StaleFencingToken { .. } => "stale_fencing_token",
            Self::InvalidLocation { .. } => "invalid_location",
            Self::NoNextStage { .. } => "no_next_stage",
            Self::NoReworkTarget { .. } => "no_rework_target",
            Self::SubTemplateNotAllowed { .. } => "sub_template_not_allowed",
            Self::PhilosophyVersionConflict { .. } => "philosophy_version_conflict",
            Self::SpecVersionConflict { .. } => "spec_version_conflict",
            Self::InvalidProposedSpec { .. } => "invalid_proposed_spec",
            Self::InvalidInput { .. } => "invalid_input",
            Self::JobAlreadyInDistrict { .. } => "job_already_in_district",
            Self::GateVerificationRequired { .. } => "gate_verification_required",
        }
    }
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OwnerMismatch { expected, actual } => {
                write!(
                    f,
                    "owner isolation violation: expected {expected}, got {actual}"
                )
            }
            Self::HumanApprovalRequired { action } => {
                write!(f, "human approval required for action: {action}")
            }
            Self::JobNotFound(id) => write!(f, "job not found: {id}"),
            Self::DistrictNotFound(id) => write!(f, "district not found: {id}"),
            Self::StageNotFound {
                district_id,
                stage_id,
            } => write!(f, "stage not found: {district_id}/{stage_id}"),
            Self::TemplateNotFound(id) => write!(f, "sub-machine template not found: {id}"),
            Self::ProposalNotFound(id) => write!(f, "proposal not found: {id}"),
            Self::ProposalAlreadyResolved(id) => write!(f, "proposal already resolved: {id}"),
            Self::QueueEmpty => write!(f, "queue has no pending jobs"),
            Self::DistrictHasNoStages(id) => write!(f, "district has no stages: {id}"),
            Self::RevisionConflict {
                job_id,
                expected,
                actual,
            } => write!(
                f,
                "revision conflict on {job_id}: expected {expected}, actual {actual}"
            ),
            Self::AlreadyLeased {
                job_id,
                holder,
                expires_at,
            } => write!(
                f,
                "job {job_id} is currently leased by {holder} until {expires_at}"
            ),
            Self::LeaseMissing { job_id } => write!(f, "job {job_id} has no active lease"),
            Self::LeaseExpired { job_id, expired_at } => {
                write!(f, "lease on {job_id} expired at {expired_at}")
            }
            Self::StaleFencingToken {
                job_id,
                provided,
                current,
            } => write!(
                f,
                "stale fencing token on {job_id}: provided {provided}, current {current}"
            ),
            Self::InvalidLocation { job_id, reason } => {
                write!(f, "invalid location for {job_id}: {reason}")
            }
            Self::NoNextStage {
                district_id,
                stage_id,
            } => write!(
                f,
                "stage has no outgoing transition: {district_id}/{stage_id}"
            ),
            Self::NoReworkTarget {
                district_id,
                stage_id,
            } => write!(f, "stage has no rework target: {district_id}/{stage_id}"),
            Self::SubTemplateNotAllowed {
                district_id,
                stage_id,
            } => write!(
                f,
                "sub-machine template cannot be attached to gate/terminal stage: {district_id}/{stage_id}"
            ),
            Self::PhilosophyVersionConflict {
                district_id,
                expected,
                actual,
            } => write!(
                f,
                "philosophy version conflict on {district_id}: expected v{expected}, actual v{actual}"
            ),
            Self::SpecVersionConflict {
                district_id,
                expected,
                actual,
            } => write!(
                f,
                "procedure spec version conflict on {district_id}: expected v{expected}, actual v{actual}"
            ),
            Self::InvalidProposedSpec {
                district_id,
                reasons,
            } => write!(
                f,
                "invalid proposed spec for {district_id}: {}",
                reasons.join("; ")
            ),
            Self::InvalidInput { field, reason } => {
                write!(f, "invalid input for '{field}': {reason}")
            }
            Self::JobAlreadyInDistrict {
                job_id,
                district_id,
                stage_id,
            } => write!(
                f,
                "job {job_id} is already in {district_id}/{stage_id}; agents cannot reassign in-district jobs"
            ),
            Self::GateVerificationRequired {
                job_id,
                district_id,
                stage_id,
                missing_checks,
            } => {
                if missing_checks.is_empty() {
                    write!(
                        f,
                        "gate verification with verdict=pass required before advancing {job_id} out of {district_id}/{stage_id}"
                    )
                } else {
                    write!(
                        f,
                        "gate verification incomplete for {job_id} at {district_id}/{stage_id}; missing checks: {}",
                        missing_checks.join(", ")
                    )
                }
            }
        }
    }
}

impl std::error::Error for TransitionError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationError {
    EmptyDistrict(DistrictId),
    MissingPhilosophy(DistrictId),
    MissingProcedureSpec(DistrictId),
    DuplicateStageId {
        district_id: DistrictId,
        stage_id: StageId,
    },
    DanglingNextStage {
        district_id: DistrictId,
        from: StageId,
        to: StageId,
    },
    DanglingReworkStage {
        district_id: DistrictId,
        from: StageId,
        to: StageId,
    },
    DanglingSubTemplate {
        district_id: DistrictId,
        stage_id: StageId,
        template_id: SubMachineTemplateId,
    },
    EmptySubTemplate(SubMachineTemplateId),
    SubTemplateOnNonWorkStage {
        district_id: DistrictId,
        stage_id: StageId,
    },
    TerminalStageHasOutgoing {
        district_id: DistrictId,
        stage_id: StageId,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ValidationError {}
