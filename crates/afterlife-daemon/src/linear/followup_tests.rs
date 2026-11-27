use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use afterlife_core::{
    Actor, ArtifactKind, CommandId, DistrictId, EngineCommand, GateVerdict, JobLocation, OwnerId,
};
use afterlife_protocol::{
    CommandEnvelope, LinearOAuthToken, LinearSignalV1, LinearSyncStore, SignalAction,
    SignalSource, format_afterlife_comment,
};
use afterlife_storage::{InMemoryEventBus, SqliteLinearStore, SqliteWorkspaceStore};

use crate::linear::{
    client::{
        LinearApi, LinearApiError, LinearComment, LinearIssue, LinearIssueState,
        LinearTokenResponse, LinearViewer,
    },
    sync::{LinearSyncContext, sync_owner_once},
};

struct FollowupMockApi {
    issues: Mutex<Vec<LinearIssue>>,
    posted: Mutex<Vec<(String, String)>>,
}

impl FollowupMockApi {
    fn new(issues: Vec<LinearIssue>) -> Self {
        Self {
            issues: Mutex::new(issues),
            posted: Mutex::new(Vec::new()),
        }
    }

    fn set_issues(&self, issues: Vec<LinearIssue>) {
        *self.issues.lock().unwrap() = issues;
    }

    fn posted_comments(&self) -> Vec<(String, String)> {
        self.posted.lock().unwrap().clone()
    }
}

impl LinearApi for FollowupMockApi {
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
        Ok((
            LinearViewer {
                id: "v-1".into(),
                name: "Alice".into(),
                email: None,
            },
            self.issues.lock().unwrap().clone(),
        ))
    }

    fn fetch_issue_states(
        &self,
        _access_token: &str,
        _issue_ids: &[String],
    ) -> Result<Vec<LinearIssueState>, LinearApiError> {
        Ok(Vec::new())
    }

    fn create_comment(
        &self,
        _access_token: &str,
        issue_id: &str,
        body: &str,
    ) -> Result<LinearComment, LinearApiError> {
        let mut posted = self.posted.lock().unwrap();
        let idx = posted.len() + 1;
        posted.push((issue_id.to_string(), body.to_string()));
        Ok(LinearComment {
            id: format!("out-{idx}"),
            body: body.to_string(),
            url: format!("https://linear.app/acme/issue/ENG-101#comment-out-{idx}"),
            author_name: Some("Afterlife".into()),
            created_at: "2026-11-27T12:00:00Z".into(),
        })
    }
}

fn setup_ctx(
    issues: Vec<LinearIssue>,
) -> (LinearSyncContext, Arc<FollowupMockApi>, OwnerId) {
    let store = Arc::new(SqliteWorkspaceStore::open_in_memory().unwrap());
    let linear = Arc::new(SqliteLinearStore::open_in_memory().unwrap());
    let event_bus = Arc::new(InMemoryEventBus::new(64));
    let api = Arc::new(FollowupMockApi::new(issues));
    let owner = OwnerId::from("owner-1");
    linear
        .save_token(&LinearOAuthToken {
            owner_id: owner.clone(),
            client_id: "cid".into(),
            client_secret: None,
            access_token: "tok".into(),
            refresh_token: None,
            expires_at_ms: Some(9_000_000_000),
            scope: "read comments:create".into(),
            viewer_id: Some("v-1".into()),
            viewer_name: Some("Alice".into()),
            viewer_email: None,
            default_district_id: None,
            updated_at_ms: 1_000,
        })
        .unwrap();
    let ctx = LinearSyncContext {
        store,
        linear,
        event_bus,
        api: api.clone(),
        refresh_lock: Arc::new(Mutex::new(())),
    };
    (ctx, api, owner)
}

fn make_issue(id: &str, ident: &str, comments: Vec<LinearComment>) -> LinearIssue {
    LinearIssue {
        id: id.into(),
        identifier: ident.into(),
        title: "Notification settings".into(),
        description: Some("Design and implement notification settings".into()),
        url: format!("https://linear.app/acme/issue/{ident}/slug"),
        state_name: "In Progress".into(),
        state_type: "started".into(),
        comments,
    }
}

fn comment_with_signal(id: &str, signal: LinearSignalV1) -> LinearComment {
    let body = format_afterlife_comment("Step", None, &signal);
    LinearComment {
        id: id.into(),
        body,
        url: format!("https://linear.app/acme/issue/ENG-101#comment-{id}"),
        author_name: Some("Cloud Agent".into()),
        created_at: "2026-11-27T12:00:00Z".into(),
    }
}

