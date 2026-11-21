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
        assert_eq!(server_tool_names.len(), 16);

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
        assert_eq!(dists["districts"].as_array().unwrap().len(), 2);

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
        assert_eq!(ctx["stage"]["id"], "design");
        let checks: Vec<String> = ctx["stage"]["gate_rubric"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| format!("{}=true", c.as_str().unwrap()))
            .collect();

        let (ok, claimed) = run_cli(
            &url_clone,
            &["jobs", "claim", &job_id, "--runner", "Grok Bot CLI"],
        );
        assert!(ok, "claim failed: {claimed}");
        let token = claimed["events"][0]["fencing_token"]
            .as_u64()
            .unwrap()
            .to_string();

        let (ok, _) = run_cli(
            &url_clone,
            &[
                "artifacts",
                "attach",
                &job_id,
                "--kind",
                "spec",
                "--label",
                "design.md",
                "--uri",
                "https://example.com/design.md",
                "--fencing-token",
                &token,
            ],
        );
        assert!(ok);

        let mut gate_args = vec![
            "gate",
            "verify",
            &job_id,
            "--verdict",
            "pass",
            "--summary",
            "Design verified via CLI",
            "--fencing-token",
            &token,
        ];
        for c in &checks {
            gate_args.push("--check");
            gate_args.push(c.as_str());
        }
        let (ok, _) = run_cli(&url_clone, &gate_args);
        assert!(ok);

        let (ok, _) = run_cli(
            &url_clone,
            &["jobs", "advance", &job_id, "--fencing-token", &token],
        );
        assert!(ok);

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

        let (ok, err) = run_cli(&url_clone, &["jobs", "advance", "JOB-041"]);
        assert!(!ok, "CLI must NOT be able to advance ApprovalGate");
        assert_eq!(err["status"], 403);
        assert_eq!(err["code"], "human_approval_required");

        let (ok, err) = run_cli(
            &url_clone,
            &["jobs", "archive", "JOB-041", "--reason", "try bypass"],
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

fn run_cli_with_data_dir(data_dir: &std::path::Path, args: &[&str]) -> (bool, Value) {
    let bin = env!("CARGO_BIN_EXE_afterlife");
    let out = Command::new(bin)
        .env("AFTERLIFE_DATA_DIR", data_dir)
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

#[test]
fn e2e_cli_backdrop_crud_and_tuning() {
    let seq = DIR_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp_dir = std::env::temp_dir().join(format!(
        "afterlife_backdrop_cli_{}_{seq}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let tiny_png: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240,
        31, 0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let img_a = tmp_dir.join("alpha.png");
    let img_b = tmp_dir.join("beta.png");
    let img_c = tmp_dir.join("gamma.png");
    let fake_txt = tmp_dir.join("notes.txt");
    std::fs::write(&img_a, tiny_png).unwrap();
    std::fs::write(&img_b, tiny_png).unwrap();
    std::fs::write(&img_c, tiny_png).unwrap();
    std::fs::write(&fake_txt, "definitely not an image").unwrap();

    let (ok, list0) = run_cli_with_data_dir(&tmp_dir, &["backdrop", "list"]);
    assert!(ok, "backdrop list failed: {list0}");
    assert_eq!(list0["count"], 0);
    assert_eq!(list0["tuning"]["image_enabled"], false);
    assert_eq!(list0["tuning"]["apple_music_artwork_enabled"], true);
    assert!((list0["tuning"]["tile_size_px"].as_f64().unwrap() - 6.0).abs() < 1e-4);
    assert!((list0["tuning"]["image_hold_secs"].as_f64().unwrap() - 10.0).abs() < 1e-4);
    assert!((list0["tuning"]["image_brightness"].as_f64().unwrap() - 0.40).abs() < 1e-4);
    assert!((list0["tuning"]["image_peak_luminance"].as_f64().unwrap() - 0.55).abs() < 1e-4);
    assert!((list0["tuning"]["image_saturation"].as_f64().unwrap() - 0.64).abs() < 1e-4);
    assert!((list0["tuning"]["master_brightness"].as_f64().unwrap() - 1.50).abs() < 1e-4);

    let (ok, rejected) = run_cli_with_data_dir(
        &tmp_dir,
        &[
            "backdrop",
            "add",
            fake_txt.to_str().unwrap(),
            "--label",
            "bad",
        ],
    );
    assert!(!ok, "non-image file must be rejected: {rejected}");

    let store = afterlife_protocol::BackdropStore::open(&tmp_dir);
    let saved_glass = afterlife_protocol::BackdropGlassAppearance {
        blur_std_dev: 5.5,
        radius: 9.0,
        tint_rgb: [27, 45, 56],
        pause_animation_when_inactive: true,
        ..afterlife_protocol::BackdropGlassAppearance::default()
    };
    store
        .save_appearance(afterlife_protocol::BackdropTuning::default(), saved_glass)
        .unwrap();

    let (ok, added1) = run_cli_with_data_dir(
        &tmp_dir,
        &[
            "backdrop",
            "add",
            img_a.to_str().unwrap(),
            "--label",
            "first",
        ],
    );
    assert!(ok, "backdrop add 1 failed: {added1}");
    assert_eq!(added1["added"]["id"], "img-1");
    assert_eq!(added1["added"]["label"], "first");
    assert_eq!(added1["count"], 1);
    assert!(std::path::Path::new(added1["added"]["path"].as_str().unwrap()).is_file());
    assert_eq!(store.load().unwrap().glass, Some(saved_glass));

    let (ok, added2) = run_cli_with_data_dir(
        &tmp_dir,
        &[
            "backdrop",
            "add",
            img_b.to_str().unwrap(),
            "--label",
            "second",
        ],
    );
    assert!(ok, "backdrop add 2 failed: {added2}");
    assert_eq!(added2["added"]["id"], "img-2");
    assert_eq!(added2["count"], 2);

    let (ok, noop_update) = run_cli_with_data_dir(&tmp_dir, &["backdrop", "update", "img-2"]);
    assert!(
        !ok,
        "update with no mutation flags must fail: {noop_update}"
    );

    let (ok, missing_update) = run_cli_with_data_dir(
        &tmp_dir,
        &["backdrop", "update", "img-999", "--label", "ghost"],
    );
    assert!(!ok, "update on unknown id must fail: {missing_update}");

    let (ok, updated) = run_cli_with_data_dir(
        &tmp_dir,
        &[
            "backdrop",
            "update",
            "img-2",
            "--path",
            img_c.to_str().unwrap(),
            "--label",
            "promoted",
            "--index",
            "0",
        ],
    );
    assert!(ok, "backdrop update failed: {updated}");
    assert_eq!(updated["updated"]["label"], "promoted");
    assert_eq!(updated["images"][0]["id"], "img-2");
    assert_eq!(updated["images"][1]["id"], "img-1");
    assert_eq!(store.load().unwrap().glass, Some(saved_glass));

    let (ok, tuned) = run_cli_with_data_dir(
        &tmp_dir,
        &[
            "backdrop",
            "tuning",
            "--enable-images",
            "--tile-size",
            "10.5",
            "--hold-secs",
            "15.0",
            "--image-brightness",
            "0.65",
            "--image-peak",
            "0.70",
            "--image-saturation",
            "0.80",
            "--master-brightness",
            "1.40",
        ],
    );
    assert!(ok, "backdrop tuning failed: {tuned}");
    assert_eq!(tuned["tuning"]["image_enabled"], true);
    assert!((tuned["tuning"]["tile_size_px"].as_f64().unwrap() - 11.0).abs() < 1e-4);
    assert!((tuned["tuning"]["image_hold_secs"].as_f64().unwrap() - 15.0).abs() < 1e-4);
    assert!((tuned["tuning"]["image_brightness"].as_f64().unwrap() - 0.65).abs() < 1e-4);
    assert!((tuned["tuning"]["image_peak_luminance"].as_f64().unwrap() - 0.70).abs() < 1e-4);
    assert!((tuned["tuning"]["image_saturation"].as_f64().unwrap() - 0.80).abs() < 1e-4);
    assert!((tuned["tuning"]["master_brightness"].as_f64().unwrap() - 1.40).abs() < 1e-4);
    assert_eq!(store.load().unwrap().glass, Some(saved_glass));

    let (ok, conflict) = run_cli_with_data_dir(
        &tmp_dir,
        &["backdrop", "tuning", "--enable-images", "--disable-images"],
    );
    assert!(
        !ok,
        "conflicting enable/disable flags should fail: {conflict}"
    );

    let (ok, disabled) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--disable-images"]);
    assert!(ok, "backdrop tuning disable failed: {disabled}");
    assert_eq!(disabled["tuning"]["image_enabled"], false);

    let (ok, reenabled) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--enable-images"]);
    assert!(ok, "backdrop tuning re-enable failed: {reenabled}");
    assert_eq!(reenabled["tuning"]["image_enabled"], true);

    let (ok, am_conflict) = run_cli_with_data_dir(
        &tmp_dir,
        &[
            "backdrop",
            "tuning",
            "--enable-apple-music",
            "--disable-apple-music",
        ],
    );
    assert!(
        !ok,
        "conflicting apple music enable/disable flags should fail: {am_conflict}"
    );

    let (ok, am_disabled) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--disable-apple-music"]);
    assert!(
        ok,
        "backdrop tuning disable apple music failed: {am_disabled}"
    );
    assert_eq!(am_disabled["tuning"]["apple_music_artwork_enabled"], false);

    let (ok, am_reenabled) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--enable-apple-music"]);
    assert!(
        ok,
        "backdrop tuning enable apple music failed: {am_reenabled}"
    );
    assert_eq!(am_reenabled["tuning"]["apple_music_artwork_enabled"], true);

    let (ok, am_disabled_before_reset) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--disable-apple-music"]);
    assert!(
        ok,
        "backdrop tuning disable before reset failed: {am_disabled_before_reset}"
    );

    let (ok, yt_conflict) = run_cli_with_data_dir(
        &tmp_dir,
        &[
            "backdrop",
            "tuning",
            "--enable-youtube-mirror",
            "--disable-youtube-mirror",
        ],
    );
    assert!(
        !ok,
        "conflicting youtube mirror enable/disable flags should fail: {yt_conflict}"
    );

    let (ok, yt_disabled) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--disable-youtube-mirror"]);
    assert!(
        ok,
        "backdrop tuning disable youtube mirror failed: {yt_disabled}"
    );
    assert_eq!(yt_disabled["tuning"]["youtube_mirror_enabled"], false);

    let (ok, yt_reenabled) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--enable-youtube-mirror"]);
    assert!(
        ok,
        "backdrop tuning enable youtube mirror failed: {yt_reenabled}"
    );
    assert_eq!(yt_reenabled["tuning"]["youtube_mirror_enabled"], true);

    let (ok, yt_disabled_before_reset) =
        run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--disable-youtube-mirror"]);
    assert!(
        ok,
        "backdrop tuning disable youtube before reset failed: {yt_disabled_before_reset}"
    );

    let (ok, reset_tuned) = run_cli_with_data_dir(&tmp_dir, &["backdrop", "tuning", "--reset"]);
    assert!(ok, "backdrop tuning reset failed: {reset_tuned}");
    assert_eq!(reset_tuned["tuning"]["image_enabled"], false);
    assert_eq!(reset_tuned["tuning"]["apple_music_artwork_enabled"], true);
    assert_eq!(reset_tuned["tuning"]["youtube_mirror_enabled"], true);
    assert!((reset_tuned["tuning"]["tile_size_px"].as_f64().unwrap() - 6.0).abs() < 1e-4);
    assert!((reset_tuned["tuning"]["image_hold_secs"].as_f64().unwrap() - 10.0).abs() < 1e-4);
    assert!((reset_tuned["tuning"]["image_brightness"].as_f64().unwrap() - 0.40).abs() < 1e-4);
    assert_eq!(store.load().unwrap().glass, Some(saved_glass));

    let (ok, missing_remove) = run_cli_with_data_dir(&tmp_dir, &["backdrop", "remove", "img-999"]);
    assert!(!ok, "remove on unknown id must fail: {missing_remove}");

    let removed_file = added1["added"]["path"].as_str().unwrap().to_string();
    let (ok, removed) = run_cli_with_data_dir(&tmp_dir, &["backdrop", "remove", "img-1"]);
    assert!(ok, "backdrop remove failed: {removed}");
    assert_eq!(removed["count"], 1);
    assert!(!std::path::Path::new(&removed_file).exists());
    assert_eq!(store.load().unwrap().glass, Some(saved_glass));

    let (ok, cleared) = run_cli_with_data_dir(&tmp_dir, &["backdrop", "clear"]);
    assert!(ok, "backdrop clear failed: {cleared}");
    assert_eq!(cleared["removed_count"], 1);
    assert_eq!(cleared["count"], 0);
    assert_eq!(store.load().unwrap().glass, Some(saved_glass));

    let _ = std::fs::remove_dir_all(&tmp_dir);
}
