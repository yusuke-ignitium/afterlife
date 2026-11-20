use crate::{
    error::TransitionError,
    ids::{DistrictId, FencingToken, TimestampMs},
    model::{
        actor::Actor,
        district::StageDef,
        feedback::{FeedbackKind, GateVerdict},
        job::{Job, JobLocation},
    },
};

pub const MIN_LEASE_TTL_MS: TimestampMs = 1_000;
pub const MAX_LEASE_TTL_MS: TimestampMs = 3_600_000;

pub fn validate_lease_ttl(ttl_ms: TimestampMs) -> Result<(), TransitionError> {
    if !(MIN_LEASE_TTL_MS..=MAX_LEASE_TTL_MS).contains(&ttl_ms) {
        return Err(TransitionError::InvalidInput {
            field: "ttl_ms".into(),
            reason: format!(
                "must be between {MIN_LEASE_TTL_MS}ms and {MAX_LEASE_TTL_MS}ms (got {ttl_ms}ms)"
            ),
        });
    }
    Ok(())
}

pub fn is_human_only_stage(stage: &StageDef) -> bool {
    stage.is_gate()
        || stage
            .gate_rubric
            .as_ref()
            .is_some_and(|r| r.requires_human_approval)
}

pub fn require_stage_authority(
    actor: &Actor,
    district_id: &DistrictId,
    stage: &StageDef,
    action: &str,
) -> Result<(), TransitionError> {
    if is_human_only_stage(stage) && !actor.is_human() {
        return Err(TransitionError::HumanApprovalRequired {
            action: format!("{action} on human-only stage {district_id}/{}", stage.id),
        });
    }
    Ok(())
}

pub fn require_feedback_authority(
    actor: &Actor,
    kind: FeedbackKind,
) -> Result<(), TransitionError> {
    if kind == FeedbackKind::ApprovalNote && !actor.is_human() {
        return Err(TransitionError::HumanApprovalRequired {
            action: "submit approval_note feedback".into(),
        });
    }
    Ok(())
}

pub fn require_assign_authority(
    actor: &Actor,
    job: &Job,
    current_stage: Option<(&DistrictId, &StageDef)>,
    at: TimestampMs,
) -> Result<(), TransitionError> {
    match &job.location {
        JobLocation::Queue => Ok(()),
        JobLocation::InDistrict {
            district_id,
            stage_id,
            ..
        } => {
            if let Some((cur_dist, stage)) = current_stage {
                require_stage_authority(actor, cur_dist, stage, "reassign job out of")?;
            }
            if !actor.is_human() {
                return Err(TransitionError::JobAlreadyInDistrict {
                    job_id: job.id.clone(),
                    district_id: district_id.clone(),
                    stage_id: stage_id.clone(),
                });
            }
            if let Some(lease) = &job.lease {
                if lease.is_active_at(at) {
                    return Err(TransitionError::AlreadyLeased {
                        job_id: job.id.clone(),
                        holder: lease.runner_label.clone(),
                        expires_at: lease.expires_at,
                    });
                }
            }
            Ok(())
        }
        JobLocation::Completed { .. } | JobLocation::Archived => {
            Err(TransitionError::InvalidLocation {
                job_id: job.id.clone(),
                reason: "cannot assign completed or archived job".into(),
            })
        }
    }
}

pub fn require_update_authority(
    actor: &Actor,
    job: &Job,
    current_stage: Option<(&DistrictId, &StageDef)>,
    provided: Option<FencingToken>,
    at: TimestampMs,
) -> Result<(), TransitionError> {
    match &job.location {
        JobLocation::Queue => Ok(()),
        JobLocation::InDistrict { .. } => {
            if let Some((cur_dist, stage)) = current_stage {
                require_stage_authority(actor, cur_dist, stage, "update job in")?;
            }
            require_mutation_lease(actor, job, provided, at)
        }
        JobLocation::Completed { .. } => {
            if actor.is_human() {
                Ok(())
            } else {
                Err(TransitionError::InvalidLocation {
                    job_id: job.id.clone(),
                    reason: "cannot update completed job as agent".into(),
                })
            }
        }
        JobLocation::Archived => Err(TransitionError::InvalidLocation {
            job_id: job.id.clone(),
            reason: "cannot update archived job".into(),
        }),
    }
}

