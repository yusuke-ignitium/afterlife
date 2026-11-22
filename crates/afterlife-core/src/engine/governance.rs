use crate::{
    engine::{AfterlifeEngine, validate::validate_stages},
    error::TransitionError,
    ids::{
        DistrictId, FeedbackId, OwnerId, PhilosophyVersion, ProposalId, SpecVersion, TimestampMs,
    },
    model::{
        Actor, JobEvent, Philosophy, PhilosophyProposal, Principle, ProcedureSpec, ProposalStatus,
        SpecProposal, StageDef,
    },
};

impl AfterlifeEngine {
    pub fn pending_philosophy_proposals(&self) -> Vec<&PhilosophyProposal> {
        self.philosophy_proposals
            .iter()
            .filter(|p| p.status == ProposalStatus::Pending)
            .collect()
    }

    pub fn pending_spec_proposals(&self) -> Vec<&SpecProposal> {
        self.spec_proposals
            .iter()
            .filter(|p| p.status == ProposalStatus::Pending)
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_propose_philosophy_update(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        district_id: DistrictId,
        base_version: PhilosophyVersion,
        proposed_district_name: Option<String>,
        summary: String,
        principles: Vec<Principle>,
        anti_patterns: Vec<String>,
        decision_heuristics: Vec<String>,
        rationale: String,
        source_feedback_ids: Vec<FeedbackId>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
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
        let normalized_name = proposed_district_name.map(|n| n.trim().to_string());
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
            proposed_district_name: normalized_name,
            rationale,
            source_feedback_ids,
            proposed_by: actor.clone(),
            status: ProposalStatus::Pending,
            created_at: at,
            resolved_at: None,
        });
        Ok(vec![JobEvent::PhilosophyProposed {
            owner_id,
            actor: actor.clone(),
            proposal_id: pid,
            district_id,
            base_version,
            proposed_version: next_ver,
            at,
        }])
    }

    pub(super) fn apply_approve_philosophy_proposal(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        proposal_id: ProposalId,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
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
        if let Some(new_name) = prop.proposed_district_name {
            district.name = new_name;
        }

        let p_mut = self
            .philosophy_proposals
            .iter_mut()
            .find(|p| p.id == proposal_id)
            .unwrap();
        p_mut.status = ProposalStatus::Approved;
        p_mut.resolved_at = Some(at);

        Ok(vec![JobEvent::PhilosophyUpdated {
            owner_id,
            actor: actor.clone(),
            proposal_id,
            district_id: prop.district_id,
            new_version,
            at,
        }])
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_propose_spec_update(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        district_id: DistrictId,
        base_version: SpecVersion,
        proposed_district_name: Option<String>,
        stages: Vec<StageDef>,
        changelog: String,
        rationale: String,
        source_feedback_ids: Vec<FeedbackId>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
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
        let normalized_name = proposed_district_name.map(|n| n.trim().to_string());
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
            proposed_district_name: normalized_name,
            rationale,
            source_feedback_ids,
            proposed_by: actor.clone(),
            status: ProposalStatus::Pending,
            created_at: at,
            resolved_at: None,
        });
        Ok(vec![JobEvent::SpecProposed {
            owner_id,
            actor: actor.clone(),
            proposal_id: pid,
            district_id,
            base_version,
            proposed_version: next_ver,
            at,
        }])
    }

    pub(super) fn apply_approve_spec_proposal(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        proposal_id: ProposalId,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
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
        if let Some(new_name) = prop.proposed_district_name {
            district.name = new_name;
        }

        let p_mut = self
            .spec_proposals
            .iter_mut()
            .find(|p| p.id == proposal_id)
            .unwrap();
        p_mut.status = ProposalStatus::Approved;
        p_mut.resolved_at = Some(at);

        Ok(vec![JobEvent::SpecUpdated {
            owner_id,
            actor: actor.clone(),
            proposal_id,
            district_id: prop.district_id,
            new_version,
            at,
        }])
    }
}
