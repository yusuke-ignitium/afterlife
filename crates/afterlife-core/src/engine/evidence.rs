use crate::{
    engine::{AfterlifeEngine, policy},
    error::TransitionError,
    ids::{
        ArtifactId, DistrictId, FeedbackId, FencingToken, JobId, OwnerId, StageId,
        SubMachineTemplateId, TimestampMs,
    },
    model::{
        Actor, Artifact, ArtifactKind, Feedback, FeedbackKind, GateVerdict, GateVerification,
        JobEvent, JobLocation,
    },
};

impl AfterlifeEngine {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_attach_artifact(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        job_id: JobId,
        kind: ArtifactKind,
        label: String,
        uri: String,
        sha256: Option<String>,
        fencing_token: Option<FencingToken>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        let job_snap = self
            .find_job(&job_id)
            .cloned()
            .ok_or_else(|| TransitionError::JobNotFound(job_id.clone()))?;
        if let Some((cur_dist, cur_stage)) = self.current_stage_for_job(&job_snap) {
            policy::require_stage_authority(actor, cur_dist, cur_stage, "attach artifact")?;
        }
        policy::require_mutation_lease(actor, &job_snap, fencing_token, at)?;

        let (produced_at_stage, produced_at_sub_stage) = match &job_snap.location {
            JobLocation::InDistrict {
                stage_id,
                sub_stage_id,
                ..
            } => (Some(stage_id.clone()), sub_stage_id.clone()),
            _ => (None, None),
        };
        let uri = uri.trim().to_string();
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
            produced_at_sub_stage,
            revision_at_capture: target.revision,
        });

        Ok(vec![JobEvent::ArtifactAttached {
            owner_id,
            actor: actor.clone(),
            job_id,
            revision: target.revision,
            artifact_id: art_id,
            kind,
            label,
            uri,
            at,
        }])
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_record_gate_verification(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        job_id: JobId,
        verdict: GateVerdict,
        summary: String,
        checklist_results: Vec<(String, bool)>,
        fencing_token: Option<FencingToken>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
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

        Ok(vec![JobEvent::GateVerificationRecorded {
            owner_id,
            actor: actor.clone(),
            job_id,
            stage_id,
            verdict,
            summary,
            at,
        }])
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_submit_feedback(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        district_id: DistrictId,
        job_id: Option<JobId>,
        stage_id: Option<StageId>,
        kind: FeedbackKind,
        body: String,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
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
        Ok(vec![JobEvent::FeedbackSubmitted {
            owner_id,
            actor: actor.clone(),
            feedback_id: fid,
            district_id,
            job_id,
            stage_id,
            kind,
            at,
        }])
    }

    pub(super) fn apply_set_stage_sub_template(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        district_id: DistrictId,
        stage_id: StageId,
        template_id: Option<SubMachineTemplateId>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
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
        if district.archived {
            return Err(TransitionError::DistrictArchived(district_id));
        }
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

        Ok(vec![JobEvent::StageSubTemplateChanged {
            owner_id,
            actor: actor.clone(),
            district_id,
            stage_id,
            template_id,
            at,
        }])
    }
}
