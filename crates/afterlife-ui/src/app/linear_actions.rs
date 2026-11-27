use std::sync::Arc;

use afterlife_daemon::linear::{
    oauth::build_linear_status,
    sync::{now_ms, sync_owner_once},
};
use afterlife_protocol::{
    LinearSyncStatus, default_backdrop_data_dir, export_workspace_skills, write_skill_bundle_atomic,
};

use crate::app::state::AppState;

pub fn read_linear_status(st: &AppState) -> Option<LinearSyncStatus> {
    let daemon = st.daemon.as_ref()?;
    build_linear_status(daemon, &st.owner_id).ok()
}

pub fn trigger_linear_sync_async(st: &AppState) {
    let Some(daemon) = st.daemon.clone() else {
        return;
    };
    let owner_id = st.owner_id.clone();
    let _ = std::thread::Builder::new()
        .name("afterlife-linear-sync".into())
        .spawn(move || {
            let ctx = daemon.linear.sync_context(&daemon);
            if let Err(err) = sync_owner_once(&ctx, &owner_id, now_ms()) {
                eprintln!("[afterlife-ui] linear manual sync failed: {err}");
            }
        });
}

pub fn disconnect_linear(st: &AppState) {
    let Some(daemon) = st.daemon.as_ref() else {
        return;
    };
    if let Err(err) = daemon.ports.linear.delete_token(&st.owner_id) {
        eprintln!("[afterlife-ui] linear disconnect failed: {err}");
    }
}

pub fn export_skills_now(st: &AppState) -> Result<String, String> {
    let engine = Arc::clone(&st.engine);
    let out_dir = st
        .daemon
        .as_ref()
        .and_then(|d| d.skills_dir())
        .unwrap_or_else(|| default_backdrop_data_dir().join("skills"));
    let bundle = export_workspace_skills(&engine, None);
    let report = write_skill_bundle_atomic(&out_dir, &bundle, true)?;
    Ok(format!(
        "Exported {} skills to {}",
        bundle.skills.len(),
        report.out_dir
    ))
}
