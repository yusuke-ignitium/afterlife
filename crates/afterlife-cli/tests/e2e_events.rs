use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use afterlife_core::{Actor, CommandId, DistrictId, EngineCommand, JobEvent, JobId, OwnerId};
use afterlife_daemon::{AppDaemonState, build_router};
use afterlife_protocol::{AfterlifeConfig, CommandEnvelope, EventRecord};
use serde_json::Value;
use tokio::net::TcpListener;

static NEXT_DIR_SEQ: AtomicU64 = AtomicU64::new(1);
static NEXT_CMD_SEQ: AtomicU64 = AtomicU64::new(1);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let seq = NEXT_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "afterlife_cli_events_{tag}_{}_{seq}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct TestDaemon {
    addr: SocketAddr,
    state: Arc<AppDaemonState>,
    runtime: Option<tokio::runtime::Runtime>,
}

impl TestDaemon {
    async fn start_on(data_dir: &Path, bind_addr: SocketAddr) -> Self {
        let data_dir_str = data_dir.to_str().unwrap().to_string();
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap();
            let cfg = AfterlifeConfig::local_default(&data_dir_str);
            let state = AppDaemonState::from_config(cfg).unwrap();
            let app = build_router(state.clone());
            let (addr, listener) = rt.block_on(async {
                let listener = TcpListener::bind(bind_addr).await.unwrap();
                let addr = listener.local_addr().unwrap();
                (addr, listener)
            });
            rt.spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            Self {
                addr,
                state,
                runtime: Some(rt),
            }
        })
        .await
        .unwrap()
    }

    async fn start(data_dir: &Path) -> Self {
        Self::start_on(data_dir, SocketAddr::from(([127, 0, 0, 1], 0))).await
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn latest_seq(&self) -> u64 {
        self.state
            .ports
            .store
            .load_workspace(&OwnerId::from("local-owner"))
            .unwrap()
            .latest_seq
    }

    fn exec_human(&self, command: EngineCommand) -> Vec<JobEvent> {
        let owner = OwnerId::from("local-owner");
        let cmd_id = CommandId::new(format!(
            "test-cmd-{}",
            NEXT_CMD_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let outcome = self
            .state
            .ports
            .store
            .execute_command(CommandEnvelope {
                command_id: cmd_id.clone(),
                actor: Actor::local_human(owner.clone()),
                command,
            })
            .unwrap();
        for (idx, ev) in outcome.events.iter().enumerate() {
            self.state.ports.event_bus.publish(EventRecord {
                seq: outcome.seq_start + idx as u64,
                owner_id: owner.clone(),
                command_id: cmd_id.clone(),
                event: ev.clone(),
            });
        }
        outcome.events
    }

    async fn stop(mut self) {
        if let Some(rt) = self.runtime.take() {
            tokio::task::spawn_blocking(move || {
                rt.shutdown_timeout(Duration::from_millis(20));
            })
            .await
            .unwrap();
        }
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        if let Some(rt) = self.runtime.take() {
            rt.shutdown_background();
        }
    }
}

fn cli_bin() -> &'static str {
    env!("CARGO_BIN_EXE_afterlife")
}

async fn run_cli(url: String, args: Vec<String>) -> (i32, String, String) {
    tokio::task::spawn_blocking(move || {
        let out = Command::new(cli_bin())
            .args(&args)
            .env("AFTERLIFE_URL", &url)
            .env_remove("AFTERLIFE_TOKEN")
            .output()
            .expect("spawn afterlife cli");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    })
    .await
    .unwrap()
}

fn str_args(slice: &[&str]) -> Vec<String> {
    slice.iter().map(|s| (*s).to_string()).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_toolmap_preserves_zero_unmapped_mcp_tools_guard() {
    let dir = TempDir::new("toolmap_guard");
    let daemon = TestDaemon::start(dir.path()).await;

    let (code, stdout, stderr) = run_cli(daemon.url(), str_args(&["toolmap"])).await;
    assert_eq!(code, 0, "stderr: {stderr}");
    let toolmap: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(toolmap["live"], true);
    assert_eq!(toolmap["unmapped_server_tools"].as_array().unwrap().len(), 0);
    let cmds: Vec<&str> = toolmap["toolmap"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["cmd"].as_str())
        .collect();
    assert!(cmds.iter().any(|c| c.starts_with("afterlife wait ")));
    assert!(cmds.iter().any(|c| c.starts_with("afterlife watch ")));

    daemon.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wait_handles_precheck_live_sse_stop_timeout_and_reconnect() {
    let dir = TempDir::new("wait_flow");
    let daemon = TestDaemon::start(dir.path()).await;
    let dist_id = DistrictId::from("DIST-001");

    let q_ev = daemon.exec_human(EngineCommand::QueueJob {
        title: "Wait Target Job".into(),
        raw_input: "raw".into(),
        target_district_hint: Some(dist_id.clone()),
        at: 1_900_000_000_000,
    });
    let job_id = match &q_ev[0] {
        JobEvent::JobQueued { job_id, .. } => job_id.clone(),
        _ => unreachable!(),
    };

    let (code, stdout, stderr) = run_cli(
        daemon.url(),
        vec![
            "wait".into(),
            "--job".into(),
            job_id.to_string(),
            "--until".into(),
            "queued".into(),
            "--timeout".into(),
            "5".into(),
        ],
    )
    .await;
    assert_eq!(code, 0, "race-free precheck failed: {stderr}");
    let pre_val: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(pre_val["outcome"], "queued");
    assert_eq!(pre_val["already_satisfied"], true);

    daemon.exec_human(EngineCommand::AssignJobToDistrict {
        job_id: job_id.clone(),
        district_id: dist_id.clone(),
        expected_revision: None,
        at: 1_900_000_001_000,
    });

    let url = daemon.url();
    let jid_str = job_id.to_string();
    let wait_stop = tokio::spawn(run_cli(
        url.clone(),
        vec![
            "wait".into(),
            "--job".into(),
            jid_str.clone(),
            "--until".into(),
            "approved,reworked,resumed".into(),
            "--exit-on-stop".into(),
            "--timeout".into(),
            "5".into(),
        ],
    ));
    tokio::time::sleep(Duration::from_millis(120)).await;
    daemon.exec_human(EngineCommand::StopJob {
        job_id: job_id.clone(),
        reason: Some("Operator hold".into()),
        expected_revision: None,
        at: 1_900_000_002_000,
    });
    let (code, stdout, stderr) = wait_stop.await.unwrap();
    assert_eq!(code, 3, "expected exit code 3 on human stop, stderr: {stderr}");
    let stop_val: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(stop_val["outcome"], "stopped");
    assert_eq!(stop_val["stopped"], true);

    let (code, stdout, stderr) = run_cli(
        url.clone(),
        vec![
            "wait".into(),
            "--job".into(),
            jid_str.clone(),
            "--until".into(),
            "resumed".into(),
            "--timeout".into(),
            "1".into(),
        ],
    )
    .await;
    assert_eq!(code, 124, "expected timeout exit 124, stderr: {stderr}");
    let to_val: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(to_val["timed_out"], true);
    let last_seq = to_val["last_seq"].as_u64().unwrap();

    daemon.exec_human(EngineCommand::ResumeJob {
        job_id: job_id.clone(),
        expected_revision: None,
        at: 1_900_000_003_000,
    });
    let (code, stdout, stderr) = run_cli(
        url.clone(),
        vec![
            "wait".into(),
            "--job".into(),
            jid_str.clone(),
            "--until".into(),
            "resumed".into(),
            "--since-seq".into(),
            last_seq.to_string(),
            "--timeout".into(),
            "5".into(),
        ],
    )
    .await;
    assert_eq!(code, 0, "expected lossless resume from last_seq, stderr: {stderr}");
    let res_val: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(res_val["outcome"], "resumed");
    assert_eq!(res_val["stopped"], false);

    let saved_addr = daemon.addr;
    let wait_reconnect = tokio::spawn(run_cli(
        url.clone(),
        vec![
            "wait".into(),
            "--job".into(),
            jid_str.clone(),
            "--until".into(),
            "reworked,resumed".into(),
            "--timeout".into(),
            "8".into(),
        ],
    ));
    tokio::time::sleep(Duration::from_millis(120)).await;
    daemon.stop().await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    let daemon2 = TestDaemon::start_on(dir.path(), saved_addr).await;
    daemon2.exec_human(EngineCommand::StopJob {
        job_id: job_id.clone(),
        reason: Some("Pause before resume".into()),
        expected_revision: None,
        at: 1_900_000_004_000,
    });
    daemon2.exec_human(EngineCommand::ResumeJob {
        job_id: job_id.clone(),
        expected_revision: None,
        at: 1_900_000_005_000,
    });
    let (code, stdout, stderr) = wait_reconnect.await.unwrap();
    assert_eq!(code, 0, "expected reconnect resume wakeup, stderr: {stderr}");
    let rec_val: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(rec_val["outcome"], "resumed");

    daemon2.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn watch_ndjson_streams_matching_events_and_filters_noise() {
    let dir = TempDir::new("watch_ndjson");
    let daemon = TestDaemon::start(dir.path()).await;
    let dist_id = DistrictId::from("DIST-001");

    let q1 = daemon.exec_human(EngineCommand::QueueJob {
        title: "Watched Job".into(),
        raw_input: "raw".into(),
        target_district_hint: Some(dist_id.clone()),
        at: 1_900_000_010_000,
    });
    let j1 = match &q1[0] {
        JobEvent::JobQueued { job_id, .. } => job_id.clone(),
        _ => unreachable!(),
    };
    let q2 = daemon.exec_human(EngineCommand::QueueJob {
        title: "Other Noise Job".into(),
        raw_input: "raw".into(),
        target_district_hint: Some(dist_id.clone()),
        at: 1_900_000_011_000,
    });
    let j2 = match &q2[0] {
        JobEvent::JobQueued { job_id, .. } => job_id.clone(),
        _ => unreachable!(),
    };
    daemon.exec_human(EngineCommand::AssignJobToDistrict {
        job_id: j1.clone(),
        district_id: dist_id.clone(),
        expected_revision: None,
        at: 1_900_000_012_000,
    });
    daemon.exec_human(EngineCommand::AssignJobToDistrict {
        job_id: j2.clone(),
        district_id: dist_id,
        expected_revision: None,
        at: 1_900_000_013_000,
    });

    let since_before_watch = daemon.latest_seq();
    let watch_task = tokio::spawn(run_cli(
        daemon.url(),
        vec![
            "watch".into(),
            "--job".into(),
            j1.to_string(),
            "--events".into(),
            "stopped,resumed".into(),
            "--since-seq".into(),
            since_before_watch.to_string(),
            "--max-events".into(),
            "2".into(),
            "--timeout".into(),
            "5".into(),
            "--ndjson".into(),
        ],
    ));
    tokio::time::sleep(Duration::from_millis(80)).await;

    daemon.exec_human(EngineCommand::StopJob {
        job_id: j2,
        reason: Some("Noise job stop".into()),
        expected_revision: None,
        at: 1_900_000_014_000,
    });
    daemon.exec_human(EngineCommand::StopJob {
        job_id: j1.clone(),
        reason: Some("Target stop".into()),
        expected_revision: None,
        at: 1_900_000_015_000,
    });
    daemon.exec_human(EngineCommand::ResumeJob {
        job_id: j1.clone(),
        expected_revision: None,
        at: 1_900_000_016_000,
    });

    let (code, stdout, stderr) = watch_task.await.unwrap();
    assert_eq!(code, 0, "watch failed: {stderr}");
    let lines: Vec<Value> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["job_id"], j1.as_str());
    assert_eq!(lines[0]["outcome"], "stopped");
    assert_eq!(lines[1]["job_id"], j1.as_str());
    assert_eq!(lines[1]["outcome"], "resumed");
    assert_ne!(JobId::from("JOB-X"), j1);

    daemon.stop().await;
}
