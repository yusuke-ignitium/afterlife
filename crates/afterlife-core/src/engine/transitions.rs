use std::collections::BTreeMap;

use crate::{
    engine::{AfterlifeEngine, policy},
    error::TransitionError,
    ids::{ArtifactId, FeedbackId, FencingToken, JobId, OwnerId, TimestampMs},
    model::{
        Actor, Feedback, FeedbackKind, JobEvent, JobLocation, StepRecord, StepRecordOutcome,
    },
};

impl AfterlifeEngine {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_advance_job(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        job_id: JobId,
        fields: BTreeMap<String, String>,
        artifact_ids: Vec<ArtifactId>,
        expected_revision: Option<u64>,
        fencing_token: Option<FencingToken>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
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
        policy::require_agent_execution(actor, "advance job")?;
        policy::require_mutation_lease(actor, &job_snap, fencing_token, at)?;

        let current_sub = match (&stage.sub_template_id, &sub_stage_id) {
            (Some(tid), Some(cur_sub_id)) => self
                .templates
                .get(tid)
                .and_then(|tmpl| tmpl.stages.iter().find(|s| &s.id == cur_sub_id))
                .cloned(),
            _ => None,
        };

        policy::require_step_record(
            &job_snap,
            &district_id,
            &stage,
            current_sub.as_ref(),
            &fields,
            &artifact_ids,
        )?;

        if let (Some(tid), Some(cur_sub_id)) = (&stage.sub_template_id, &sub_stage_id) {
            if let Some(tmpl) = self.templates.get(tid) {
                if let Some(pos) = tmpl.stages.iter().position(|s| &s.id == cur_sub_id) {
                    if let Some(next_sub) = tmpl.stages.get(pos + 1).cloned() {
                        let to = JobLocation::InDistrict {
                            district_id: district_id.clone(),
                            stage_id: stage_id.clone(),
                            sub_stage_id: Some(next_sub.id),
                        };
                        let target = self.jobs.iter_mut().find(|j| j.id == job_id).unwrap();
                        let from = target.location.clone();
                        target.location = to.clone();
                        target.lease = None;
                        target.revision += 1;
                        target.updated_at = at;
                        target.step_records.push(StepRecord {
                            district_id: district_id.clone(),
                            stage_id: stage_id.clone(),
                            sub_stage_id: sub_stage_id.clone(),
                            revision: target.revision,
                            actor: actor.clone(),
                            outcome: StepRecordOutcome::Advanced,
                            fields,
                            artifact_ids,
                            recorded_at: at,
                        });
                        let ev = vec![
                            JobEvent::StepRecorded {
                                owner_id: owner_id.clone(),
                                actor: actor.clone(),
                                job_id: job_id.clone(),
                                district_id,
                                stage_id,
                                sub_stage_id,
                                revision: target.revision,
                                outcome: StepRecordOutcome::Advanced,
                                at,
                            },
                            JobEvent::JobTransitioned {
                                owner_id,
                                actor: actor.clone(),
                                job_id,
                                revision: target.revision,
                                from,
                                to,
                                claimed_by: None,
                                fencing_token: None,
                                at,
                            },
                        ];
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
                district_id: district_id.clone(),
                terminal_stage_id: next_stage.id,
                completed_at: at,
            }
        } else {
            JobLocation::InDistrict {
                district_id: district_id.clone(),
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
        target.step_records.push(StepRecord {
            district_id: district_id.clone(),
            stage_id: stage_id.clone(),
            sub_stage_id: sub_stage_id.clone(),
            revision: target.revision,
            actor: actor.clone(),
            outcome: StepRecordOutcome::Advanced,
            fields,
            artifact_ids,
            recorded_at: at,
        });

        Ok(vec![
            JobEvent::StepRecorded {
                owner_id: owner_id.clone(),
                actor: actor.clone(),
                job_id: job_id.clone(),
                district_id,
                stage_id,
                sub_stage_id,
                revision: target.revision,
                outcome: StepRecordOutcome::Advanced,
                at,
            },
            JobEvent::JobTransitioned {
                owner_id,
                actor: actor.clone(),
                job_id,
                revision: target.revision,
                from,
                to,
                claimed_by: None,
                fencing_token: None,
                at,
            },
        ])
    }

    pub(super) fn apply_approve_job(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        job_id: JobId,
        note: Option<String>,
        expected_revision: Option<u64>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        if !actor.is_human() {
            return Err(TransitionError::HumanApprovalRequired {
                action: format!("approve job {job_id}"),
            });
        }

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

        if !policy::is_human_only_stage(&stage) {
            return Err(TransitionError::InvalidLocation {
                job_id,
                reason: format!(
                    "stage {district_id}/{} is not a human approval gate",
                    stage.id
                ),
            });
        }

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
                district_id: district_id.clone(),
                terminal_stage_id: next_stage.id,
                completed_at: at,
            }
        } else {
            JobLocation::InDistrict {
                district_id: district_id.clone(),
                stage_id: next_stage.id,
                sub_stage_id: next_sub_id,
            }
        };

        let mut fields = BTreeMap::new();
        let mut emitted = Vec::new();
        let fb_id = if let Some(ref body) = note {
            fields.insert("note".to_string(), body.clone());
            let fid = FeedbackId::new(format!("FB-{:04}", self.next_feedback_seq));
            self.next_feedback_seq += 1;
            self.feedbacks.push(Feedback {
                id: fid.clone(),
                owner_id: owner_id.clone(),
                district_id: district_id.clone(),
                job_id: Some(job_id.clone()),
                stage_id: Some(stage_id.clone()),
                kind: FeedbackKind::ApprovalNote,
                body: body.clone(),
                author: actor.clone(),
                created_at: at,
            });
            emitted.push(JobEvent::FeedbackSubmitted {
                owner_id: owner_id.clone(),
                actor: actor.clone(),
                feedback_id: fid.clone(),
                district_id: district_id.clone(),
                job_id: Some(job_id.clone()),
                stage_id: Some(stage_id.clone()),
                kind: FeedbackKind::ApprovalNote,
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
        target.stopped = false;
        target.stopped_at = None;
        target.stopped_reason = None;
        if let Some(fid) = fb_id {
            target.feedback_ids.push(fid);
        }
        target.revision += 1;
        target.updated_at = at;
        target.step_records.push(StepRecord {
            district_id: district_id.clone(),
            stage_id: stage_id.clone(),
            sub_stage_id: sub_stage_id.clone(),
            revision: target.revision,
            actor: actor.clone(),
            outcome: StepRecordOutcome::Approved,
            fields,
            artifact_ids: Vec::new(),
            recorded_at: at,
        });

        emitted.insert(
            0,
            JobEvent::StepRecorded {
                owner_id: owner_id.clone(),
                actor: actor.clone(),
                job_id: job_id.clone(),
                district_id,
                stage_id,
                sub_stage_id,
                revision: target.revision,
                outcome: StepRecordOutcome::Approved,
                at,
            },
        );
        emitted.push(JobEvent::JobTransitioned {
            owner_id,
            actor: actor.clone(),
            job_id,
            revision: target.revision,
            from,
            to,
            claimed_by: None,
            fencing_token: None,
            at,
        });
        Ok(emitted)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_rework_job(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        job_id: JobId,
        expected_revision: Option<u64>,
        fencing_token: Option<FencingToken>,
        reason: Option<String>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
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

        let mut fields = BTreeMap::new();
        let mut emitted = Vec::new();
        let fb_id = if let Some(ref body) = reason {
            fields.insert("reason".to_string(), body.clone());
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
                district_id: district_id.clone(),
                job_id: Some(job_id.clone()),
                stage_id: Some(stage_id.clone()),
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
        target.stopped = false;
        target.stopped_at = None;
        target.stopped_reason = None;
        if let Some(fid) = fb_id {
            target.feedback_ids.push(fid);
        }
        target.revision += 1;
        target.updated_at = at;
        target.step_records.push(StepRecord {
            district_id: district_id.clone(),
            stage_id: stage_id.clone(),
            sub_stage_id: sub_stage_id.clone(),
            revision: target.revision,
            actor: actor.clone(),
            outcome: StepRecordOutcome::Reworked,
            fields,
            artifact_ids: Vec::new(),
            recorded_at: at,
        });

        emitted.insert(
            0,
            JobEvent::StepRecorded {
                owner_id: owner_id.clone(),
                actor: actor.clone(),
                job_id: job_id.clone(),
                district_id,
                stage_id,
                sub_stage_id,
                revision: target.revision,
                outcome: StepRecordOutcome::Reworked,
                at,
            },
        );
        emitted.push(JobEvent::JobReworked {
            owner_id,
            actor: actor.clone(),
            job_id,
            revision: target.revision,
            from,
            to,
            reason,
            at,
        });
        Ok(emitted)
    }
}
