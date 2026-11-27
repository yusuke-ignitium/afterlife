use std::{
    collections::BTreeSet,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use afterlife_daemon::{AppDaemonState, build_router};
use afterlife_protocol::AfterlifeConfig;
use serde_json::Value;
use tokio::{net::TcpListener, sync::oneshot};

static DIR_SEQ: AtomicU64 = AtomicU64::new(1);

fn run_cli(base_url: &str, args: &[&str]) -> (bool, Value) {
    let bin = env!("CARGO_BIN_EXE_afterlife");
    let out = Command::new(bin)
        .env("AFTERLIFE_URL", base_url)
        .env_remove("AFTERLIFE_TOKEN")
        .args(args)
        .output()
        .expect("spawn afterlife CLI");

    let stream = if out.status.success() {
        &out.stdout
    } else {
        &out.stderr
    };
    let text = String::from_utf8_lossy(stream);
    let parsed: Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("CLI output for {args:?} not JSON ({e}): {text}"));
    (out.status.success(), parsed)
}

#[tokio::test]
async fn e2e_cli_subprocess_full_agent_workflow_and_approval_gate_guard() {
    let seq = DIR_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp_dir =
        std::env::temp_dir().join(format!("afterlife_cli_e2e_{}_{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{addr}");

    let mut cfg = AfterlifeConfig::desktop_embedded(tmp_dir.to_str().unwrap());
    cfg.server.bind_addr = addr.to_string();
    cfg.server.public_base_url = base_url.clone();
    if let afterlife_protocol::AuthConfig::LocalDev { ref mut issuer, .. } = cfg.auth {
        *issuer = base_url.clone();
    }

    let state = AppDaemonState::from_config(cfg).unwrap();
    let router = build_router(state);
    let (tx, rx) = oneshot::channel::<()>();
    let server_join = tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await;
    });

    let url_clone = base_url.clone();
    let tmp_dir_for_cli = tmp_dir.clone();
    tokio::task::spawn_blocking(move || {
        let (ok, health) = run_cli(&url_clone, &["health"]);
        assert!(ok, "health failed: {health}");
        assert_eq!(health["human_http_policy"], "desktop_only");

        let (ok, tools) = run_cli(&url_clone, &["tools"]);
        assert!(ok);
        let server_tool_names: BTreeSet<String> = tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(server_tool_names.len(), 24);
        assert!(server_tool_names.contains("districts_export_skills"));

        let (ok, exported_skills) = run_cli(&url_clone, &["skills", "export", "--print"]);
        assert!(ok, "skills export --print failed: {exported_skills}");
        assert_eq!(exported_skills["source"], "live");
        assert!(
            exported_skills["skills"]
                .as_array()
                .is_some_and(|arr| arr.iter().any(|s| s["slug"] == "afterlife"))
        );

        let (ok, linear_status) = run_cli(&url_clone, &["linear", "status"]);
        assert!(ok, "linear status failed: {linear_status}");
        assert_eq!(linear_status["connected"], false);
        assert!(
            linear_status["redirect_uri"]
                .as_str()
                .is_some_and(|u| u.ends_with("/oauth/linear/callback"))
        );

        let (ok, toolmap) = run_cli(&url_clone, &["toolmap"]);
        assert!(ok, "toolmap failed: {toolmap}");
        assert_eq!(toolmap["live"], true);
        assert!(toolmap.get("endpoint").is_none());
        assert_eq!(toolmap["human_http_policy"], "desktop_only");
        assert_eq!(
            toolmap["limits"]["min_lease_ttl_ms"],
            afterlife_core::engine::policy::MIN_LEASE_TTL_MS
        );
        assert_eq!(
            toolmap["limits"]["max_lease_ttl_ms"],
            afterlife_core::engine::policy::MAX_LEASE_TTL_MS
        );
        assert_eq!(
            toolmap["unmapped_server_tools"].as_array().unwrap().len(),
            0
        );
        let cli_mapped_tools: BTreeSet<String> = toolmap["toolmap"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|c| c["mcp_tool"].as_str().map(str::to_string))
            .collect();
        assert_eq!(cli_mapped_tools, server_tool_names);
        let backdrop_cmds: Vec<&str> = toolmap["toolmap"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|c| c["cmd"].as_str())
            .filter(|cmd| cmd.starts_with("afterlife backdrop "))
            .collect();
        assert_eq!(backdrop_cmds.len(), 6);

        let (ok, guide_alias) = run_cli(&url_clone, &["guide"]);
        assert!(ok);
        assert_eq!(guide_alias, toolmap);

        let (ok, dists) = run_cli(&url_clone, &["districts"]);
        assert!(ok);
        assert_eq!(dists["districts"].as_array().unwrap().len(), 5);

        let (ok, created) = run_cli(
            &url_clone,
            &[
                "jobs",
                "create",
                "CLI E2E Job",
                "--input",
                "Driven via afterlife CLI over MCP",
                "--district",
                "DIST-001",
            ],
        );
        assert!(ok, "create failed: {created}");
        let job_id = created["events"][0]["job_id"].as_str().unwrap().to_string();

        let (ok, _) = run_cli(&url_clone, &["jobs", "assign", &job_id, "DIST-001"]);
        assert!(ok);

        let (ok, ctx) = run_cli(&url_clone, &["jobs", "context", &job_id]);
        assert!(ok);
        assert_eq!(ctx["stage"]["id"], "discover");

        loop {
            let (ok, ctx) = run_cli(&url_clone, &["jobs", "context", &job_id]);
            assert!(ok, "context failed: {ctx}");
            let is_human_gate = ctx["stage"]["kind"] == "approval_gate"
                || ctx["stage"]["gate_rubric"]["requires_human_approval"] == true;
            if is_human_gate {
                break;
            }

            let (ok, claimed) = run_cli(
                &url_clone,
                &["jobs", "claim", &job_id, "--runner", "Grok Bot CLI"],
            );
            assert!(ok, "claim failed: {claimed}");
            let token = claimed["events"][0]["fencing_token"]
                .as_u64()
                .unwrap()
                .to_string();

            let art_kind = ctx["required_artifact_kinds"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_str())
                .unwrap_or("spec");
            let (ok_bad, bad_attach) = run_cli(
                &url_clone,
                &[
                    "artifacts",
                    "attach",
                    &job_id,
                    "--kind",
                    art_kind,
                    "--label",
                    "fake.json",
                    "--uri",
                    "evidence://fake/report.json",
                    "--fencing-token",
                    &token,
                ],
            );
            assert!(!ok_bad, "fictional URI should be rejected: {bad_attach}");
            assert_eq!(bad_attach["code"], "invalid_input");

            let evidence_file = tmp_dir_for_cli.join("cli-evidence.json");
            let evidence_body = format!("{{\"job\":\"{job_id}\",\"kind\":\"{art_kind}\"}}");
            std::fs::write(&evidence_file, &evidence_body).unwrap();
            let evidence_path = evidence_file.to_string_lossy().into_owned();
            let (ok, attached) = run_cli(
                &url_clone,
                &[
                    "artifacts",
                    "attach",
                    &job_id,
                    "--kind",
                    art_kind,
                    "--label",
                    "evidence.json",
                    "--file",
                    &evidence_path,
                    "--fencing-token",
                    &token,
                ],
            );
            assert!(ok, "attach failed: {attached}");
            let art_id = attached["events"][0]["artifact_id"]
                .as_str()
                .unwrap()
                .to_string();
            assert!(
                attached["events"][0]["uri"]
                    .as_str()
                    .unwrap()
                    .starts_with("blob://")
            );
            let (ok_read, art_read) = run_cli(&url_clone, &["artifacts", "read", &job_id, &art_id]);
            assert!(ok_read, "artifacts read failed: {art_read}");
            assert_eq!(art_read["storage"], "blob");
            assert_eq!(art_read["content"], evidence_body);

            let field_pairs: Vec<String> = ctx["required_step_fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| {
                    let key = f["key"].as_str().unwrap();
                    format!("{key}=Recorded {key} via CLI")
                })
                .collect();

            let mut adv_args: Vec<&str> = vec!["jobs", "advance", &job_id, "--fencing-token", &token, "--artifact-id", &art_id];
            for pair in &field_pairs {
                adv_args.push("--field");
                adv_args.push(pair.as_str());
            }

            if ctx["sub_stage"].is_null() {
                if let Some(checks_arr) = ctx["stage"]["gate_rubric"]["checks"].as_array() {
                    let checks: Vec<String> = checks_arr
                        .iter()
                        .map(|c| format!("{}=true", c.as_str().unwrap()))
                        .collect();
                    let mut gate_args = vec![
                        "gate",
                        "verify",
                        &job_id,
                        "--verdict",
                        "pass",
                        "--summary",
                        "Verified via CLI",
                        "--fencing-token",
                        &token,
                    ];
                    for c in &checks {
                        gate_args.push("--check");
                        gate_args.push(c.as_str());
                    }
                    let (ok, _) = run_cli(&url_clone, &gate_args);
                    assert!(ok);
                }
            } else {
                let (ok, adv) = run_cli(&url_clone, &adv_args);
                if ok {
                    continue;
                }
                assert_eq!(adv["code"], "gate_verification_required");
                let checks: Vec<String> = ctx["stage"]["gate_rubric"]["checks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| format!("{}=true", c.as_str().unwrap()))
                    .collect();
                let mut gate_args = vec![
                    "gate",
                    "verify",
                    &job_id,
                    "--verdict",
                    "pass",
                    "--summary",
                    "Stage exit verified via CLI",
                    "--fencing-token",
                    &token,
                ];
                for c in &checks {
                    gate_args.push("--check");
                    gate_args.push(c.as_str());
                }
                let (ok, _) = run_cli(&url_clone, &gate_args);
                assert!(ok);
            }

            let (ok, adv_ok) = run_cli(&url_clone, &adv_args);
            assert!(ok, "advance failed: {adv_ok}");
        }

        let (ok, gate_ctx) = run_cli(&url_clone, &["jobs", "context", &job_id]);
        assert!(ok, "gate context failed: {gate_ctx}");
        assert_eq!(gate_ctx["step_records"].as_array().unwrap().len(), 12);

        let (ok, _) = run_cli(
            &url_clone,
            &[
                "feedback",
                "submit",
                "DIST-001",
                "--kind",
                "philosophy_insight",
                "--body",
                "CLI works cleanly over MCP",
            ],
        );
        assert!(ok);

        let (ok, list) = run_cli(
            &url_clone,
            &["mcp", "call", "jobs_list", "--args-json", r#"{"limit":5}"#],
        );
        assert!(ok);
        assert!(!list["jobs"].as_array().unwrap().is_empty());

        let (ok, err) = run_cli(&url_clone, &["jobs", "advance", &job_id]);
        assert!(!ok, "CLI must NOT be able to advance ApprovalGate");
        assert_eq!(err["status"], 403);
        assert_eq!(err["code"], "human_approval_required");

        let (ok, err) = run_cli(
            &url_clone,
            &["jobs", "archive", &job_id, "--reason", "try bypass"],
        );
        assert!(!ok, "CLI must NOT be able to archive ApprovalGate job");
        assert_eq!(err["status"], 403);
        assert_eq!(err["code"], "human_approval_required");

        let (ok, draft) = run_cli(&url_clone, &["jobs", "create", "Draft before link"]);
        assert!(ok, "draft create failed: {draft}");
        let draft_id = draft["events"][0]["job_id"].as_str().unwrap().to_string();

        let (ok, upd) = run_cli(
            &url_clone,
            &[
                "jobs",
                "update",
                &draft_id,
                "--title",
                "[LIN-123](https://linear.app/example/issue/LIN-123) Updated title",
                "--summary",
                "Updated spec summary",
                "--expected-revision",
                "1",
            ],
        );
        assert!(ok, "update failed: {upd}");
        assert_eq!(upd["events"][0]["type"], "job_updated");
        assert_eq!(upd["events"][0]["revision"], 2);

        let (ok, arch) = run_cli(
            &url_clone,
            &["jobs", "archive", &draft_id, "--reason", "duplicate ticket"],
        );
        assert!(ok, "archive failed: {arch}");
        assert_eq!(arch["events"][0]["type"], "job_archived");

        let (ok, active_list) = run_cli(&url_clone, &["jobs", "list", "--limit", "50"]);
        assert!(ok);
        assert!(
            !active_list["jobs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|j| j["id"] == draft_id)
        );

        let (ok, all_list) = run_cli(
            &url_clone,
            &["jobs", "list", "--limit", "50", "--include-archived"],
        );
        assert!(ok);
        let archived_row = all_list["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|j| j["id"] == draft_id)
            .expect("archived job must appear with --include-archived");
        assert_eq!(
            archived_row["title"],
            "[LIN-123](https://linear.app/example/issue/LIN-123) Updated title"
        );
        assert_eq!(archived_row["location"]["state"], "archived");
    })
    .await
    .unwrap();

    let _ = tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(2), server_join).await;
    let _ = std::fs::remove_dir_all(&tmp_dir);
}