pub fn require_archive_authority(
    actor: &Actor,
    job: &Job,
    current_stage: Option<(&DistrictId, &StageDef)>,
    provided: Option<FencingToken>,
    at: TimestampMs,
) -> Result<(), TransitionError> {
    match &job.location {
        JobLocation::Queue | JobLocation::Completed { .. } => Ok(()),
        JobLocation::InDistrict { .. } => {
            if let Some((cur_dist, stage)) = current_stage {
                require_stage_authority(actor, cur_dist, stage, "archive job in")?;
            }
            require_mutation_lease(actor, job, provided, at)
        }
        JobLocation::Archived => Err(TransitionError::InvalidLocation {
            job_id: job.id.clone(),
            reason: "job is already archived".into(),
        }),
    }
}

pub fn require_mutation_lease(
    actor: &Actor,
    job: &Job,
    provided: Option<FencingToken>,
    at: TimestampMs,
) -> Result<(), TransitionError> {
    match (actor.is_human(), provided, &job.lease) {
        (true, None, _) => Ok(()),
        (_, Some(tok), Some(lease)) => {
            if lease.fencing_token != tok {
                return Err(TransitionError::StaleFencingToken {
                    job_id: job.id.clone(),
                    provided: tok,
                    current: lease.fencing_token,
                });
            }
            if !lease.is_active_at(at) {
                return Err(TransitionError::LeaseExpired {
                    job_id: job.id.clone(),
                    expired_at: lease.expires_at,
                });
            }
            Ok(())
        }
        (_, Some(_), None) => Err(TransitionError::LeaseMissing {
            job_id: job.id.clone(),
        }),
        (false, None, Some(lease)) => Err(TransitionError::StaleFencingToken {
            job_id: job.id.clone(),
            provided: FencingToken::new(0),
            current: lease.fencing_token,
        }),
        (false, None, None) => Err(TransitionError::LeaseMissing {
            job_id: job.id.clone(),
        }),
    }
}

pub fn require_stage_exit_gate(
    actor: &Actor,
    job: &Job,
    district_id: &DistrictId,
    stage: &StageDef,
) -> Result<(), TransitionError> {
    if actor.is_human() {
        return Ok(());
    }
    let Some(rubric) = &stage.gate_rubric else {
        return Ok(());
    };
    let latest = job
        .gate_verifications
        .iter()
        .rev()
        .find(|gv| gv.stage_id == stage.id);

    let Some(gv) = latest else {
        return Err(TransitionError::GateVerificationRequired {
            job_id: job.id.clone(),
            district_id: district_id.clone(),
            stage_id: stage.id.clone(),
            missing_checks: rubric.checks.clone(),
        });
    };

    if gv.verdict != GateVerdict::Pass {
        return Err(TransitionError::GateVerificationRequired {
            job_id: job.id.clone(),
            district_id: district_id.clone(),
            stage_id: stage.id.clone(),
            missing_checks: vec![format!("latest verdict is {:?}, expected pass", gv.verdict)],
        });
    }

    let mut missing = Vec::new();
    for required_item in &rubric.checks {
        let passed = gv
            .checklist_results
            .iter()
            .any(|(label, ok)| *ok && label.trim() == required_item.trim());
        if !passed {
            missing.push(required_item.clone());
        }
    }
    if !missing.is_empty() {
        return Err(TransitionError::GateVerificationRequired {
            job_id: job.id.clone(),
            district_id: district_id.clone(),
            stage_id: stage.id.clone(),
            missing_checks: missing,
        });
    }

    Ok(())
}
