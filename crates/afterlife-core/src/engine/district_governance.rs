use crate::{
    engine::{AfterlifeEngine, validate::validate_stages, validation::validate_principles},
    error::TransitionError,
    ids::{
        DistrictId, FeedbackId, JobId, OwnerId, PhilosophyVersion, ProposalId, SpecVersion,
        TimestampMs,
    },
    model::{
        Actor, District, DistrictProposal, DistrictProposalAction, JobEvent, JobLocation,
        Philosophy, Principle, ProcedureSpec, ProposalStatus, StageDef, StageKind,
    },
};

fn validate_district_id_format(id: &DistrictId) -> Result<DistrictId, TransitionError> {
    let trimmed = id.as_str().trim();
    if trimmed.is_empty()
        || !trimmed
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(TransitionError::InvalidInput {
            field: "district_id".into(),
            reason: "must be non-empty ASCII alphanumeric, '-', or '_'".into(),
        });
    }
    Ok(DistrictId::new(trimmed))
}

impl AfterlifeEngine {
    pub fn pending_district_proposals(&self) -> Vec<&DistrictProposal> {
        self.district_proposals
            .iter()
            .filter(|p| p.status == ProposalStatus::Pending)
            .collect()
    }

    pub fn active_districts(&self) -> Vec<&District> {
        self.districts.iter().filter(|d| !d.archived).collect()
    }

    pub(super) fn active_job_ids_in_district(&self, district_id: &DistrictId) -> Vec<JobId> {
        self.jobs
            .iter()
            .filter(|j| {
                matches!(
                    &j.location,
                    JobLocation::InDistrict { district_id: did, .. } if did == district_id
                )
            })
            .map(|j| j.id.clone())
            .collect()
    }

    fn allocate_district_id(&self) -> DistrictId {
        let mut max_num = 0u32;
        for d in &self.districts {
            if let Some(rest) = d.id.as_str().strip_prefix("DIST-")
                && let Ok(n) = rest.parse::<u32>()
            {
                max_num = max_num.max(n);
            }
        }
        let mut candidate_num = max_num + 1;
        loop {
            let candidate = DistrictId::new(format!("DIST-{candidate_num:03}"));
            if self.find_district(&candidate).is_none() {
                return candidate;
            }
            candidate_num += 1;
        }
    }

