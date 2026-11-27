use std::sync::{Arc, Mutex};

use afterlife_core::{
    Actor, CommandId, DistrictId, EngineCommand, JobLocation, OwnerId, StageKind,
};
use afterlife_protocol::{
    CommandEnvelope, LinearOAuthToken, LinearSyncStore, WorkspaceStore,
};
use afterlife_storage::{InMemoryEventBus, SqliteLinearStore, SqliteWorkspaceStore};

use crate::linear::{
    client::{
        LinearApi, LinearApiError, LinearComment, LinearIssue, LinearIssueState,
        LinearTokenResponse, LinearViewer,
    },
    sync::{LinearSyncContext, sync_owner_once},
};

struct ArchiveMockState {
    open_issues: Vec<LinearIssue>,
    issue_states: Vec<LinearIssueState>,
}

struct ArchiveMockApi {
    state: Mutex<ArchiveMockState>,
}

impl ArchiveMockApi {
    fn new(open_issues: Vec<LinearIssue>) -> Self {
        Self {
            state: Mutex::new(ArchiveMockState {
                open_issues,
                issue_states: Vec::new(),
            }),
        }
    }

    fn set_open_and_states(
        &self,
        open_issues: Vec<LinearIssue>,
        issue_states: Vec<LinearIssueState>,
    ) {
        let mut st = self.state.lock().unwrap();
        st.open_issues = open_issues;
        st.issue_states = issue_states;
    }
}

impl LinearApi for ArchiveMockApi {
    fn exchange_code(
        &self,
        _client_id: &str,
        _client_secret: Option<&str>,
        _code: &str,
        _redirect_uri: &str,
        _code_verifier: &str,
    ) -> Result<LinearTokenResponse, LinearApiError> {
        unreachable!()
    }

    fn refresh_token(
        &self,
        _client_id: &str,
        _client_secret: Option<&str>,
        _refresh_token: &str,
    ) -> Result<LinearTokenResponse, LinearApiError> {
        unreachable!()
    }

    fn fetch_viewer(&self, _access_token: &str) -> Result<LinearViewer, LinearApiError> {
        Ok(LinearViewer {
            id: "v-1".into(),
            name: "Alice".into(),
            email: None,
        })
    }

    fn fetch_open_assigned_issues(
        &self,
        _access_token: &str,
    ) -> Result<(LinearViewer, Vec<LinearIssue>), LinearApiError> {
        let st = self.state.lock().unwrap();
        Ok((
            LinearViewer {
                id: "v-1".into(),
                name: "Alice".into(),
                email: None,
            },
            st.open_issues.clone(),
        ))
    }

    fn fetch_issue_states(
        &self,
        _access_token: &str,
        issue_ids: &[String],
    ) -> Result<Vec<LinearIssueState>, LinearApiError> {
        let st = self.state.lock().unwrap();
        Ok(st
            .issue_states
            .iter()
            .filter(|s| issue_ids.contains(&s.id))
            .cloned()
            .collect())
    }

    fn create_comment(
        &self,
        _access_token: &str,
        _issue_id: &str,
        _body: &str,
    ) -> Result<LinearComment, LinearApiError> {
        unreachable!()
    }
}

fn make_issue(id: &str, ident: &str) -> LinearIssue {
    LinearIssue {
        id: id.into(),
        identifier: ident.into(),
        title: format!("Task {ident}"),
        description: None,
        url: format!("https://linear.app/acme/issue/{ident}"),
        state_name: "In Progress".into(),
        state_type: "started".into(),
        comments: vec![],
    }
}