fn enter_signal(district_id: &str) -> LinearSignalV1 {
    LinearSignalV1 {
        version: 1,
        source: SignalSource::Agent,
        action: SignalAction::EnterDistrict,
        district_id: Some(district_id.into()),
        stage_id: None,
        sub_stage_id: None,
        target_stage_id: None,
        runner_label: Some("Cloud Agent".into()),
        summary: None,
        fields: BTreeMap::new(),
        gate: None,
        artifacts: Vec::new(),
        afterlife_job_id: None,
        afterlife_seq: None,
        reason: None,
    }
}

fn complete_dist002_with_spec(
    ctx: &LinearSyncContext,
    owner: &OwnerId,
    job_id: &afterlife_core::JobId,
    now: u64,
) {
    let agent = Actor::Agent {
        owner_id: owner.clone(),
        client_id: "test-agent".into(),
        runner_label: "Test Agent".into(),
        token_jti: "jti-1".into(),
    };
    let human = Actor::Human {
        owner_id: owner.clone(),
        email: "human@local".into(),
    };
    let exec = |id: &str, actor: Actor, command: EngineCommand| {
        ctx.store
            .execute_command(CommandEnvelope {
                command_id: CommandId::from(id.to_string()),
                actor,
                command,
            })
            .unwrap()
    };

    exec(
        "d2-enter",
        agent.clone(),
        EngineCommand::AssignJobToDistrict {
            job_id: job_id.clone(),
            district_id: DistrictId::from("DIST-002"),
            expected_revision: None,
            at: now,
        },
    );

    let mut step = 0u64;
    loop {
        step += 1;
        let snap = ctx.store.load_workspace(owner).unwrap();
        let job = snap.engine.find_job(job_id).unwrap().clone();
        let JobLocation::InDistrict {
            district_id,
            stage_id,
            sub_stage_id,
        } = job.location
        else {
            break;
        };
        let dist = snap.engine.find_district(&district_id).unwrap();
        let st = dist
            .active_spec()
            .stages
            .iter()
            .find(|s| s.id == stage_id)
            .unwrap()
            .clone();
        if st.is_gate() {
            exec(
                &format!("d2-approve-{step}"),
                human.clone(),
                EngineCommand::ApproveJob {
                    job_id: job_id.clone(),
                    note: Some("LGTM".into()),
                    expected_revision: None,
                    at: now + step * 10,
                },
            );
            break;
        }
        let claim = exec(
            &format!("d2-claim-{step}"),
            agent.clone(),
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: afterlife_core::RunnerId::from("r1"),
                runner_label: "Test".into(),
                ttl_ms: 30_000,
                expected_revision: None,
                at: now + step * 10,
            },
        );
        let tok = claim
            .events
            .iter()
            .find_map(|e| match e {
                afterlife_core::JobEvent::JobClaimed { fencing_token, .. } => Some(*fencing_token),
                _ => None,
            })
            .unwrap();

        let exec_ctx = snap.engine.compile_execution_context(job_id).unwrap();
        let mut fields = BTreeMap::new();
        for f in exec_ctx.required_step_fields {
            fields.insert(f.key, "evidence".into());
        }
        let mut art_ids = Vec::new();
        if let Some(kind) = exec_ctx.required_artifact_kinds.first().copied() {
            let attached = exec(
                &format!("d2-art-{step}"),
                agent.clone(),
                EngineCommand::AttachArtifact {
                    job_id: job_id.clone(),
                    kind,
                    label: "ENG-101-Spec".into(),
                    uri: "https://linear.app/acme/document/eng-101-spec".into(),
                    sha256: None,
                    fencing_token: Some(tok),
                    at: now + step * 10 + 1,
                },
            );
            if let Some(aid) = attached.events.iter().find_map(|e| match e {
                afterlife_core::JobEvent::ArtifactAttached { artifact_id, .. } => {
                    Some(artifact_id.clone())
                }
                _ => None,
            }) {
                art_ids.push(aid);
            }
        }
        let is_last_sub = match (&st.sub_template_id, &sub_stage_id) {
            (Some(tid), Some(sid)) => snap
                .engine
                .templates
                .get(tid)
                .and_then(|tpl| tpl.stages.last())
                .is_some_and(|last| &last.id == sid),
            _ => true,
        };
        if is_last_sub && let Some(rubric) = &st.gate_rubric {
            let checks = rubric.checks.iter().map(|c| (c.clone(), true)).collect();
            exec(
                &format!("d2-gate-{step}"),
                agent.clone(),
                EngineCommand::RecordGateVerification {
                    job_id: job_id.clone(),
                    verdict: GateVerdict::Pass,
                    summary: "passed".into(),
                    checklist_results: checks,
                    fencing_token: Some(tok),
                    at: now + step * 10 + 2,
                },
            );
        }
        exec(
            &format!("d2-adv-{step}"),
            agent.clone(),
            EngineCommand::AdvanceJob {
                job_id: job_id.clone(),
                fields,
                artifact_ids: art_ids,
                expected_revision: None,
                fencing_token: Some(tok),
                at: now + step * 10 + 3,
            },
        );
    }
}

