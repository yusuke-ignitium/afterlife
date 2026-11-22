use crate::{
    engine::AfterlifeEngine,
    error::TransitionError,
    ids::{JobId, OwnerId, TimestampMs},
    model::{Actor, JobEvent, JobLocation},
};

impl AfterlifeEngine {
    pub(super) fn apply_stop_job(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        job_id: JobId,
        reason: Option<String>,
        expected_revision: Option<u64>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        if !actor.is_human() {
            return Err(TransitionError::HumanApprovalRequired {
                action: format!("stop job {job_id}"),
            });
        }
        let job_snap = self
            .find_job(&job_id)
            .cloned()
            .ok_or_else(|| TransitionError::JobNotFound(job_id.clone()))?;
        if let Some(exp) = expected_revision
            && job_snap.revision != exp
        {
            return Err(TransitionError::RevisionConflict {
                job_id,
                expected: exp,
                actual: job_snap.revision,
            });
        }
        let JobLocation::InDistrict {
            district_id,
            stage_id,
            ..
        } = &job_snap.location
        else {
            return Err(TransitionError::InvalidLocation {
                job_id,
                reason: "only in-district jobs can be stopped".into(),
            });
        };
        let stage = self
            .find_stage_for_job(&job_snap, district_id, stage_id)
            .ok_or_else(|| TransitionError::StageNotFound {
                district_id: district_id.clone(),
                stage_id: stage_id.clone(),
            })?;
        if stage.is_terminal() {
            return Err(TransitionError::InvalidLocation {
                job_id,
                reason: "cannot stop a job on a terminal stage".into(),
            });
        }
        if job_snap.stopped {
            return Err(TransitionError::JobStopped { job_id });
        }

        let target = self.jobs.iter_mut().find(|j| j.id == job_id).unwrap();
        let revoked_fencing_token = target.lease.take().map(|l| l.fencing_token);
        let normalized_reason = reason.map(|r| r.trim().to_string());
        target.stopped = true;
        target.stopped_at = Some(at);
        target.stopped_reason = normalized_reason.clone();
        target.revision += 1;
        target.updated_at = at;
        let rev = target.revision;

        Ok(vec![JobEvent::JobStopped {
            owner_id,
            actor: actor.clone(),
            job_id,
            revision: rev,
            revoked_fencing_token,
            reason: normalized_reason,
            at,
        }])
    }

    pub(super) fn apply_resume_job(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        job_id: JobId,
        expected_revision: Option<u64>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        if !actor.is_human() {
            return Err(TransitionError::HumanApprovalRequired {
                action: format!("resume job {job_id}"),
            });
        }
        let job_snap = self
            .find_job(&job_id)
            .cloned()
            .ok_or_else(|| TransitionError::JobNotFound(job_id.clone()))?;
        if let Some(exp) = expected_revision
            && job_snap.revision != exp
        {
            return Err(TransitionError::RevisionConflict {
                job_id,
                expected: exp,
                actual: job_snap.revision,
            });
        }
        let JobLocation::InDistrict {
            district_id,
            stage_id,
            ..
        } = &job_snap.location
        else {
            return Err(TransitionError::InvalidLocation {
                job_id,
                reason: "only in-district jobs can be resumed".into(),
            });
        };
        let stage = self
            .find_stage_for_job(&job_snap, district_id, stage_id)
            .ok_or_else(|| TransitionError::StageNotFound {
                district_id: district_id.clone(),
                stage_id: stage_id.clone(),
            })?;
        if stage.is_terminal() {
            return Err(TransitionError::InvalidLocation {
                job_id,
                reason: "cannot resume a job on a terminal stage".into(),
            });
        }
        if !job_snap.stopped {
            return Err(TransitionError::JobNotStopped { job_id });
        }

        let target = self.jobs.iter_mut().find(|j| j.id == job_id).unwrap();
        target.stopped = false;
        target.stopped_at = None;
        target.stopped_reason = None;
        target.revision += 1;
        target.updated_at = at;
        let rev = target.revision;

        Ok(vec![JobEvent::JobResumed {
            owner_id,
            actor: actor.clone(),
            job_id,
            revision: rev,
            at,
        }])
    }
}