#[test]
fn linear_completed_or_canceled_auto_archives_queue_and_work_but_protects_reassigned_and_approval_gate()
{
    let owner_id = OwnerId::from("local-owner");
    let store: Arc<dyn WorkspaceStore> = Arc::new(SqliteWorkspaceStore::open_in_memory().unwrap());
    let linear: Arc<dyn LinearSyncStore> = Arc::new(SqliteLinearStore::open_in_memory().unwrap());
    let event_bus = Arc::new(InMemoryEventBus::new(64));

    let api = Arc::new(ArchiveMockApi::new(vec![
        make_issue("iss-q-done", "ENG-1"),
        make_issue("iss-w-cancel", "ENG-2"),
        make_issue("iss-reassigned", "ENG-3"),
        make_issue("iss-gate-done", "ENG-4"),
    ]));

    linear
        .save_token(&LinearOAuthToken {
            owner_id: owner_id.clone(),
            client_id: "c-1".into(),
            client_secret: None,
            access_token: "acc-1".into(),
            refresh_token: None,
            expires_at_ms: None,
            scope: "read,comments:create".into(),
            viewer_id: None,
            viewer_name: None,
            viewer_email: None,
            default_district_id: None,
            updated_at_ms: 1_000,
        })
        .unwrap();

    let ctx = LinearSyncContext {
        store: store.clone(),
        linear: linear.clone(),
        event_bus,
        api: api.clone(),
        refresh_lock: Arc::new(Mutex::new(())),
    };
    let now = 2_000_000_000_000u64;

    let first = sync_owner_once(&ctx, &owner_id, now).unwrap();
    assert_eq!(first.jobs_created, 4);

    let job_q = linear.find_link_by_issue(&owner_id, "iss-q-done").unwrap().unwrap().job_id;
    let job_w = linear.find_link_by_issue(&owner_id, "iss-w-cancel").unwrap().unwrap().job_id;
    let job_reassigned = linear.find_link_by_issue(&owner_id, "iss-reassigned").unwrap().unwrap().job_id;
    let job_gate = linear.find_link_by_issue(&owner_id, "iss-gate-done").unwrap().unwrap().job_id;

    store
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("assign-w"),
            actor: Actor::local_human(owner_id.clone()),
            command: EngineCommand::AssignJobToDistrict {
                job_id: job_w.clone(),
                district_id: DistrictId::from("DIST-001"),
                expected_revision: None,
                at: now + 10,
            },
        })
        .unwrap();

    store
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("assign-gate"),
            actor: Actor::local_human(owner_id.clone()),
            command: EngineCommand::AssignJobToDistrict {
                job_id: job_gate.clone(),
                district_id: DistrictId::from("DIST-002"),
                expected_revision: None,
                at: now + 20,
            },
        })
        .unwrap();

    let mut step = 0u64;
    loop {
        let snap = store.load_workspace(&owner_id).unwrap();
        let j = snap.engine.find_job(&job_gate).unwrap();
        let JobLocation::InDistrict { district_id, stage_id, sub_stage_id } = &j.location else {
            break;
        };
        let stage = snap.engine.find_stage(district_id, stage_id).unwrap();
        if stage.kind == StageKind::ApprovalGate {
            break;
        }
        let ctx_data = snap.engine.compile_execution_context(&job_gate).unwrap();
        let agent = Actor::Agent {
            owner_id: owner_id.clone(),
            client_id: "local-test".into(),
            runner_label: "Tester".into(),
            token_jti: format!("t-{step}"),
        };
        let claim = store
            .execute_command(CommandEnvelope {
                command_id: CommandId::from(format!("claim-gate-{step}")),
                actor: agent.clone(),
                command: EngineCommand::ClaimJob {
                    job_id: job_gate.clone(),
                    runner_id: afterlife_core::RunnerId::from("tester"),
                    runner_label: "Tester".into(),
                    ttl_ms: 60_000,
                    expected_revision: None,
                    at: now + 100 + step * 10,
                },
            })
            .unwrap();
        let tok = claim.events.iter().find_map(|e| match e {
            afterlife_core::JobEvent::JobClaimed { fencing_token, .. } => Some(*fencing_token),
            _ => None,
        }).unwrap();
        let mut aids = Vec::new();
        if let Some(&kind) = ctx_data.required_artifact_kinds.first() {
            let art = store
                .execute_command(CommandEnvelope {
                    command_id: CommandId::from(format!("art-gate-{step}")),
                    actor: agent.clone(),
                    command: EngineCommand::AttachArtifact {
                        job_id: job_gate.clone(),
                        kind,
                        label: "spec".into(),
                        uri: "https://linear.app/acme/issue/ENG-4".into(),
                        sha256: None,
                        fencing_token: Some(tok),
                        at: now + 101 + step * 10,
                    },
                })
                .unwrap();
            if let Some(id) = art.events.iter().find_map(|e| match e {
                afterlife_core::JobEvent::ArtifactAttached { artifact_id, .. } => Some(artifact_id.clone()),
                _ => None,
            }) {
                aids.push(id);
            }
        }
        if let Some(ref rubric) = ctx_data.gate_rubric {
            let is_exit = sub_stage_id.is_none()
                || stage
                    .sub_template_id
                    .as_ref()
                    .and_then(|tid| snap.engine.templates.get(tid))
                    .and_then(|t| t.stages.last())
                    .is_some_and(|l| sub_stage_id.as_ref() == Some(&l.id));
            if is_exit {
                store
                    .execute_command(CommandEnvelope {
                        command_id: CommandId::from(format!("v-gate-{step}")),
                        actor: agent.clone(),
                        command: EngineCommand::RecordGateVerification {
                            job_id: job_gate.clone(),
                            verdict: afterlife_core::GateVerdict::Pass,
                            summary: "ok".into(),
                            checklist_results: rubric.checks.iter().map(|c| (c.clone(), true)).collect(),
                            fencing_token: Some(tok),
                            at: now + 102 + step * 10,
                        },
                    })
                    .unwrap();
            }
        }
        let mut fields = std::collections::BTreeMap::new();
        for f in &ctx_data.required_step_fields {
            fields.insert(f.key.clone(), "ok".into());
        }
        store
            .execute_command(CommandEnvelope {
                command_id: CommandId::from(format!("adv-gate-{step}")),
                actor: agent,
                command: EngineCommand::AdvanceJob {
                    job_id: job_gate.clone(),
                    fields,
                    artifact_ids: aids,
                    expected_revision: None,
                    fencing_token: Some(tok),
                    at: now + 103 + step * 10,
                },
            })
            .unwrap();
        step += 1;
    }

    api.set_open_and_states(
        vec![],
        vec![
            LinearIssueState {
                id: "iss-q-done".into(),
                state_name: "Done".into(),
                state_type: "completed".into(),
                archived: false,
            },
            LinearIssueState {
                id: "iss-w-cancel".into(),
                state_name: "Duplicate".into(),
                state_type: "canceled".into(),
                archived: false,
            },
            LinearIssueState {
                id: "iss-reassigned".into(),
                state_name: "In Progress".into(),
                state_type: "started".into(),
                archived: false,
            },
            LinearIssueState {
                id: "iss-gate-done".into(),
                state_name: "Done".into(),
                state_type: "completed".into(),
                archived: false,
            },
        ],
    );

    let rep = sync_owner_once(&ctx, &owner_id, now + 10_000).unwrap();
    assert_eq!(rep.jobs_archived, 2);

    let snap = store.load_workspace(&owner_id).unwrap();
    assert!(matches!(snap.engine.find_job(&job_q).unwrap().location, JobLocation::Archived));
    assert!(matches!(snap.engine.find_job(&job_w).unwrap().location, JobLocation::Archived));
    assert!(matches!(snap.engine.find_job(&job_reassigned).unwrap().location, JobLocation::Queue));
    assert!(matches!(
        snap.engine.find_job(&job_gate).unwrap().location,
        JobLocation::InDistrict { .. }
    ));
}

