use crate::{
    engine::{AfterlifeEngine, EngineCommand, policy},
    error::TransitionError,
    ids::{DistrictId, FencingToken, JobId, LeaseId, OwnerId, TimestampMs},
    model::{Actor, Artifact, ArtifactKind, Job, JobEvent, JobLease, JobLocation},
};

impl AfterlifeEngine {
    pub(super) fn apply_queue_job(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        title: String,
        raw_input: String,
        target_district_hint: Option<DistrictId>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        let id = JobId::new(format!("JOB-{:03}", self.next_job_seq));
        self.next_job_seq += 1;
        let spec_summary = if raw_input.trim().is_empty() {
            "Added to Jobs. Ready for Runner claim.".into()
        } else {
            raw_input.clone()
        };
        let job = Job {
            id: id.clone(),
            owner_id: owner_id.clone(),
            revision: 1,
            title: title.clone(),
            raw_input,
            spec_summary,
            location: JobLocation::Queue,
            target_district_hint,
            pinned_philosophy_version: None,
            pinned_spec_version: None,
            lease: None,
            stopped: false,
            stopped_at: None,
            stopped_reason: None,
            artifacts: vec![Artifact::simple(
                format!("{id}-drop"),
                "queue-drop.json",
                ArtifactKind::RawNote,
            )],
            gate_verifications: Vec::new(),
            step_records: Vec::new(),
            feedback_ids: Vec::new(),
            created_at: at,
            updated_at: at,
        };
        self.jobs.insert(0, job);
        Ok(vec![JobEvent::JobQueued {
            owner_id,
            actor: actor.clone(),
            job_id: id,
            revision: 1,
            title,
            at,
        }])
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_update_job(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        job_id: JobId,
        title: Option<String>,
        raw_input: Option<String>,
        spec_summary: Option<String>,
        target_district_hint: Option<DistrictId>,
        expected_revision: Option<u64>,
        fencing_token: Option<FencingToken>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        {
            let job = self
                .find_job(&job_id)
                .ok_or_else(|| TransitionError::JobNotFound(job_id.clone()))?;
            if let Some(exp) = expected_revision {
                if job.revision != exp {
                    return Err(TransitionError::RevisionConflict {
                        job_id,
                        expected: exp,
                        actual: job.revision,
                    });
                }
            }
            let cur_stage = self.current_stage_for_job(job);
            policy::require_update_authority(actor, job, cur_stage, fencing_token, at)?;
        }
        let job = self
            .jobs
            .iter_mut()
            .find(|j| j.id == job_id)
            .ok_or_else(|| TransitionError::JobNotFound(job_id.clone()))?;
        if let Some(t) = title {
            job.title = t;
        }
        if let Some(r) = raw_input {
            job.raw_input = r;
        }
        if let Some(s) = spec_summary {
            job.spec_summary = s;
        }
        if let Some(d) = target_district_hint {
            job.target_district_hint = Some(d);
        }
        job.revision += 1;
        job.updated_at = at;
        let rev = job.revision;
        Ok(vec![JobEvent::JobUpdated {
            owner_id,
            actor: actor.clone(),
            job_id,
            revision: rev,
            at,
        }])
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_archive_job(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        job_id: JobId,
        reason: Option<String>,
        expected_revision: Option<u64>,
        fencing_token: Option<FencingToken>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        {
            let job = self
                .find_job(&job_id)
                .ok_or_else(|| TransitionError::JobNotFound(job_id.clone()))?;
            if let Some(exp) = expected_revision {
                if job.revision != exp {
                    return Err(TransitionError::RevisionConflict {
                        job_id,
                        expected: exp,
                        actual: job.revision,
                    });
                }
            }
            let cur_stage = self.current_stage_for_job(job);
            policy::require_archive_authority(actor, job, cur_stage, fencing_token, at)?;
        }
        let job = self
            .jobs
            .iter_mut()
            .find(|j| j.id == job_id)
            .ok_or_else(|| TransitionError::JobNotFound(job_id.clone()))?;
        let from = job.location.clone();
        job.location = JobLocation::Archived;
        job.lease = None;
        job.stopped = false;
        job.stopped_at = None;
        job.stopped_reason = None;
        job.revision += 1;
        job.updated_at = at;
        let rev = job.revision;
        Ok(vec![JobEvent::JobArchived {
            owner_id,
            actor: actor.clone(),
            job_id,
            revision: rev,
            from,
            reason,
            at,
        }])
    }

    pub(super) fn apply_pull_next_job(
        &mut self,
        actor: &Actor,
        district_id: DistrictId,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        let district = self
            .find_district(&district_id)
            .ok_or_else(|| TransitionError::DistrictNotFound(district_id.clone()))?;
        if district.archived {
            return Err(TransitionError::DistrictArchived(district_id));
        }
        let Some(job_id) = self
            .jobs
            .iter()
            .find(|j| matches!(j.location, JobLocation::Queue))
            .map(|j| j.id.clone())
        else {
            return Err(TransitionError::QueueEmpty);
        };
        return self.apply(
            actor,
            EngineCommand::AssignJobToDistrict {
                job_id,
                district_id,
                expected_revision: None,
                at,
            },
        );
    }

    pub(super) fn apply_assign_job_to_district(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        job_id: JobId,
        district_id: DistrictId,
        expected_revision: Option<u64>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        let district = self
            .find_district(&district_id)
            .cloned()
            .ok_or_else(|| TransitionError::DistrictNotFound(district_id.clone()))?;
        if district.archived {
            return Err(TransitionError::DistrictArchived(district_id));
        }
        let first_stage = district
            .stages()
            .first()
            .cloned()
            .ok_or_else(|| TransitionError::DistrictHasNoStages(district_id.clone()))?;

        {
            let job = self
                .find_job(&job_id)
                .ok_or_else(|| TransitionError::JobNotFound(job_id.clone()))?;
            if let Some(exp) = expected_revision {
                if job.revision != exp {
                    return Err(TransitionError::RevisionConflict {
                        job_id,
                        expected: exp,
                        actual: job.revision,
                    });
                }
            }
            let cur_stage = self.current_stage_for_job(job);
            policy::require_assign_authority(actor, job, cur_stage, at)?;
        }

        let sub_stage_id = self.resolve_entry_sub_stage(&first_stage);
        let job = self.jobs.iter_mut().find(|j| j.id == job_id).unwrap();
        let from = job.location.clone();
        let to = if first_stage.is_terminal() {
            JobLocation::Completed {
                district_id: district.id.clone(),
                terminal_stage_id: first_stage.id.clone(),
                completed_at: at,
            }
        } else {
            JobLocation::InDistrict {
                district_id: district.id.clone(),
                stage_id: first_stage.id.clone(),
                sub_stage_id,
            }
        };
        job.location = to.clone();
        job.pinned_philosophy_version = Some(district.active_philosophy_version);
        job.pinned_spec_version = Some(district.active_spec_version);
        job.lease = None;
        job.revision += 1;
        job.updated_at = at;

        Ok(vec![JobEvent::JobTransitioned {
            owner_id,
            actor: actor.clone(),
            job_id,
            revision: job.revision,
            from,
            to,
            claimed_by: None,
            fencing_token: None,
            at,
        }])
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_claim_job(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        job_id: JobId,
        runner_id: crate::ids::RunnerId,
        runner_label: String,
        ttl_ms: TimestampMs,
        expected_revision: Option<u64>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        {
            let job = self
                .find_job(&job_id)
                .ok_or_else(|| TransitionError::JobNotFound(job_id.clone()))?;
            if let Some(exp) = expected_revision {
                if job.revision != exp {
                    return Err(TransitionError::RevisionConflict {
                        job_id,
                        expected: exp,
                        actual: job.revision,
                    });
                }
            }
            if matches!(
                job.location,
                JobLocation::Completed { .. } | JobLocation::Archived
            ) {
                return Err(TransitionError::InvalidLocation {
                    job_id,
                    reason: "cannot claim completed or archived job".into(),
                });
            }
            policy::require_not_stopped(job)?;
            if let Some((cur_dist, cur_stage)) = self.current_stage_for_job(job) {
                policy::require_stage_authority(actor, cur_dist, cur_stage, "claim")?;
            }
            if let Some(existing) = &job.lease {
                if existing.is_active_at(at) {
                    return Err(TransitionError::AlreadyLeased {
                        job_id,
                        holder: existing.runner_label.clone(),
                        expires_at: existing.expires_at,
                    });
                }
            }
        }

        let token = self.alloc_fencing_token();
        let lease_id = LeaseId::new(format!("lease-{}", token.get()));
        let expires_at = at.saturating_add(ttl_ms);
        let holder_client_id = match actor {
            Actor::Agent { client_id, .. } => Some(client_id.clone()),
            Actor::Human { .. } => None,
        };
        let lease = JobLease {
            lease_id: lease_id.clone(),
            fencing_token: token,
            runner_id,
            runner_label: runner_label.clone(),
            holder_client_id,
            acquired_at: at,
            expires_at,
        };
        let job = self.jobs.iter_mut().find(|j| j.id == job_id).unwrap();
        job.lease = Some(lease);
        job.revision += 1;
        job.updated_at = at;

        Ok(vec![JobEvent::JobClaimed {
            owner_id,
            actor: actor.clone(),
            job_id,
            revision: job.revision,
            lease_id,
            fencing_token: token,
            runner_label,
            expires_at,
            at,
        }])
    }

    pub(super) fn apply_heartbeat_lease(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        job_id: JobId,
        fencing_token: FencingToken,
        ttl_ms: TimestampMs,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        {
            let job = self
                .find_job(&job_id)
                .ok_or_else(|| TransitionError::JobNotFound(job_id.clone()))?;
            policy::require_not_stopped(job)?;
            if let Some((cur_dist, cur_stage)) = self.current_stage_for_job(job) {
                policy::require_stage_authority(actor, cur_dist, cur_stage, "heartbeat")?;
            }
        }
        let job = self
            .jobs
            .iter_mut()
            .find(|j| j.id == job_id)
            .ok_or_else(|| TransitionError::JobNotFound(job_id.clone()))?;
        let lease = job
            .lease
            .as_mut()
            .ok_or_else(|| TransitionError::LeaseMissing {
                job_id: job_id.clone(),
            })?;
        if lease.fencing_token != fencing_token {
            return Err(TransitionError::StaleFencingToken {
                job_id,
                provided: fencing_token,
                current: lease.fencing_token,
            });
        }
        if !lease.is_active_at(at) {
            return Err(TransitionError::LeaseExpired {
                job_id,
                expired_at: lease.expires_at,
            });
        }
        lease.expires_at = at.saturating_add(ttl_ms);
        let lease_id = lease.lease_id.clone();
        let expires_at = lease.expires_at;
        job.updated_at = at;

        Ok(vec![JobEvent::JobLeaseRenewed {
            owner_id,
            actor: actor.clone(),
            job_id,
            lease_id,
            fencing_token,
            expires_at,
            at,
        }])
    }

    pub(super) fn apply_release_lease(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        job_id: JobId,
        fencing_token: FencingToken,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        let job = self
            .jobs
            .iter_mut()
            .find(|j| j.id == job_id)
            .ok_or_else(|| TransitionError::JobNotFound(job_id.clone()))?;
        let lease = job
            .lease
            .as_ref()
            .ok_or_else(|| TransitionError::LeaseMissing {
                job_id: job_id.clone(),
            })?;
        if lease.fencing_token != fencing_token {
            return Err(TransitionError::StaleFencingToken {
                job_id,
                provided: fencing_token,
                current: lease.fencing_token,
            });
        }
        let lease_id = lease.lease_id.clone();
        job.lease = None;
        job.revision += 1;
        job.updated_at = at;

        Ok(vec![JobEvent::JobLeaseReleased {
            owner_id,
            actor: actor.clone(),
            job_id,
            lease_id,
            fencing_token,
            at,
        }])
    }
}
