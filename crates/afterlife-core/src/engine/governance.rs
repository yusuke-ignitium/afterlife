use crate::{
    engine::{
        AfterlifeEngine,
        validate::{validate_stages, validate_template},
    },
    error::TransitionError,
    fixtures,
    ids::{
        DistrictId, FeedbackId, OwnerId, PhilosophyVersion, ProposalId, SpecVersion,
        SubMachineTemplateId, TimestampMs,
    },
    model::{
        Actor, JobEvent, Philosophy, PhilosophyProposal, Principle, ProcedureSpec, ProposalStatus,
        SpecProposal, StageDef, SubMachineTemplate, SubStageDef, TemplateProposal,
    },
};

const LEGACY_JAPANESE_BUILTIN_DESCRIPTIONS: &[(&str, &str)] = &[
    (
        "tmpl-evidence",
        "一次仕様・既存実装トレースから隔離環境での実機検証、判断基準と前提の確定まで",
    ),
    (
        "tmpl-observe",
        "対象経路の棚卸しから未観測区間の計測計装、再現可能な評価指標仕様の確定まで",
    ),
    (
        "tmpl-baseline-guard",
        "メイン変更に着手する前のBeforeベースライン採取とデグレガード先行固定",
    ),
    (
        "tmpl-red-repro",
        "修正前に障害を確実に再現するRedテスト固定と周辺退行ガード確認",
    ),
    (
        "tmpl-build-loop",
        "先行ガード下での土台整合変更、境界・異常系テスト補強、警告ゼロ検証ループ",
    ),
    (
        "tmpl-perf-loop",
        "実測プロファイル駆動のボトルネック特定、原則的改善、同一条件再計測ループ",
    ),
    (
        "tmpl-verify-ship",
        "実機・本番相当環境検証、Before/After前後比較、証跡とWhyの整理",
    ),
];

fn has_identical_builtin_structure(
    current: &SubMachineTemplate,
    seed: &SubMachineTemplate,
) -> bool {
    if current.id != seed.id
        || current.name != seed.name
        || current.stages.len() != seed.stages.len()
    {
        return false;
    }
    current.stages.iter().zip(seed.stages.iter()).all(|(a, b)| {
        a.id == b.id
            && a.label == b.label
            && a.runner == b.runner
            && a.runner_kind == b.runner_kind
            && a.required_artifact_kinds == b.required_artifact_kinds
            && a.record_fields.len() == b.record_fields.len()
            && a.record_fields
                .iter()
                .zip(b.record_fields.iter())
                .all(|(fa, fb)| fa.key == fb.key && fa.required == fb.required)
    })
}

