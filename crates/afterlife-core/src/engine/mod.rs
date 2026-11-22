pub mod command;
pub mod policy;
pub mod validate;
pub mod validation;

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{
    error::{TransitionError, ValidationError},
    fixtures,
    ids::{
        ArtifactId, DistrictId, FeedbackId, FencingToken, JobId, LeaseId, OwnerId, ProposalId,
        StageId, SubMachineTemplateId, TimestampMs,
    },
    model::{
        Actor, Artifact, ArtifactKind, District, ExecutionContext, Feedback, FeedbackKind,
        GateVerification, Job, JobEvent, JobLease, JobLocation, Philosophy, PhilosophyProposal,
        ProcedureSpec, ProposalStatus, SpecProposal, StageDef, SubMachineTemplate,
    },
};

pub use command::EngineCommand;
pub use policy::{MAX_LEASE_TTL_MS, MIN_LEASE_TTL_MS};
pub use validate::{validate_stages, validate_workspace};

fn now_ms() -> TimestampMs {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AfterlifeEngine {
    pub owner_id: OwnerId,
    pub templates: HashMap<SubMachineTemplateId, SubMachineTemplate>,
    pub template_order: Vec<SubMachineTemplateId>,
    pub districts: Vec<District>,
    pub jobs: Vec<Job>,
    pub feedbacks: Vec<Feedback>,
    pub philosophy_proposals: Vec<PhilosophyProposal>,
    pub spec_proposals: Vec<SpecProposal>,
    pub event_log: Vec<JobEvent>,
    next_job_seq: u32,
    next_fencing_token: u64,
    next_feedback_seq: u64,
    next_proposal_seq: u64,
    next_artifact_seq: u64,
}

impl Default for AfterlifeEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AfterlifeEngine {
    pub fn new() -> Self {
        Self::new_for_owner(OwnerId::from(fixtures::LOCAL_OWNER_ID))
    }

    pub fn new_for_owner(owner_id: OwnerId) -> Self {
        let seed = fixtures::default_workspace(&owner_id);
        Self {
            owner_id,
            templates: seed.templates,
            template_order: seed.template_order,
            districts: seed.districts,
            jobs: seed.jobs,
            feedbacks: Vec::new(),
            philosophy_proposals: Vec::new(),
            spec_proposals: Vec::new(),
            event_log: Vec::new(),
            next_job_seq: seed.next_job_seq,
            next_fencing_token: seed.next_fencing_token,
            next_feedback_seq: 1,
            next_proposal_seq: 1,
            next_artifact_seq: 1,
        }
    }

    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        validate_workspace(&self.districts, &self.templates)
    }

    pub fn queued_jobs(&self) -> Vec<&Job> {
        self.jobs
            .iter()
            .filter(|j| matches!(j.location, JobLocation::Queue))
            .collect()
    }

    pub fn approval_jobs(&self) -> Vec<&Job> {
        self.jobs
            .iter()
            .filter(|j| self.is_job_at_approval_gate(j))
            .collect()
    }

    pub fn is_job_at_approval_gate(&self, job: &Job) -> bool {
        if let JobLocation::InDistrict {
            district_id,
            stage_id,
            ..
        } = &job.location
        {
            if let Some(stage) = self.find_stage(district_id, stage_id) {
                return stage.is_gate();
            }
        }
        false
    }

    pub fn find_district(&self, district_id: &DistrictId) -> Option<&District> {
        self.districts.iter().find(|d| &d.id == district_id)
    }

    pub fn find_stage(&self, district_id: &DistrictId, stage_id: &StageId) -> Option<&StageDef> {
        self.find_district(district_id)?
            .stages()
            .iter()
            .find(|s| &s.id == stage_id)
    }

    pub fn find_job(&self, job_id: &JobId) -> Option<&Job> {
        self.jobs.iter().find(|j| &j.id == job_id)
    }

    pub fn compile_execution_context(&self, job_id: &JobId) -> Option<ExecutionContext> {
        let job = self.find_job(job_id)?;
        let JobLocation::InDistrict {
            district_id,
            stage_id,
            sub_stage_id,
        } = &job.location
        else {
            return None;
        };
        let district = self.find_district(district_id)?;
        let philosophy = job
            .pinned_philosophy_version
            .and_then(|v| district.philosophy_at(v))
            .unwrap_or_else(|| district.active_philosophy())
            .clone();
        let spec = job
            .pinned_spec_version
            .and_then(|v| district.spec_at(v))
            .unwrap_or_else(|| district.active_spec());
        let stage = spec.stages.iter().find(|s| &s.id == stage_id)?.clone();
        let sub_stage = match (&stage.sub_template_id, sub_stage_id) {
            (Some(tid), Some(sid)) => self
                .templates
                .get(tid)
                .and_then(|t| t.stages.iter().find(|s| &s.id == sid))
                .cloned(),
            _ => None,
        };
        let recent_feedbacks = self
            .feedbacks
            .iter()
            .rev()
            .filter(|f| &f.district_id == district_id)
            .take(10)
            .cloned()
            .collect();

        Some(ExecutionContext {
            job_id: job.id.clone(),
            owner_id: self.owner_id.clone(),
            district_id: district.id.clone(),
            district_name: district.name.clone(),
            philosophy,
            gate_rubric: stage.gate_rubric.clone(),
            stage,
            sub_stage,
            recent_feedbacks,
        })
    }

    fn alloc_fencing_token(&mut self) -> FencingToken {
        let tok = FencingToken::new(self.next_fencing_token);
        self.next_fencing_token += 1;
        tok
    }

    fn resolve_entry_sub_stage(&self, stage: &StageDef) -> Option<crate::ids::SubStageId> {
        stage
            .sub_template_id
            .as_ref()
            .and_then(|tid| self.templates.get(tid))
            .and_then(|t| t.stages.first())
            .map(|sub| sub.id.clone())
    }

    fn current_stage_for_job<'a>(&'a self, job: &'a Job) -> Option<(&'a DistrictId, &'a StageDef)> {
        if let JobLocation::InDistrict {
            district_id,
            stage_id,
            ..
        } = &job.location
        {
            self.find_stage(district_id, stage_id)
                .map(|s| (district_id, s))
        } else {
            None
        }
    }

    pub fn apply(
        &mut self,
        actor: &Actor,
        cmd: EngineCommand,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        if actor.owner_id() != &self.owner_id {
            return Err(TransitionError::OwnerMismatch {
                expected: self.owner_id.clone(),
                actual: actor.owner_id().clone(),
            });
        }
        validation::validate_command(self, &cmd)?;

        let owner_id = self.owner_id.clone();
        let events = match cmd {
            EngineCommand::QueueJob {
                title,
                raw_input,
                target_district_hint,
                at,
            } => {
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
                    artifacts: vec![Artifact::simple(
                        format!("{id}-drop"),
                        "queue-drop.json",
                        ArtifactKind::RawNote,
                    )],
                    gate_verifications: Vec::new(),
                    feedback_ids: Vec::new(),
                    created_at: at,
                    updated_at: at,
                };
                self.jobs.insert(0, job);
                vec![JobEvent::JobQueued {
                    owner_id,
                    actor: actor.clone(),
                    job_id: id,
                    revision: 1,
                    title,
                    at,
                }]
            }
            EngineCommand::UpdateJob {
                job_id,
                title,
                raw_input,
                spec_summary,
                target_district_hint,
                expected_revision,
                fencing_token,
                at,
            } => {
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
                vec![JobEvent::JobUpdated {
                    owner_id,
                    actor: actor.clone(),
                    job_id,
                    revision: rev,
                    at,
                }]
            }
            EngineCommand::ArchiveJob {
                job_id,
                reason,
                expected_revision,
                fencing_token,
                at,
            } => {
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
                job.revision += 1;
                job.updated_at = at;
                let rev = job.revision;
                vec![JobEvent::JobArchived {
                    owner_id,
                    actor: actor.clone(),
                    job_id,
                    revision: rev,
                    from,
                    reason,
                    at,
                }]
            }
            EngineCommand::PullNextJob { district_id, at } => {
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
            EngineCommand::AssignJobToDistrict {
                job_id,
                district_id,
                expected_revision,
                at,
            } => {
                let district = self
                    .find_district(&district_id)
                    .cloned()
                    .ok_or_else(|| TransitionError::DistrictNotFound(district_id.clone()))?;
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

                vec![JobEvent::JobTransitioned {
                    owner_id,
                    actor: actor.clone(),
                    job_id,
                    revision: job.revision,
                    from,
                    to,
                    claimed_by: None,
                    fencing_token: None,
                    at,
                }]
            }
            EngineCommand::ClaimJob {
                job_id,
                runner_id,
                runner_label,
                ttl_ms,
                expected_revision,
                at,
            } => {
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

                vec![JobEvent::JobClaimed {
                    owner_id,
                    actor: actor.clone(),
                    job_id,
                    revision: job.revision,
                    lease_id,
                    fencing_token: token,
                    runner_label,
                    expires_at,
                    at,
                }]
            }
            EngineCommand::HeartbeatLease {
                job_id,
                fencing_token,
                ttl_ms,
                at,
            } => {
                {
                    let job = self
                        .find_job(&job_id)
                        .ok_or_else(|| TransitionError::JobNotFound(job_id.clone()))?;
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

                vec![JobEvent::JobLeaseRenewed {
                    owner_id,
                    actor: actor.clone(),
                    job_id,
                    lease_id,
                    fencing_token,
                    expires_at,
                    at,
                }]
            }
            EngineCommand::ReleaseLease {
                job_id,
                fencing_token,
                at,
            } => {
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

                vec![JobEvent::JobLeaseReleased {
                    owner_id,
                    actor: actor.clone(),
                    job_id,
                    lease_id,
                    fencing_token,
                    at,
                }]
            }
            EngineCommand::AdvanceJob {
                job_id,
                expected_revision,
                fencing_token,
                at,
            } => {
                let job_snap = self
                    .find_job(&job_id)
                    .cloned()
                    .ok_or_else(|| TransitionError::JobNotFound(job_id.clone()))?;

                if let Some(exp) = expected_revision {
                    if job_snap.revision != exp {
                        return Err(TransitionError::RevisionConflict {
                            job_id,
                            expected: exp,
                            actual: job_snap.revision,
                        });
                    }
                }

                let JobLocation::InDistrict {
                    district_id,
                    stage_id,
                    sub_stage_id,
                } = job_snap.location.clone()
                else {
                    return Err(TransitionError::InvalidLocation {
                        job_id,
                        reason: "job is not currently in an active district stage".into(),
                    });
                };

                let stage = self
                    .find_stage(&district_id, &stage_id)
                    .cloned()
                    .ok_or_else(|| TransitionError::StageNotFound {
                        district_id: district_id.clone(),
                        stage_id: stage_id.clone(),
                    })?;

                policy::require_stage_authority(actor, &district_id, &stage, "advance")?;
                policy::require_mutation_lease(actor, &job_snap, fencing_token, at)?;

                if let (Some(tid), Some(cur_sub_id)) = (&stage.sub_template_id, &sub_stage_id) {
                    if let Some(tmpl) = self.templates.get(tid) {
                        if let Some(pos) = tmpl.stages.iter().position(|s| &s.id == cur_sub_id) {
                            if let Some(next_sub) = tmpl.stages.get(pos + 1).cloned() {
                                let to = JobLocation::InDistrict {
                                    district_id,
                                    stage_id,
                                    sub_stage_id: Some(next_sub.id),
                                };
                                let target = self.jobs.iter_mut().find(|j| j.id == job_id).unwrap();
                                let from = target.location.clone();
                                target.location = to.clone();
                                target.lease = None;
                                target.revision += 1;
                                target.updated_at = at;
                                let ev = vec![JobEvent::JobTransitioned {
                                    owner_id,
                                    actor: actor.clone(),
                                    job_id,
                                    revision: target.revision,
                                    from,
                                    to,
                                    claimed_by: None,
                                    fencing_token: None,
                                    at,
                                }];
                                self.event_log.extend(ev.clone());
                                return Ok(ev);
                            }
                        }
                    }
                }

                policy::require_stage_exit_gate(actor, &job_snap, &district_id, &stage)?;

                let next_stage_id =
                    stage
                        .next
                        .first()
                        .cloned()
                        .ok_or_else(|| TransitionError::NoNextStage {
                            district_id: district_id.clone(),
                            stage_id: stage_id.clone(),
                        })?;
                let next_stage = self
                    .find_stage(&district_id, &next_stage_id)
                    .cloned()
                    .ok_or_else(|| TransitionError::StageNotFound {
                        district_id: district_id.clone(),
                        stage_id: next_stage_id.clone(),
                    })?;

                let next_sub_id = self.resolve_entry_sub_stage(&next_stage);
                let to = if next_stage.is_terminal() {
                    JobLocation::Completed {
                        district_id,
                        terminal_stage_id: next_stage.id,
                        completed_at: at,
                    }
                } else {
                    JobLocation::InDistrict {
                        district_id,
                        stage_id: next_stage.id,
                        sub_stage_id: next_sub_id,
                    }
                };
                let target = self.jobs.iter_mut().find(|j| j.id == job_id).unwrap();
                let from = target.location.clone();
                target.location = to.clone();
                target.lease = None;
                target.revision += 1;
                target.updated_at = at;

                vec![JobEvent::JobTransitioned {
                    owner_id,
                    actor: actor.clone(),
                    job_id,
                    revision: target.revision,
                    from,
                    to,
                    claimed_by: None,
                    fencing_token: None,
                    at,
                }]
            }
            EngineCommand::ReworkJob {
                job_id,
                expected_revision,
                fencing_token,
                reason,
                at,
            } => {
                let job_snap = self
                    .find_job(&job_id)
                    .cloned()
                    .ok_or_else(|| TransitionError::JobNotFound(job_id.clone()))?;

                if let Some(exp) = expected_revision {
                    if job_snap.revision != exp {
                        return Err(TransitionError::RevisionConflict {
                            job_id,
                            expected: exp,
                            actual: job_snap.revision,
                        });
                    }
                }

                let JobLocation::InDistrict {
                    district_id,
                    stage_id,
                    ..
                } = job_snap.location.clone()
                else {
                    return Err(TransitionError::InvalidLocation {
                        job_id,
                        reason: "job is not in an active district stage".into(),
                    });
                };

                let stage = self
                    .find_stage(&district_id, &stage_id)
                    .cloned()
                    .ok_or_else(|| TransitionError::StageNotFound {
                        district_id: district_id.clone(),
                        stage_id: stage_id.clone(),
                    })?;

                policy::require_stage_authority(actor, &district_id, &stage, "rework")?;
                policy::require_mutation_lease(actor, &job_snap, fencing_token, at)?;

                let rework_id = stage
                    .rework_to
                    .ok_or_else(|| TransitionError::NoReworkTarget {
                        district_id: district_id.clone(),
                        stage_id: stage_id.clone(),
                    })?;
                let rework_stage = self
                    .find_stage(&district_id, &rework_id)
                    .cloned()
                    .ok_or_else(|| TransitionError::StageNotFound {
                        district_id: district_id.clone(),
                        stage_id: rework_id.clone(),
                    })?;

                let sub_id = self.resolve_entry_sub_stage(&rework_stage);
                let to = JobLocation::InDistrict {
                    district_id: district_id.clone(),
                    stage_id: rework_stage.id,
                    sub_stage_id: sub_id,
                };

                let mut emitted = Vec::new();
                let fb_id = if let Some(ref body) = reason {
                    let fid = FeedbackId::new(format!("FB-{:04}", self.next_feedback_seq));
                    self.next_feedback_seq += 1;
                    self.feedbacks.push(Feedback {
                        id: fid.clone(),
                        owner_id: owner_id.clone(),
                        district_id: district_id.clone(),
                        job_id: Some(job_id.clone()),
                        stage_id: Some(stage_id.clone()),
                        kind: FeedbackKind::ReworkReason,
                        body: body.clone(),
                        author: actor.clone(),
                        created_at: at,
                    });
                    emitted.push(JobEvent::FeedbackSubmitted {
                        owner_id: owner_id.clone(),
                        actor: actor.clone(),
                        feedback_id: fid.clone(),
                        district_id,
                        job_id: Some(job_id.clone()),
                        stage_id: Some(stage_id),
                        kind: FeedbackKind::ReworkReason,
                        at,
                    });
                    Some(fid)
                } else {
                    None
                };

                let target = self.jobs.iter_mut().find(|j| j.id == job_id).unwrap();
                let from = target.location.clone();
                target.location = to.clone();
                target.lease = None;
                if let Some(fid) = fb_id {
                    target.feedback_ids.push(fid);
                }
                target.revision += 1;
                target.updated_at = at;

                emitted.insert(
                    0,
                    JobEvent::JobReworked {
                        owner_id,
                        actor: actor.clone(),
                        job_id,
                        revision: target.revision,
                        from,
                        to,
                        reason,
                        at,
                    },
                );
                emitted
            }
            EngineCommand::AttachArtifact {
                job_id,
                kind,
                label,
                uri,
                sha256,
                fencing_token,
                at,
            } => {
                let job_snap = self
                    .find_job(&job_id)
                    .cloned()
                    .ok_or_else(|| TransitionError::JobNotFound(job_id.clone()))?;
                if let Some((cur_dist, cur_stage)) = self.current_stage_for_job(&job_snap) {
                    policy::require_stage_authority(actor, cur_dist, cur_stage, "attach artifact")?;
                }
                policy::require_mutation_lease(actor, &job_snap, fencing_token, at)?;

                let produced_at_stage = match &job_snap.location {
                    JobLocation::InDistrict { stage_id, .. } => Some(stage_id.clone()),
                    _ => None,
                };
                let art_id = ArtifactId::new(format!("art-{:05}", self.next_artifact_seq));
                self.next_artifact_seq += 1;

                let target = self.jobs.iter_mut().find(|j| j.id == job_id).unwrap();
                target.revision += 1;
                target.updated_at = at;
                target.artifacts.push(Artifact {
                    id: art_id.clone(),
                    kind,
                    label: label.clone(),
                    uri: uri.clone(),
                    sha256,
                    produced_at_stage,
                    revision_at_capture: target.revision,
                });

                vec![JobEvent::ArtifactAttached {
                    owner_id,
                    actor: actor.clone(),
                    job_id,
                    revision: target.revision,
                    artifact_id: art_id,
                    kind,
                    label,
                    uri,
                    at,
                }]
            }
            EngineCommand::RecordGateVerification {
                job_id,
                verdict,
                summary,
                checklist_results,
                fencing_token,
                at,
            } => {
                let job_snap = self
                    .find_job(&job_id)
                    .cloned()
                    .ok_or_else(|| TransitionError::JobNotFound(job_id.clone()))?;

                let JobLocation::InDistrict {
                    district_id,
                    stage_id,
                    ..
                } = job_snap.location.clone()
                else {
                    return Err(TransitionError::InvalidLocation {
                        job_id,
                        reason: "job must be in a district stage to record gate verification"
                            .into(),
                    });
                };

                let stage = self
                    .find_stage(&district_id, &stage_id)
                    .cloned()
                    .ok_or_else(|| TransitionError::StageNotFound {
                        district_id: district_id.clone(),
                        stage_id: stage_id.clone(),
                    })?;

                policy::require_stage_authority(
                    actor,
                    &district_id,
                    &stage,
                    "record gate verification",
                )?;
                policy::require_mutation_lease(actor, &job_snap, fencing_token, at)?;

                let target = self.jobs.iter_mut().find(|j| j.id == job_id).unwrap();
                target.gate_verifications.push(GateVerification {
                    stage_id: stage_id.clone(),
                    verdict,
                    summary: summary.clone(),
                    checklist_results,
                    verifier: actor.clone(),
                    verified_at: at,
                });
                target.updated_at = at;

                vec![JobEvent::GateVerificationRecorded {
                    owner_id,
                    actor: actor.clone(),
                    job_id,
                    stage_id,
                    verdict,
                    summary,
                    at,
                }]
            }
            EngineCommand::SubmitFeedback {
                district_id,
                job_id,
                stage_id,
                kind,
                body,
                at,
            } => {
                policy::require_feedback_authority(actor, kind)?;
                if self.find_district(&district_id).is_none() {
                    return Err(TransitionError::DistrictNotFound(district_id));
                }
                let fid = FeedbackId::new(format!("FB-{:04}", self.next_feedback_seq));
                self.next_feedback_seq += 1;
                self.feedbacks.push(Feedback {
                    id: fid.clone(),
                    owner_id: owner_id.clone(),
                    district_id: district_id.clone(),
                    job_id: job_id.clone(),
                    stage_id: stage_id.clone(),
                    kind,
                    body,
                    author: actor.clone(),
                    created_at: at,
                });
                if let Some(ref jid) = job_id {
                    if let Some(job) = self.jobs.iter_mut().find(|j| &j.id == jid) {
                        job.feedback_ids.push(fid.clone());
                    }
                }
                vec![JobEvent::FeedbackSubmitted {
                    owner_id,
                    actor: actor.clone(),
                    feedback_id: fid,
                    district_id,
                    job_id,
                    stage_id,
                    kind,
                    at,
                }]
            }
            EngineCommand::ProposePhilosophyUpdate {
                district_id,
                base_version,
                summary,
                principles,
                anti_patterns,
                decision_heuristics,
                rationale,
                source_feedback_ids,
                at,
            } => {
                let district = self
                    .find_district(&district_id)
                    .ok_or_else(|| TransitionError::DistrictNotFound(district_id.clone()))?;
                if district.active_philosophy_version != base_version {
                    return Err(TransitionError::PhilosophyVersionConflict {
                        district_id,
                        expected: base_version,
                        actual: district.active_philosophy_version,
                    });
                }
                let next_ver = base_version.next();
                let pid = ProposalId::new(format!("PROP-PHIL-{:04}", self.next_proposal_seq));
                self.next_proposal_seq += 1;
                let proposed = Philosophy {
                    version: next_ver,
                    summary,
                    principles,
                    anti_patterns,
                    decision_heuristics,
                    source_feedback_ids: source_feedback_ids.clone(),
                    updated_by: actor.clone(),
                    updated_at: at,
                };
                self.philosophy_proposals.push(PhilosophyProposal {
                    id: pid.clone(),
                    owner_id: owner_id.clone(),
                    district_id: district_id.clone(),
                    base_version,
                    proposed,
                    rationale,
                    source_feedback_ids,
                    proposed_by: actor.clone(),
                    status: ProposalStatus::Pending,
                    created_at: at,
                    resolved_at: None,
                });
                vec![JobEvent::PhilosophyProposed {
                    owner_id,
                    actor: actor.clone(),
                    proposal_id: pid,
                    district_id,
                    base_version,
                    proposed_version: next_ver,
                    at,
                }]
            }
            EngineCommand::ApprovePhilosophyProposal { proposal_id, at } => {
                if !actor.is_human() {
                    return Err(TransitionError::HumanApprovalRequired {
                        action: format!("approve philosophy proposal {proposal_id}"),
                    });
                }
                let prop = self
                    .philosophy_proposals
                    .iter()
                    .find(|p| p.id == proposal_id)
                    .cloned()
                    .ok_or_else(|| TransitionError::ProposalNotFound(proposal_id.clone()))?;
                if prop.status != ProposalStatus::Pending {
                    return Err(TransitionError::ProposalAlreadyResolved(proposal_id));
                }
                let district = self
                    .districts
                    .iter_mut()
                    .find(|d| d.id == prop.district_id)
                    .ok_or_else(|| TransitionError::DistrictNotFound(prop.district_id.clone()))?;
                if district.active_philosophy_version != prop.base_version {
                    return Err(TransitionError::PhilosophyVersionConflict {
                        district_id: prop.district_id,
                        expected: prop.base_version,
                        actual: district.active_philosophy_version,
                    });
                }
                let new_version = prop.proposed.version;
                district.philosophy_history.push(prop.proposed);
                district.active_philosophy_version = new_version;

                let p_mut = self
                    .philosophy_proposals
                    .iter_mut()
                    .find(|p| p.id == proposal_id)
                    .unwrap();
                p_mut.status = ProposalStatus::Approved;
                p_mut.resolved_at = Some(at);

                vec![JobEvent::PhilosophyUpdated {
                    owner_id,
                    actor: actor.clone(),
                    proposal_id,
                    district_id: prop.district_id,
                    new_version,
                    at,
                }]
            }
            EngineCommand::ProposeSpecUpdate {
                district_id,
                base_version,
                stages,
                changelog,
                rationale,
                source_feedback_ids,
                at,
            } => {
                let district = self
                    .find_district(&district_id)
                    .ok_or_else(|| TransitionError::DistrictNotFound(district_id.clone()))?;
                if district.active_spec_version != base_version {
                    return Err(TransitionError::SpecVersionConflict {
                        district_id,
                        expected: base_version,
                        actual: district.active_spec_version,
                    });
                }
                let errs = validate_stages(&district_id, &stages, &self.templates);
                if !errs.is_empty() {
                    return Err(TransitionError::InvalidProposedSpec {
                        district_id,
                        reasons: errs.into_iter().map(|e| e.to_string()).collect(),
                    });
                }
                let next_ver = base_version.next();
                let pid = ProposalId::new(format!("PROP-SPEC-{:04}", self.next_proposal_seq));
                self.next_proposal_seq += 1;
                let proposed = ProcedureSpec {
                    version: next_ver,
                    stages,
                    changelog,
                    source_feedback_ids: source_feedback_ids.clone(),
                    updated_by: actor.clone(),
                    updated_at: at,
                };
                self.spec_proposals.push(SpecProposal {
                    id: pid.clone(),
                    owner_id: owner_id.clone(),
                    district_id: district_id.clone(),
                    base_version,
                    proposed,
                    rationale,
                    source_feedback_ids,
                    proposed_by: actor.clone(),
                    status: ProposalStatus::Pending,
                    created_at: at,
                    resolved_at: None,
                });
                vec![JobEvent::SpecProposed {
                    owner_id,
                    actor: actor.clone(),
                    proposal_id: pid,
                    district_id,
                    base_version,
                    proposed_version: next_ver,
                    at,
                }]
            }
            EngineCommand::ApproveSpecProposal { proposal_id, at } => {
                if !actor.is_human() {
                    return Err(TransitionError::HumanApprovalRequired {
                        action: format!("approve spec proposal {proposal_id}"),
                    });
                }
                let prop = self
                    .spec_proposals
                    .iter()
                    .find(|p| p.id == proposal_id)
                    .cloned()
                    .ok_or_else(|| TransitionError::ProposalNotFound(proposal_id.clone()))?;
                if prop.status != ProposalStatus::Pending {
                    return Err(TransitionError::ProposalAlreadyResolved(proposal_id));
                }
                let district = self
                    .districts
                    .iter_mut()
                    .find(|d| d.id == prop.district_id)
                    .ok_or_else(|| TransitionError::DistrictNotFound(prop.district_id.clone()))?;
                if district.active_spec_version != prop.base_version {
                    return Err(TransitionError::SpecVersionConflict {
                        district_id: prop.district_id,
                        expected: prop.base_version,
                        actual: district.active_spec_version,
                    });
                }
                let new_version = prop.proposed.version;
                district.spec_history.push(prop.proposed);
                district.active_spec_version = new_version;

                let p_mut = self
                    .spec_proposals
                    .iter_mut()
                    .find(|p| p.id == proposal_id)
                    .unwrap();
                p_mut.status = ProposalStatus::Approved;
                p_mut.resolved_at = Some(at);

                vec![JobEvent::SpecUpdated {
                    owner_id,
                    actor: actor.clone(),
                    proposal_id,
                    district_id: prop.district_id,
                    new_version,
                    at,
                }]
            }
            EngineCommand::SetStageSubTemplate {
                district_id,
                stage_id,
                template_id,
                at,
            } => {
                if let Some(ref tid) = template_id {
                    if !self.templates.contains_key(tid) {
                        return Err(TransitionError::TemplateNotFound(tid.clone()));
                    }
                }
                let district = self
                    .districts
                    .iter_mut()
                    .find(|d| d.id == district_id)
                    .ok_or_else(|| TransitionError::DistrictNotFound(district_id.clone()))?;
                let stage = district
                    .active_spec_mut()
                    .stages
                    .iter_mut()
                    .find(|s| s.id == stage_id)
                    .ok_or_else(|| TransitionError::StageNotFound {
                        district_id: district_id.clone(),
                        stage_id: stage_id.clone(),
                    })?;
                if !stage.supports_sub_machine() {
                    return Err(TransitionError::SubTemplateNotAllowed {
                        district_id,
                        stage_id,
                    });
                }
                stage.sub_template_id = template_id.clone();

                vec![JobEvent::StageSubTemplateChanged {
                    owner_id,
                    actor: actor.clone(),
                    district_id,
                    stage_id,
                    template_id,
                    at,
                }]
            }
        };

        self.event_log.extend(events.clone());
        Ok(events)
    }

    pub fn local_actor(&self) -> Actor {
        Actor::local_human(self.owner_id.clone())
    }

    pub fn queue_preset_job(&mut self, title: &str, hint: Option<&DistrictId>) -> JobId {
        let actor = self.local_actor();
        let events = self
            .apply(
                &actor,
                EngineCommand::QueueJob {
                    title: title.to_string(),
                    raw_input: title.to_string(),
                    target_district_hint: hint.cloned(),
                    at: now_ms(),
                },
            )
            .expect("QueueJob cannot fail");
        match events.into_iter().next() {
            Some(JobEvent::JobQueued { job_id, .. }) => job_id,
            _ => unreachable!(),
        }
    }

    pub fn pull_next_job_into_district(
        &mut self,
        district_id: &DistrictId,
    ) -> Result<JobId, TransitionError> {
        let actor = self.local_actor();
        let events = self.apply(
            &actor,
            EngineCommand::PullNextJob {
                district_id: district_id.clone(),
                at: now_ms(),
            },
        )?;
        match events.into_iter().next() {
            Some(JobEvent::JobTransitioned { job_id, .. }) => Ok(job_id),
            _ => Err(TransitionError::QueueEmpty),
        }
    }

    pub fn assign_job_to_district(
        &mut self,
        job_id: &JobId,
        district_id: &DistrictId,
    ) -> Result<(), TransitionError> {
        let actor = self.local_actor();
        self.apply(
            &actor,
            EngineCommand::AssignJobToDistrict {
                job_id: job_id.clone(),
                district_id: district_id.clone(),
                expected_revision: None,
                at: now_ms(),
            },
        )
        .map(|_| ())
    }

    pub fn advance_job(&mut self, job_id: &JobId) -> Result<(), TransitionError> {
        let actor = self.local_actor();
        self.apply(
            &actor,
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                expected_revision: None,
                fencing_token: None,
                at: now_ms(),
            },
        )
        .map(|_| ())
    }

    pub fn rework_job(&mut self, job_id: &JobId) -> Result<(), TransitionError> {
        let actor = self.local_actor();
        self.apply(
            &actor,
            EngineCommand::ReworkJob {
                job_id: job_id.clone(),
                expected_revision: None,
                fencing_token: None,
                reason: None,
                at: now_ms(),
            },
        )
        .map(|_| ())
    }

    pub fn cycle_stage_sub_template(&mut self, district_id: &DistrictId, stage_id: &StageId) {
        let Some(stage) = self.find_stage(district_id, stage_id) else {
            return;
        };
        if !stage.supports_sub_machine() {
            return;
        }
        let next_tmpl = match &stage.sub_template_id {
            None => self.template_order.first().cloned(),
            Some(cur) => {
                let pos = self.template_order.iter().position(|id| id == cur);
                match pos {
                    Some(i) if i + 1 < self.template_order.len() => {
                        Some(self.template_order[i + 1].clone())
                    }
                    _ => None,
                }
            }
        };
        let actor = self.local_actor();
        let _ = self.apply(
            &actor,
            EngineCommand::SetStageSubTemplate {
                district_id: district_id.clone(),
                stage_id: stage_id.clone(),
                template_id: next_tmpl,
                at: now_ms(),
            },
        );
    }
}