#[test]
fn canonical_linear_url_strips_japanese_slug_and_upgrades_legacy_queue_job_links() {
    use afterlife_core::{ArtifactKind, parse_inline_links};

    let owner_id = OwnerId::from("local-owner");
    let store: Arc<dyn WorkspaceStore> = Arc::new(SqliteWorkspaceStore::open_in_memory().unwrap());
    let linear: Arc<dyn LinearSyncStore> = Arc::new(SqliteLinearStore::open_in_memory().unwrap());
    let event_bus = Arc::new(InMemoryEventBus::new(64));
    let slugged = "https://linear.app/acme/issue/ENG-853/日本語のタイトル slug";
    let issue = LinearIssue {
        id: "iss-slug-1".into(),
        identifier: "ENG-853".into(),
        title: "Replace docs link".into(),
        description: Some("Body text".into()),
        url: slugged.into(),
        state_name: "In Progress".into(),
        state_type: "started".into(),
        comments: vec![],
    };
    let api = Arc::new(ArchiveMockApi::new(vec![issue]));
    linear
        .save_token(&LinearOAuthToken {
            owner_id: owner_id.clone(),
            client_id: "c-1".into(),
            client_secret: None,
            access_token: "acc-1".into(),
            refresh_token: None,
            expires_at_ms: None,
            scope: "read,comments:create".into(),
            viewer_id: None,
            viewer_name: None,
            viewer_email: None,
            default_district_id: None,
            updated_at_ms: 1_000,
        })
        .unwrap();
    let ctx = LinearSyncContext {
        store: store.clone(),
        linear: linear.clone(),
        event_bus,
        api,
        refresh_lock: Arc::new(Mutex::new(())),
    };
    let now = 2_000_000_000_000u64;

    let legacy_out = store
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("legacy-queue"),
            actor: Actor::local_human(owner_id.clone()),
            command: EngineCommand::QueueJob {
                title: "[ENG-853] Replace docs link".into(),
                raw_input: format!("Linear: ENG-853 ({slugged})\nState: In Progress\n"),
                target_district_hint: None,
                at: now - 1_000,
            },
        })
        .unwrap();
    let legacy_job_id = legacy_out
        .events
        .iter()
        .find_map(|e| match e {
            afterlife_core::JobEvent::JobQueued { job_id, .. } => Some(job_id.clone()),
            _ => None,
        })
        .unwrap();
    linear
        .upsert_link(&afterlife_protocol::LinearIssueLink {
            owner_id: owner_id.clone(),
            linear_issue_id: "iss-slug-1".into(),
            linear_identifier: "ENG-853".into(),
            linear_url: slugged.into(),
            job_id: legacy_job_id.clone(),
            last_synced_at_ms: now - 1_000,
        })
        .unwrap();

    sync_owner_once(&ctx, &owner_id, now).unwrap();

    let snap = store.load_workspace(&owner_id).unwrap();
    let job = snap.engine.find_job(&legacy_job_id).unwrap();
    let title_links = parse_inline_links(&job.title);
    assert_eq!(title_links.links.len(), 1);
    assert_eq!(title_links.links[0].label, "ENG-853");
    assert_eq!(
        title_links.links[0].url,
        "https://linear.app/acme/issue/ENG-853"
    );
    let body_links = parse_inline_links(&job.raw_input);
    assert_eq!(body_links.links.len(), 1);
    assert_eq!(
        body_links.links[0].url,
        "https://linear.app/acme/issue/ENG-853"
    );
    assert!(job.artifacts.iter().any(|a| {
        a.kind == ArtifactKind::Issue && a.uri == "https://linear.app/acme/issue/ENG-853"
    }));
}
