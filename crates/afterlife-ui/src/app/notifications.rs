use std::collections::BTreeMap;

use afterlife_core::{AfterlifeEngine, DistrictId, JobId, JobLocation, StageId};

use crate::app::state::{AppState, TabId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalGateEntry {
    pub district_id: DistrictId,
    pub stage_id: StageId,
    pub step_record_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalNotificationTarget {
    pub job_id: JobId,
    pub job_title: String,
    pub district_name: String,
    pub stage_id: StageId,
    pub entry_key: String,
}

pub fn snapshot_approval_gates(engine: &AfterlifeEngine) -> BTreeMap<JobId, ApprovalGateEntry> {
    let mut map = BTreeMap::new();
    for job in engine.approval_jobs() {
        if let JobLocation::InDistrict {
            district_id,
            stage_id,
            ..
        } = &job.location
        {
            map.insert(
                job.id.clone(),
                ApprovalGateEntry {
                    district_id: district_id.clone(),
                    stage_id: stage_id.clone(),
                    step_record_count: job.step_records.len(),
                },
            );
        }
    }
    map
}

pub fn diff_newly_entered_approval_gates(
    prev: &BTreeMap<JobId, ApprovalGateEntry>,
    next: &AfterlifeEngine,
) -> Vec<ApprovalNotificationTarget> {
    let mut arrived = Vec::new();
    for job in next.approval_jobs() {
        let JobLocation::InDistrict {
            district_id,
            stage_id,
            ..
        } = &job.location
        else {
            continue;
        };
        let entry = ApprovalGateEntry {
            district_id: district_id.clone(),
            stage_id: stage_id.clone(),
            step_record_count: job.step_records.len(),
        };
        if prev.get(&job.id) == Some(&entry) {
            continue;
        }
        let district_name = next
            .find_district(district_id)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| district_id.to_string());
        arrived.push(ApprovalNotificationTarget {
            job_id: job.id.clone(),
            job_title: job.title.clone(),
            district_name,
            stage_id: stage_id.clone(),
            entry_key: format!("{}-{}-{}", district_id, stage_id, entry.step_record_count),
        });
    }
    arrived
}

impl AppState {
    pub fn open_notification_target(&mut self, job_id: &JobId) -> bool {
        let Some(job) = self.engine.find_job(job_id) else {
            return false;
        };
        if matches!(
            job.location,
            JobLocation::Completed { .. } | JobLocation::Archived
        ) {
            return false;
        }
        self.select_tab(TabId::Workspace);
        self.open_job_detail(job_id.clone());
        true
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use afterlife_core::{
        Actor, ArtifactKind, DistrictId, EngineCommand, GateVerdict, JobId, RunnerId,
    };

    use super::*;

    fn agent(engine: &AfterlifeEngine) -> Actor {
        Actor::Agent {
            owner_id: engine.owner_id.clone(),
            client_id: "notif-agent".into(),
            runner_label: "Notif Agent".into(),
            token_jti: "jti-notif".into(),
        }
    }

    fn advance_job_to_approval_gate(engine: &mut AfterlifeEngine, job_id: &JobId, t: &mut u64) {
        let agent = agent(engine);
        for _ in 0..32 {
            let job = engine.find_job(job_id).unwrap().clone();
            if engine.is_job_at_approval_gate(&job) {
                return;
            }
            let ctx = engine.compile_execution_context(job_id).unwrap();
            *t += 100;
            let claim_ev = engine
                .apply(
                    &agent,
                    EngineCommand::ClaimJob {
                        job_id: job_id.clone(),
                        runner_id: RunnerId::from("notif-agent"),
                        runner_label: "Notif Agent".into(),
                        ttl_ms: 60_000,
                        expected_revision: None,
                        at: *t,
                    },
                )
                .unwrap();
            let ft = match &claim_ev[0] {
                afterlife_core::JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
                _ => unreachable!(),
            };
            let mut art_ids = Vec::new();
            if let Some(kind) = ctx.required_artifact_kinds.first().copied() {
                *t += 10;
                let att = engine
                    .apply(
                        &agent,
                        EngineCommand::AttachArtifact {
                            job_id: job_id.clone(),
                            kind,
                            label: format!("art-{t}.json"),
                            uri: "https://example.com/evidence".into(),
                            sha256: None,
                            fencing_token: Some(ft),
                            at: *t,
                        },
                    )
                    .unwrap();
                if let afterlife_core::JobEvent::ArtifactAttached { artifact_id, .. } = &att[0] {
                    art_ids.push(artifact_id.clone());
                }
            }
            let is_last_sub = match (&ctx.stage.sub_template_id, &ctx.sub_stage) {
                (Some(tid), Some(sub)) => engine
                    .templates
                    .get(tid)
                    .and_then(|tmpl| tmpl.stages.last())
                    .is_some_and(|last| last.id == sub.id),
                _ => true,
            };
            if is_last_sub && let Some(rubric) = &ctx.gate_rubric {
                *t += 10;
                let checks = rubric
                    .checks
                    .iter()
                    .map(|c| (c.clone(), true))
                    .collect::<Vec<_>>();
                engine
                    .apply(
                        &agent,
                        EngineCommand::RecordGateVerification {
                            job_id: job_id.clone(),
                            verdict: GateVerdict::Pass,
                            summary: "Passed".into(),
                            checklist_results: checks,
                            fencing_token: Some(ft),
                            at: *t,
                        },
                    )
                    .unwrap();
            }
            let mut fields = BTreeMap::new();
            for def in &ctx.required_step_fields {
                fields.insert(def.key.clone(), "verified".into());
            }
            *t += 10;
            engine
                .apply(
                    &agent,
                    EngineCommand::AdvanceJob {
                        job_id: job_id.clone(),
                        fields,
                        artifact_ids: art_ids,
                        expected_revision: None,
                        fencing_token: Some(ft),
                        at: *t,
                    },
                )
                .unwrap();
        }
        panic!("job {job_id} did not reach approval gate");
    }

    #[test]
    fn approval_gate_diff_prevents_startup_and_duplicate_alerts_and_catches_rework_and_batches() {
        let mut engine = AfterlifeEngine::new();
        let human = engine.local_actor();
        let dist_id = DistrictId::from("DIST-001");
        let mut t = 1_900_000_000_000;

        let j_startup = engine.queue_preset_job("Already Waiting At Startup", Some(&dist_id));
        engine.assign_job_to_district(&j_startup, &dist_id).unwrap();
        advance_job_to_approval_gate(&mut engine, &j_startup, &mut t);

        let snap0 = snapshot_approval_gates(&engine);
        assert_eq!(snap0.len(), 1);
        assert!(diff_newly_entered_approval_gates(&snap0, &engine).is_empty());

        engine
            .apply(
                &human,
                EngineCommand::UpdateJob {
                    job_id: j_startup.clone(),
                    title: Some("Edited title while still at gate".into()),
                    raw_input: None,
                    spec_summary: None,
                    target_district_hint: None,
                    expected_revision: None,
                    fencing_token: None,
                    at: t + 100,
                },
            )
            .unwrap();
        assert!(diff_newly_entered_approval_gates(&snap0, &engine).is_empty());

        let j_batch1 = engine.queue_preset_job("Batch Arrival 1", Some(&dist_id));
        let j_batch2 = engine.queue_preset_job("Batch Arrival 2", Some(&dist_id));
        engine.assign_job_to_district(&j_batch1, &dist_id).unwrap();
        engine.assign_job_to_district(&j_batch2, &dist_id).unwrap();
        advance_job_to_approval_gate(&mut engine, &j_batch1, &mut t);
        advance_job_to_approval_gate(&mut engine, &j_batch2, &mut t);

        let batch = diff_newly_entered_approval_gates(&snap0, &engine);
        assert_eq!(batch.len(), 2);
        let batch_ids: Vec<&JobId> = batch.iter().map(|n| &n.job_id).collect();
        assert!(batch_ids.contains(&&j_batch1));
        assert!(batch_ids.contains(&&j_batch2));
        assert!(!batch_ids.contains(&&j_startup));

        let snap1 = snapshot_approval_gates(&engine);
        assert!(diff_newly_entered_approval_gates(&snap1, &engine).is_empty());

        t += 100;
        engine
            .apply(
                &human,
                EngineCommand::ReworkJob {
                    job_id: j_batch1.clone(),
                    expected_revision: None,
                    fencing_token: None,
                    reason: Some("Need one more check".into()),
                    at: t,
                },
            )
            .unwrap();
        advance_job_to_approval_gate(&mut engine, &j_batch1, &mut t);

        let coalesced_rearrival = diff_newly_entered_approval_gates(&snap1, &engine);
        assert_eq!(coalesced_rearrival.len(), 1);
        assert_eq!(coalesced_rearrival[0].job_id, j_batch1);
        assert_eq!(coalesced_rearrival[0].job_title, "Batch Arrival 1");
        assert!(matches!(ArtifactKind::Snapshot, ArtifactKind::Snapshot));
    }

    #[test]
    fn open_notification_target_switches_tab_and_falls_back_safely_for_completed_archived_or_missing() {
        let mut st = AppState::new_ephemeral();
        let dist_id = DistrictId::from("DIST-001");
        let j_active = st.engine_mut().queue_preset_job("Active Gate Job", Some(&dist_id));
        let j_archived = st.engine_mut().queue_preset_job("Archived Job", Some(&dist_id));
        st.engine_mut()
            .assign_job_to_district(&j_active, &dist_id)
            .unwrap();
        st.dispatch(EngineCommand::ArchiveJob {
            job_id: j_archived.clone(),
            reason: Some("Done".into()),
            expected_revision: None,
            fencing_token: None,
            at: 1_900_000_000_000,
        })
        .unwrap();

        st.select_tab(TabId::Templates);
        assert!(st.open_notification_target(&j_active));
        assert!(st.active_tab == TabId::Workspace);
        assert!(st.detail_open);
        assert_eq!(st.selected_job_id, j_active);

        st.close_detail();
        assert!(!st.open_notification_target(&j_archived));
        assert!(!st.detail_open);
        assert!(!st.open_notification_target(&JobId::from("JOB-DOES-NOT-EXIST")));
        assert!(!st.detail_open);
    }
}