#[test]
fn completed_job_spawns_followup_job_for_next_district_and_advances_in_same_sync() {
    let issue = make_issue("lin-101", "ENG-101", Vec::new());
    let (ctx, api, owner) = setup_ctx(vec![issue]);

    let first = sync_owner_once(&ctx, &owner, 10_000).unwrap();
    assert_eq!(first.jobs_created, 1);
    let link_before = ctx
        .linear
        .find_link_by_issue(&owner, "lin-101")
        .unwrap()
        .unwrap();
    let old_job_id = link_before.job_id.clone();

    complete_dist002_with_spec(&ctx, &owner, &old_job_id, 20_000);
    let snap = ctx.store.load_workspace(&owner).unwrap();
    assert!(matches!(
        snap.engine.find_job(&old_job_id).unwrap().location,
        JobLocation::Completed { .. }
    ));

    let mut adv_fields = BTreeMap::new();
    adv_fields.insert("sources_read".into(), "src/notifications/client.ts".into());
    adv_fields.insert("findings".into(), "client requires 5s request timeout".into());
    let advance_sig = LinearSignalV1 {
        version: 1,
        source: SignalSource::Agent,
        action: SignalAction::AdvanceStage,
        district_id: Some("DIST-001".into()),
        stage_id: Some("discover".into()),
        sub_stage_id: Some("trace_sources".into()),
        target_stage_id: None,
        runner_label: Some("Cloud Agent".into()),
        summary: None,
        fields: adv_fields,
        gate: None,
        artifacts: Vec::new(),
        afterlife_job_id: None,
        afterlife_seq: None,
        reason: None,
    };

    api.set_issues(vec![make_issue(
        "lin-101",
        "ENG-101",
        vec![
            comment_with_signal("cmt-dup-d2", enter_signal("DIST-002")),
            comment_with_signal("cmt-enter-d1", enter_signal("DIST-001")),
            comment_with_signal("cmt-adv-d1", advance_sig),
        ],
    )]);

    let report = sync_owner_once(&ctx, &owner, 30_000).unwrap();
    assert_eq!(report.sync_errors_posted, 0);
    assert_eq!(report.jobs_created, 1);
    assert_eq!(report.signals_applied, 3);

    let link_after = ctx
        .linear
        .find_link_by_issue(&owner, "lin-101")
        .unwrap()
        .unwrap();
    assert_ne!(link_after.job_id, old_job_id);

    let snap_after = ctx.store.load_workspace(&owner).unwrap();
    assert!(matches!(
        snap_after.engine.find_job(&old_job_id).unwrap().location,
        JobLocation::Completed { ref district_id, .. } if district_id.as_str() == "DIST-002"
    ));

    let new_job = snap_after.engine.find_job(&link_after.job_id).unwrap();
    assert!(matches!(
        &new_job.location,
        JobLocation::InDistrict { district_id, stage_id, sub_stage_id }
            if district_id.as_str() == "DIST-001"
                && stage_id.as_str() == "discover"
                && sub_stage_id.as_ref().map(|s| s.as_str()) == Some("isolate_probe")
    ));
    assert!(new_job.artifacts.iter().any(|a| a.kind == ArtifactKind::Issue));
    assert!(
        new_job
            .artifacts
            .iter()
            .any(|a| a.kind == ArtifactKind::Spec && a.label == "ENG-101-Spec")
    );
}

