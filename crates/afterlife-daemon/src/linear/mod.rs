pub mod client;
pub mod inbound;
pub mod links;
pub mod oauth;
pub mod sync;
#[cfg(test)]
mod sync_tests;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use afterlife_core::{JobEvent, OwnerId};
use afterlife_protocol::{
    AuthConfig, export_workspace_skills, write_skill_bundle_atomic,
};

use crate::{
    AppDaemonState,
    linear::{
        client::{HttpLinearClient, LinearApi},
        oauth::PendingLinearOAuthStore,
        sync::{LinearSyncContext, now_ms, sync_outbound_events_once, sync_owner_once},
    },
};

const DEFAULT_SYNC_INTERVAL_SECS: u64 = 30;

pub struct LinearBridge {
    pub api: Arc<dyn LinearApi>,
    pub pending_oauth: PendingLinearOAuthStore,
    pub refresh_lock: Arc<Mutex<()>>,
}

impl Default for LinearBridge {
    fn default() -> Self {
        Self::new(Arc::new(HttpLinearClient::new()))
    }
}

impl LinearBridge {
    pub fn new(api: Arc<dyn LinearApi>) -> Self {
        Self {
            api,
            pending_oauth: PendingLinearOAuthStore::default(),
            refresh_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn sync_context(&self, state: &AppDaemonState) -> LinearSyncContext {
        LinearSyncContext {
            store: state.ports.store.clone(),
            linear: state.ports.linear.clone(),
            event_bus: state.ports.event_bus.clone(),
            api: self.api.clone(),
            refresh_lock: self.refresh_lock.clone(),
        }
    }
}

fn default_owner_id(state: &AppDaemonState) -> Option<OwnerId> {
    match &state.config.auth {
        AuthConfig::LocalDev {
            default_owner_id, ..
        } => Some(OwnerId::from(default_owner_id.clone())),
        AuthConfig::GoogleOidc { .. } => None,
    }
}

fn event_updates_district_skills(event: &JobEvent) -> bool {
    matches!(
        event,
        JobEvent::PhilosophyUpdated { .. }
            | JobEvent::SpecUpdated { .. }
            | JobEvent::StageSubTemplateChanged { .. }
            | JobEvent::TemplateUpdated { .. }
            | JobEvent::BuiltinTemplatesSynced { .. }
            | JobEvent::DistrictCreated { .. }
            | JobEvent::DistrictArchived { .. }
            | JobEvent::DistrictRestored { .. }
    )
}

fn event_is_human_governance(event: &JobEvent) -> bool {
    match event {
        JobEvent::JobReworked { actor, .. }
        | JobEvent::JobTransitioned { actor, .. }
        | JobEvent::JobStopped { actor, .. }
        | JobEvent::JobResumed { actor, .. } => actor.is_human(),
        _ => false,
    }
}

pub fn export_owner_skills_to_data_dir(state: &AppDaemonState, owner_id: &OwnerId) {
    let Some(skills_dir) = state.skills_dir() else {
        return;
    };
    if let Ok(snapshot) = state.ports.store.load_workspace(owner_id) {
        let bundle = export_workspace_skills(&snapshot.engine, None);
        if let Err(err) = write_skill_bundle_atomic(&skills_dir, &bundle, true) {
            eprintln!("[afterlifed] failed to auto-export district skills: {err}");
        }
    }
}

pub fn spawn_background_services(state: Arc<AppDaemonState>) {
    let Some(owner_id) = default_owner_id(&state) else {
        return;
    };

    export_owner_skills_to_data_dir(&state, &owner_id);

    let event_state = state.clone();
    let event_owner = owner_id.clone();
    tokio::spawn(async move {
        let mut rx = event_state.ports.event_bus.subscribe();
        while let Ok(record) = rx.recv().await {
            if record.owner_id != event_owner {
                continue;
            }
            if event_updates_district_skills(&record.event) {
                let st = event_state.clone();
                let own = event_owner.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    export_owner_skills_to_data_dir(&st, &own);
                })
                .await;
            }
            if event_is_human_governance(&record.event) {
                let st = event_state.clone();
                let own = event_owner.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    let ctx = st.linear.sync_context(&st);
                    if let Err(err) = sync_outbound_events_once(&ctx, &own, now_ms()) {
                        eprintln!("[afterlifed] linear outbound sync error: {err}");
                    }
                })
                .await;
            }
        }
    });

    let poll_secs = std::env::var("AFTERLIFE_LINEAR_SYNC_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&s| s >= 5)
        .unwrap_or(DEFAULT_SYNC_INTERVAL_SECS);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(poll_secs));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let st = state.clone();
            let own = owner_id.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let connected = st
                    .ports
                    .linear
                    .load_token(&own)
                    .ok()
                    .flatten()
                    .is_some();
                if !connected {
                    return;
                }
                let ctx = st.linear.sync_context(&st);
                if let Err(err) = sync_owner_once(&ctx, &own, now_ms()) {
                    eprintln!("[afterlifed] linear periodic sync error: {err}");
                }
            })
            .await;
        }
    });
}