    fn validate_proposed_create_stages(
        &self,
        district_id: &DistrictId,
        stages: &[StageDef],
    ) -> Result<(), TransitionError> {
        let mut reasons: Vec<String> = validate_stages(district_id, stages, &self.templates)
            .into_iter()
            .map(|e| e.to_string())
            .collect();
        if !stages.iter().any(|s| s.kind == StageKind::ApprovalGate) {
            reasons.push("district spec must include at least one ApprovalGate stage".into());
        }
        if !stages.iter().any(|s| s.kind == StageKind::Terminal) {
            reasons.push("district spec must include at least one Terminal stage".into());
        }
        if !reasons.is_empty() {
            return Err(TransitionError::InvalidProposedSpec {
                district_id: district_id.clone(),
                reasons,
            });
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_propose_district_create(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        district_id: Option<DistrictId>,
        name: String,
        setup_by: Option<String>,
        summary: String,
        principles: Vec<Principle>,
        anti_patterns: Vec<String>,
        decision_heuristics: Vec<String>,
        stages: Vec<StageDef>,
        changelog: String,
        rationale: String,
        source_feedback_ids: Vec<FeedbackId>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        let name = name.trim().to_string();
        let rationale = rationale.trim().to_string();
        let normalized_id = match district_id {
            Some(raw) => {
                let valid = validate_district_id_format(&raw)?;
                if self.find_district(&valid).is_some() {
                    return Err(TransitionError::DistrictAlreadyExists(valid));
                }
                Some(valid)
            }
            None => None,
        };
        let validation_id = normalized_id
            .clone()
            .unwrap_or_else(|| self.allocate_district_id());
        self.validate_proposed_create_stages(&validation_id, &stages)?;

        let setup_label = setup_by
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| actor.display_label().to_string());
        let changelog_text = if changelog.trim().is_empty() {
            "Initial district procedure specification".to_string()
        } else {
            changelog.trim().to_string()
        };
        let philosophy = Philosophy {
            version: PhilosophyVersion::new(1),
            summary: summary.trim().to_string(),
            principles,
            anti_patterns,
            decision_heuristics,
            source_feedback_ids: source_feedback_ids.clone(),
            updated_by: actor.clone(),
            updated_at: at,
        };
        let spec = ProcedureSpec {
            version: SpecVersion::new(1),
            stages,
            changelog: changelog_text,
            source_feedback_ids: source_feedback_ids.clone(),
            updated_by: actor.clone(),
            updated_at: at,
        };

        let pid = ProposalId::new(format!("PROP-DIST-{:04}", self.next_proposal_seq));
        self.next_proposal_seq += 1;
        self.district_proposals.push(DistrictProposal {
            id: pid.clone(),
            owner_id: owner_id.clone(),
            action: DistrictProposalAction::Create {
                proposed_district_id: normalized_id.clone(),
                name,
                setup_by: setup_label,
                philosophy,
                spec,
            },
            rationale,
            source_feedback_ids,
            proposed_by: actor.clone(),
            status: ProposalStatus::Pending,
            created_at: at,
            resolved_at: None,
        });

        Ok(vec![JobEvent::DistrictProposalCreated {
            owner_id,
            actor: actor.clone(),
            proposal_id: pid,
            action: "create".into(),
            district_id: normalized_id,
            at,
        }])
    }

    pub(super) fn apply_propose_district_archive(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        district_id: DistrictId,
        rationale: String,
        source_feedback_ids: Vec<FeedbackId>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        let rationale = rationale.trim().to_string();
        if rationale.is_empty() {
            return Err(TransitionError::InvalidInput {
                field: "rationale".into(),
                reason: "must not be empty".into(),
            });
        }
        let district = self
            .find_district(&district_id)
            .ok_or_else(|| TransitionError::DistrictNotFound(district_id.clone()))?;
        if district.archived {
            return Err(TransitionError::DistrictArchived(district_id));
        }
        let active_jobs = self.active_job_ids_in_district(&district_id);
        if !active_jobs.is_empty() {
            return Err(TransitionError::DistrictHasActiveJobs {
                district_id,
                active_job_ids: active_jobs,
            });
        }

        let pid = ProposalId::new(format!("PROP-DIST-{:04}", self.next_proposal_seq));
        self.next_proposal_seq += 1;
        self.district_proposals.push(DistrictProposal {
            id: pid.clone(),
            owner_id: owner_id.clone(),
            action: DistrictProposalAction::Archive {
                district_id: district_id.clone(),
            },
            rationale,
            source_feedback_ids,
            proposed_by: actor.clone(),
            status: ProposalStatus::Pending,
            created_at: at,
            resolved_at: None,
        });

        Ok(vec![JobEvent::DistrictProposalCreated {
            owner_id,
            actor: actor.clone(),
            proposal_id: pid,
            action: "archive".into(),
            district_id: Some(district_id),
            at,
        }])
    }

    pub(super) fn apply_propose_district_restore(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        district_id: DistrictId,
        rationale: String,
        source_feedback_ids: Vec<FeedbackId>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        let rationale = rationale.trim().to_string();
        if rationale.is_empty() {
            return Err(TransitionError::InvalidInput {
                field: "rationale".into(),
                reason: "must not be empty".into(),
            });
        }
        let district = self
            .find_district(&district_id)
            .ok_or_else(|| TransitionError::DistrictNotFound(district_id.clone()))?;
        if !district.archived {
            return Err(TransitionError::DistrictNotArchived(district_id));
        }

        let pid = ProposalId::new(format!("PROP-DIST-{:04}", self.next_proposal_seq));
        self.next_proposal_seq += 1;
        self.district_proposals.push(DistrictProposal {
            id: pid.clone(),
            owner_id: owner_id.clone(),
            action: DistrictProposalAction::Restore {
                district_id: district_id.clone(),
            },
            rationale,
            source_feedback_ids,
            proposed_by: actor.clone(),
            status: ProposalStatus::Pending,
            created_at: at,
            resolved_at: None,
        });

        Ok(vec![JobEvent::DistrictProposalCreated {
            owner_id,
            actor: actor.clone(),
            proposal_id: pid,
            action: "restore".into(),
            district_id: Some(district_id),
            at,
        }])
    }

    pub(super) fn apply_approve_district_proposal(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        proposal_id: ProposalId,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        if !actor.is_human() {
            return Err(TransitionError::HumanApprovalRequired {
                action: format!("approve district proposal {proposal_id}"),
            });
        }
        let prop = self
            .district_proposals
            .iter()
            .find(|p| p.id == proposal_id)
            .cloned()
            .ok_or_else(|| TransitionError::ProposalNotFound(proposal_id.clone()))?;
        if prop.status != ProposalStatus::Pending {
            return Err(TransitionError::ProposalAlreadyResolved(proposal_id));
        }

        let event = match prop.action {
            DistrictProposalAction::Create {
                proposed_district_id,
                name,
                setup_by,
                philosophy,
                spec,
            } => {
                if name.trim().is_empty() {
                    return Err(TransitionError::InvalidInput {
                        field: "name".into(),
                        reason: "must not be empty".into(),
                    });
                }
                if philosophy.summary.trim().is_empty() {
                    return Err(TransitionError::InvalidInput {
                        field: "summary".into(),
                        reason: "must not be empty".into(),
                    });
                }
                validate_principles(&philosophy.principles)?;
                let final_id = match proposed_district_id {
                    Some(raw) => {
                        let valid = validate_district_id_format(&raw)?;
                        if self.find_district(&valid).is_some() {
                            return Err(TransitionError::DistrictAlreadyExists(valid));
                        }
                        valid
                    }
                    None => self.allocate_district_id(),
                };
                self.validate_proposed_create_stages(&final_id, &spec.stages)?;
                self.districts.push(District {
                    id: final_id.clone(),
                    owner_id: owner_id.clone(),
                    name: name.clone(),
                    setup_by,
                    active_philosophy_version: philosophy.version,
                    philosophy_history: vec![philosophy],
                    active_spec_version: spec.version,
                    spec_history: vec![spec],
                    archived: false,
                    archived_at: None,
                });
                JobEvent::DistrictCreated {
                    owner_id,
                    actor: actor.clone(),
                    proposal_id: proposal_id.clone(),
                    district_id: final_id,
                    name,
                    at,
                }
            }
            DistrictProposalAction::Archive { district_id } => {
                let active_jobs = self.active_job_ids_in_district(&district_id);
                if !active_jobs.is_empty() {
                    return Err(TransitionError::DistrictHasActiveJobs {
                        district_id,
                        active_job_ids: active_jobs,
                    });
                }
                let district = self
                    .districts
                    .iter_mut()
                    .find(|d| d.id == district_id)
                    .ok_or_else(|| TransitionError::DistrictNotFound(district_id.clone()))?;
                if district.archived {
                    return Err(TransitionError::DistrictArchived(district_id));
                }
                district.archived = true;
                district.archived_at = Some(at);
                JobEvent::DistrictArchived {
                    owner_id,
                    actor: actor.clone(),
                    proposal_id: proposal_id.clone(),
                    district_id,
                    at,
                }
            }
            DistrictProposalAction::Restore { district_id } => {
                let district = self
                    .districts
                    .iter_mut()
                    .find(|d| d.id == district_id)
                    .ok_or_else(|| TransitionError::DistrictNotFound(district_id.clone()))?;
                if !district.archived {
                    return Err(TransitionError::DistrictNotArchived(district_id));
                }
                district.archived = false;
                district.archived_at = None;
                JobEvent::DistrictRestored {
                    owner_id,
                    actor: actor.clone(),
                    proposal_id: proposal_id.clone(),
                    district_id,
                    at,
                }
            }
        };

        let p_mut = self
            .district_proposals
            .iter_mut()
            .find(|p| p.id == proposal_id)
            .unwrap();
        p_mut.status = ProposalStatus::Approved;
        p_mut.resolved_at = Some(at);

        Ok(vec![event])
    }
}