#[test]
fn archived_job_stays_archived_until_new_enter_district_spawns_followup() {
    let issue = make_issue("lin-102", "ENG-102", Vec::new());
    let (ctx, api, owner) = setup_ctx(vec![issue]);

    sync_owner_once(&ctx, &owner, 10_000).unwrap();
    let old_job_id = ctx
        .linear
        .find_link_by_issue(&owner, "lin-102")
        .unwrap()
        .unwrap()
        .job_id;

    let human = Actor::Human {
        owner_id: owner.clone(),
        email: "human@local".into(),
    };
    ctx.store
        .execute_command(CommandEnvelope {
            command_id: CommandId::from("archive-old"),
            actor: human,
            command: EngineCommand::ArchiveJob {
                job_id: old_job_id.clone(),
                reason: Some("tidying board".into()),
                expected_revision: None,
                fencing_token: None,
                at: 15_000,
            },
        })
        .unwrap();

    let quiet = sync_owner_once(&ctx, &owner, 20_000).unwrap();
    assert_eq!(quiet.jobs_created, 0);
    assert_eq!(
        ctx.linear
            .find_link_by_issue(&owner, "lin-102")
            .unwrap()
            .unwrap()
            .job_id,
        old_job_id
    );

    api.set_issues(vec![make_issue(
        "lin-102",
        "ENG-102",
        vec![comment_with_signal("cmt-reenter", enter_signal("DIST-001"))],
    )]);
    let reopened = sync_owner_once(&ctx, &owner, 30_000).unwrap();
    assert_eq!(reopened.sync_errors_posted, 0);
    assert_eq!(reopened.jobs_created, 1);
    assert_eq!(reopened.signals_applied, 1);

    let new_job_id = ctx
        .linear
        .find_link_by_issue(&owner, "lin-102")
        .unwrap()
        .unwrap()
        .job_id;
    assert_ne!(new_job_id, old_job_id);
    let snap = ctx.store.load_workspace(&owner).unwrap();
    assert!(matches!(
        snap.engine.find_job(&old_job_id).unwrap().location,
        JobLocation::Archived
    ));
    assert!(matches!(
        &snap.engine.find_job(&new_job_id).unwrap().location,
        JobLocation::InDistrict { district_id, .. } if district_id.as_str() == "DIST-001"
    ));
}

#[test]
fn followup_rejects_unknown_district_without_creating_orphan_job() {
    let issue = make_issue("lin-103", "ENG-103", Vec::new());
    let (ctx, api, owner) = setup_ctx(vec![issue]);

    sync_owner_once(&ctx, &owner, 10_000).unwrap();
    let old_job_id = ctx
        .linear
        .find_link_by_issue(&owner, "lin-103")
        .unwrap()
        .unwrap()
        .job_id;
    complete_dist002_with_spec(&ctx, &owner, &old_job_id, 20_000);
    let count_before = ctx.store.load_workspace(&owner).unwrap().engine.jobs.len();

    api.set_issues(vec![make_issue(
        "lin-103",
        "ENG-103",
        vec![comment_with_signal("cmt-bad-dist", enter_signal("DIST-999"))],
    )]);
    let report = sync_owner_once(&ctx, &owner, 30_000).unwrap();
    assert_eq!(report.sync_errors_posted, 1);
    assert_eq!(report.jobs_created, 0);
    assert_eq!(
        ctx.store.load_workspace(&owner).unwrap().engine.jobs.len(),
        count_before
    );
    assert!(
        api.posted_comments()
            .iter()
            .any(|(_, body)| body.contains("DIST-999"))
    );
}

#[test]
fn active_in_district_job_cannot_be_hijacked_by_other_district_enter() {
    let issue = make_issue(
        "lin-104",
        "ENG-104",
        vec![
            comment_with_signal("cmt-enter-d2", enter_signal("DIST-002")),
            comment_with_signal("cmt-hijack-d1", enter_signal("DIST-001")),
        ],
    );
    let (ctx, _api, owner) = setup_ctx(vec![issue]);

    let report = sync_owner_once(&ctx, &owner, 10_000).unwrap();
    assert_eq!(report.jobs_created, 1);
    assert_eq!(report.signals_applied, 1);
    assert_eq!(report.sync_errors_posted, 1);

    let job_id = ctx
        .linear
        .find_link_by_issue(&owner, "lin-104")
        .unwrap()
        .unwrap()
        .job_id;
    let snap = ctx.store.load_workspace(&owner).unwrap();
    assert!(matches!(
        &snap.engine.find_job(&job_id).unwrap().location,
        JobLocation::InDistrict { district_id, .. } if district_id.as_str() == "DIST-002"
    ));
}
