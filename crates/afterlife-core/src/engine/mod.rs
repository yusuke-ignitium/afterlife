pub mod command;
mod district_governance;
mod evidence;
mod governance;
mod job_lifecycle;
pub mod policy;
mod stop_resume;
mod transitions;
pub mod validate;
pub mod validation;

use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{
    error::{TransitionError, ValidationError},
    fixtures,
    ids::{DistrictId, FencingToken, JobId, OwnerId, StageId, SubMachineTemplateId, TimestampMs},
    model::{
        Actor, District, DistrictProposal, ExecutionContext, Feedback, Job, JobEvent, JobLocation,
        PhilosophyProposal, SpecProposal, StageDef, SubMachineTemplate, TemplateProposal,
    },
};

pub use command::EngineCommand;
pub use policy::{MAX_LEASE_TTL_MS, MIN_LEASE_TTL_MS};
pub use validate::{validate_stages, validate_template, validate_workspace};

#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> TimestampMs {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(target_arch = "wasm32")]
fn now_ms() -> TimestampMs {
    0
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
    #[serde(default)]
    pub template_proposals: Vec<TemplateProposal>,
    #[serde(default)]
    pub district_proposals: Vec<DistrictProposal>,
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
            template_proposals: Vec::new(),
            district_proposals: Vec::new(),
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

    pub fn find_stage_for_job<'a>(
        &'a self,
        job: &Job,
        district_id: &DistrictId,
        stage_id: &StageId,
    ) -> Option<&'a StageDef> {
        let district = self.find_district(district_id)?;
        let use_pinned = match &job.location {
            JobLocation::InDistrict {
                district_id: cur, ..
            }
            | JobLocation::Completed {
                district_id: cur, ..
            } => cur == district_id,
            JobLocation::Queue | JobLocation::Archived => true,
        };
        if use_pinned
            && let Some(stage) = job
                .pinned_spec_version
                .and_then(|v| district.spec_at(v))
                .and_then(|spec| spec.stages.iter().find(|s| &s.id == stage_id))
        {
            return Some(stage);
        }
        district.stages().iter().find(|s| &s.id == stage_id)
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
        let (required_step_fields, required_artifact_kinds) =
            policy::resolve_step_contract(&stage, sub_stage.as_ref());
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
            stopped: job.stopped,
            stopped_at: job.stopped_at,
            stopped_reason: job.stopped_reason.clone(),
            philosophy,
            gate_rubric: stage.gate_rubric.clone(),
            stage,
            sub_stage,
            required_step_fields,
            required_artifact_kinds,
            step_records: job.step_records.clone(),
            artifacts: job.artifacts.clone(),
            recent_feedbacks,
        })
    }

    pub(super) fn alloc_fencing_token(&mut self) -> FencingToken {
        let tok = FencingToken::new(self.next_fencing_token);
        self.next_fencing_token += 1;
        tok
    }

    pub(super) fn resolve_entry_sub_stage(&self, stage: &StageDef) -> Option<crate::ids::SubStageId> {
        stage
            .sub_template_id
            .as_ref()
            .and_then(|tid| self.templates.get(tid))
            .and_then(|t| t.stages.first())
            .map(|sub| sub.id.clone())
    }

    pub(super) fn current_stage_for_job<'a>(&'a self, job: &'a Job) -> Option<(&'a DistrictId, &'a StageDef)> {
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
            EngineCommand::QueueJob { title, raw_input, target_district_hint, at } => {
                self.apply_queue_job(owner_id, actor, title, raw_input, target_district_hint, at)?
            }
            EngineCommand::UpdateJob {
                job_id, title, raw_input, spec_summary, target_district_hint,
                expected_revision, fencing_token, at,
            } => self.apply_update_job(
                owner_id, actor, job_id, title, raw_input, spec_summary,
                target_district_hint, expected_revision, fencing_token, at,
            )?,
            EngineCommand::ArchiveJob {
                job_id, reason, expected_revision, fencing_token, at,
            } => self.apply_archive_job(owner_id, actor, job_id, reason, expected_revision, fencing_token, at)?,
            EngineCommand::PullNextJob { district_id, at } => {
                return self.apply_pull_next_job(actor, district_id, at);
            }
            EngineCommand::AssignJobToDistrict {
                job_id, district_id, expected_revision, at,
            } => self.apply_assign_job_to_district(owner_id, actor, job_id, district_id, expected_revision, at)?,
            EngineCommand::ClaimJob {
                job_id, runner_id, runner_label, ttl_ms, expected_revision, at,
            } => self.apply_claim_job(owner_id, actor, job_id, runner_id, runner_label, ttl_ms, expected_revision, at)?,
            EngineCommand::HeartbeatLease { job_id, fencing_token, ttl_ms, at } => {
                self.apply_heartbeat_lease(owner_id, actor, job_id, fencing_token, ttl_ms, at)?
            }
            EngineCommand::ReleaseLease { job_id, fencing_token, at } => {
                self.apply_release_lease(owner_id, actor, job_id, fencing_token, at)?
            }
            EngineCommand::AdvanceJob {
                job_id, fields, artifact_ids, expected_revision, fencing_token, at,
            } => self.apply_advance_job(
                owner_id, actor, job_id, fields, artifact_ids, expected_revision, fencing_token, at,
            )?,
            EngineCommand::ApproveJob { job_id, note, expected_revision, at } => {
                self.apply_approve_job(owner_id, actor, job_id, note, expected_revision, at)?
            }
            EngineCommand::ReworkJob {
                job_id, expected_revision, fencing_token, reason, at,
            } => self.apply_rework_job(owner_id, actor, job_id, expected_revision, fencing_token, reason, at)?,
            EngineCommand::StopJob { job_id, reason, expected_revision, at } => {
                self.apply_stop_job(owner_id, actor, job_id, reason, expected_revision, at)?
            }
            EngineCommand::ResumeJob { job_id, expected_revision, at } => {
                self.apply_resume_job(owner_id, actor, job_id, expected_revision, at)?
            }
            EngineCommand::AttachArtifact {
                job_id, kind, label, uri, sha256, fencing_token, at,
            } => self.apply_attach_artifact(owner_id, actor, job_id, kind, label, uri, sha256, fencing_token, at)?,
            EngineCommand::RecordGateVerification {
                job_id, verdict, summary, checklist_results, fencing_token, at,
            } => self.apply_record_gate_verification(
                owner_id, actor, job_id, verdict, summary, checklist_results, fencing_token, at,
            )?,
            EngineCommand::SubmitFeedback { district_id, job_id, stage_id, kind, body, at } => {
                self.apply_submit_feedback(owner_id, actor, district_id, job_id, stage_id, kind, body, at)?
            }
            EngineCommand::ProposePhilosophyUpdate {
                district_id, base_version, proposed_district_name, summary, principles,
                anti_patterns, decision_heuristics, rationale, source_feedback_ids, at,
            } => self.apply_propose_philosophy_update(
                owner_id, actor, district_id, base_version, proposed_district_name, summary,
                principles, anti_patterns, decision_heuristics, rationale, source_feedback_ids, at,
            )?,
            EngineCommand::ApprovePhilosophyProposal { proposal_id, at } => {
                self.apply_approve_philosophy_proposal(owner_id, actor, proposal_id, at)?
            }
            EngineCommand::ProposeSpecUpdate {
                district_id, base_version, proposed_district_name, stages, changelog,
                rationale, source_feedback_ids, at,
            } => self.apply_propose_spec_update(
                owner_id, actor, district_id, base_version, proposed_district_name, stages,
                changelog, rationale, source_feedback_ids, at,
            )?,
            EngineCommand::ApproveSpecProposal { proposal_id, at } => {
                self.apply_approve_spec_proposal(owner_id, actor, proposal_id, at)?
            }
            EngineCommand::SetStageSubTemplate { district_id, stage_id, template_id, at } => {
                self.apply_set_stage_sub_template(owner_id, actor, district_id, stage_id, template_id, at)?
            }
            EngineCommand::ProposeTemplateUpdate {
                template_id, name, description, philosophy_notes, stages, rationale,
                source_feedback_ids, at,
            } => self.apply_propose_template_update(
                owner_id, actor, template_id, name, description, philosophy_notes, stages,
                rationale, source_feedback_ids, at,
            )?,
            EngineCommand::ApproveTemplateProposal { proposal_id, at } => {
                self.apply_approve_template_proposal(owner_id, actor, proposal_id, at)?
            }
            EngineCommand::SyncBuiltinTemplates { template_ids, at } => {
                self.apply_sync_builtin_templates(owner_id, actor, template_ids, at)?
            }
            EngineCommand::ProposeDistrictCreate {
                district_id, name, setup_by, summary, principles, anti_patterns,
                decision_heuristics, stages, changelog, rationale, source_feedback_ids, at,
            } => self.apply_propose_district_create(
                owner_id, actor, district_id, name, setup_by, summary, principles, anti_patterns,
                decision_heuristics, stages, changelog, rationale, source_feedback_ids, at,
            )?,
            EngineCommand::ProposeDistrictArchive { district_id, rationale, source_feedback_ids, at } => {
                self.apply_propose_district_archive(owner_id, actor, district_id, rationale, source_feedback_ids, at)?
            }
            EngineCommand::ProposeDistrictRestore { district_id, rationale, source_feedback_ids, at } => {
                self.apply_propose_district_restore(owner_id, actor, district_id, rationale, source_feedback_ids, at)?
            }
            EngineCommand::ApproveDistrictProposal { proposal_id, at } => {
                self.apply_approve_district_proposal(owner_id, actor, proposal_id, at)?
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

    pub fn approve_job(
        &mut self,
        job_id: &JobId,
        note: Option<String>,
    ) -> Result<(), TransitionError> {
        let actor = self.local_actor();
        self.apply(
            &actor,
            EngineCommand::ApproveJob {
                job_id: job_id.clone(),
                note,
                expected_revision: None,
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
}