fn is_unmodified_legacy_builtin(
    current: &SubMachineTemplate,
    seed: &SubMachineTemplate,
) -> bool {
    if current == seed {
        return false;
    }
    if !has_identical_builtin_structure(current, seed) {
        return false;
    }
    LEGACY_JAPANESE_BUILTIN_DESCRIPTIONS
        .iter()
        .any(|(id, desc)| *id == current.id.as_str() && *desc == current.description.as_str())
}

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

    pub fn pending_template_proposals(&self) -> Vec<&TemplateProposal> {
        self.template_proposals
            .iter()
            .filter(|p| p.status == ProposalStatus::Pending)
            .collect()
    }

    pub fn outdated_builtin_template_ids(&self) -> Vec<SubMachineTemplateId> {
        let seed = fixtures::default_workspace(&self.owner_id);
        let mut outdated = Vec::new();
        for tid in &seed.template_order {
            let Some(seed_tmpl) = seed.templates.get(tid) else {
                continue;
            };
            let was_updated_by_proposal = self.event_log.iter().any(|ev| {
                matches!(ev, JobEvent::TemplateUpdated { template_id, .. } if template_id == tid)
            });
            if was_updated_by_proposal {
                continue;
            }
            match self.templates.get(tid) {
                None => outdated.push(tid.clone()),
                Some(current) if is_unmodified_legacy_builtin(current, seed_tmpl) => {
                    outdated.push(tid.clone());
                }
                Some(_) => {}
            }
        }
        outdated
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
        if district.archived {
            return Err(TransitionError::DistrictArchived(district_id));
        }
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
        if district.archived {
            return Err(TransitionError::DistrictArchived(prop.district_id));
        }
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
        if district.archived {
            return Err(TransitionError::DistrictArchived(district_id));
        }
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
        if district.archived {
            return Err(TransitionError::DistrictArchived(prop.district_id));
        }
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

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_propose_template_update(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        template_id: SubMachineTemplateId,
        name: String,
        description: String,
        philosophy_notes: String,
        stages: Vec<SubStageDef>,
        rationale: String,
        source_feedback_ids: Vec<FeedbackId>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        let normalized_id = SubMachineTemplateId::new(template_id.as_str().trim());
        let proposed = SubMachineTemplate {
            id: normalized_id.clone(),
            name: name.trim().to_string(),
            description: description.trim().to_string(),
            philosophy_notes: philosophy_notes.trim().to_string(),
            stages,
        };
        let errs = validate_template(&proposed);
        if !errs.is_empty() {
            return Err(TransitionError::InvalidProposedTemplate {
                template_id: normalized_id,
                reasons: errs.into_iter().map(|e| e.to_string()).collect(),
            });
        }
        let pid = ProposalId::new(format!("PROP-TMPL-{:04}", self.next_proposal_seq));
        self.next_proposal_seq += 1;
        self.template_proposals.push(TemplateProposal {
            id: pid.clone(),
            owner_id: owner_id.clone(),
            template_id: normalized_id.clone(),
            proposed,
            rationale: rationale.trim().to_string(),
            source_feedback_ids,
            proposed_by: actor.clone(),
            status: ProposalStatus::Pending,
            created_at: at,
            resolved_at: None,
        });
        Ok(vec![JobEvent::TemplateProposed {
            owner_id,
            actor: actor.clone(),
            proposal_id: pid,
            template_id: normalized_id,
            at,
        }])
    }

    pub(super) fn apply_approve_template_proposal(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        proposal_id: ProposalId,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        if !actor.is_human() {
            return Err(TransitionError::HumanApprovalRequired {
                action: format!("approve template proposal {proposal_id}"),
            });
        }
        let prop = self
            .template_proposals
            .iter()
            .find(|p| p.id == proposal_id)
            .cloned()
            .ok_or_else(|| TransitionError::ProposalNotFound(proposal_id.clone()))?;
        if prop.status != ProposalStatus::Pending {
            return Err(TransitionError::ProposalAlreadyResolved(proposal_id));
        }
        let errs = validate_template(&prop.proposed);
        if !errs.is_empty() {
            return Err(TransitionError::InvalidProposedTemplate {
                template_id: prop.template_id,
                reasons: errs.into_iter().map(|e| e.to_string()).collect(),
            });
        }
        if !self.template_order.contains(&prop.template_id) {
            self.template_order.push(prop.template_id.clone());
        }
        self.templates
            .insert(prop.template_id.clone(), prop.proposed);

        let p_mut = self
            .template_proposals
            .iter_mut()
            .find(|p| p.id == proposal_id)
            .unwrap();
        p_mut.status = ProposalStatus::Approved;
        p_mut.resolved_at = Some(at);

        Ok(vec![JobEvent::TemplateUpdated {
            owner_id,
            actor: actor.clone(),
            proposal_id,
            template_id: prop.template_id,
            at,
        }])
    }

    pub(super) fn apply_sync_builtin_templates(
        &mut self,
        owner_id: OwnerId,
        actor: &Actor,
        template_ids: Option<Vec<SubMachineTemplateId>>,
        at: TimestampMs,
    ) -> Result<Vec<JobEvent>, TransitionError> {
        if !actor.is_human() {
            return Err(TransitionError::HumanApprovalRequired {
                action: "sync built-in sub-machine templates".into(),
            });
        }
        let seed = fixtures::default_workspace(&self.owner_id);
        let mut synced = Vec::new();
        let mut skipped = Vec::new();

        if let Some(requested_ids) = template_ids {
            for tid in &requested_ids {
                if !seed.templates.contains_key(tid) {
                    return Err(TransitionError::TemplateNotFound(tid.clone()));
                }
            }
            for tid in requested_ids {
                let Some(seed_tmpl) = seed.templates.get(&tid) else {
                    continue;
                };
                if self.templates.get(&tid) == Some(seed_tmpl) {
                    skipped.push(tid);
                    continue;
                }
                if !self.template_order.contains(&tid) {
                    self.template_order.push(tid.clone());
                }
                self.templates.insert(tid.clone(), seed_tmpl.clone());
                synced.push(tid);
            }
        } else {
            let outdated = self.outdated_builtin_template_ids();
            for tid in &seed.template_order {
                if outdated.contains(tid) {
                    if let Some(seed_tmpl) = seed.templates.get(tid) {
                        if !self.template_order.contains(tid) {
                            self.template_order.push(tid.clone());
                        }
                        self.templates.insert(tid.clone(), seed_tmpl.clone());
                        synced.push(tid.clone());
                    }
                } else {
                    skipped.push(tid.clone());
                }
            }
        }

        Ok(vec![JobEvent::BuiltinTemplatesSynced {
            owner_id,
            actor: actor.clone(),
            synced_template_ids: synced,
            skipped_template_ids: skipped,
            at,
        }])
    }
}
