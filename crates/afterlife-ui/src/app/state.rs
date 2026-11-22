use std::{
    collections::{BTreeMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use afterlife_core::{
    Actor, AfterlifeEngine, Artifact, CommandId, EngineCommand, Job, JobEvent, JobId, OwnerId,
    ProposalId, TimestampMs,
};
use afterlife_daemon::AppDaemonState;
use afterlife_design::{GlassSettings, Palette, UiStyle};
use afterlife_protocol::{
    AfterlifeConfig, BackdropStore, CommandEnvelope, EventRecord, resolve_tile_size_for_display,
    sanitize_display_tile_sizes,
};
use gpui::{Entity, Image, ImageFormat, set_particle_wave_tuning};

pub use crate::app::appearance::{GlassParamKey, GlassParamSection};
pub use crate::app::artifact_preview::{ArtifactPreview, ArtifactPreviewBody};
use crate::{
    app::{
        appearance::{apply_backdrop_config, snapshot_glass_appearance, snapshot_tuning},
        artifact_preview::load_artifact_preview,
        notifications::{
            ApprovalGateEntry, diff_newly_entered_approval_gates, snapshot_approval_gates,
        },
    },
    perf::PerfSession,
    platform::notifications::{deliver_approval_notification, sync_dock_badge},
};

static UI_COMMAND_SEQ: AtomicU64 = AtomicU64::new(1);

pub fn now_ms() -> TimestampMs {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as TimestampMs)
        .unwrap_or(0)
}

fn default_data_dir() -> String {
    afterlife_protocol::default_backdrop_data_dir()
        .to_string_lossy()
        .into_owned()
}

const APP_TITLE_LOGO_PNG: &[u8] = include_bytes!("../../assets/afterlife-logo.png");

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TabId {
    Workspace,
    Templates,
    Guard,
}

#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<AfterlifeEngine>,
    pub daemon: Option<Arc<AppDaemonState>>,
    pub backdrop_store: Option<BackdropStore>,
    pub owner_id: OwnerId,
    pub human_actor: Actor,
    pub app_title_logo: Arc<Image>,
    pub selected_job_id: JobId,
    pub selected_proposal_id: Option<ProposalId>,
    pub proposal_error: Option<String>,
    pub detail_open: bool,
    pub detail_height: f32,
    pub active_tab: TabId,
    pub previous_tab: TabId,
    pub show_centerline: bool,
    pub style: UiStyle,
    pub glass: GlassSettings,
    pub base_tile_size_px: f32,
    pub display_tile_sizes: BTreeMap<String, f32>,
    pub active_display_uuid: Option<String>,
    pub active_display_scale: f32,
    pub palette: Palette,
    pub settings_open: bool,
    pub inspector_open: bool,
    pub pause_animation_when_inactive: bool,
    pub fade_ui_after_secs: f32,
    pub approval_notifications_enabled: bool,
    approval_gate_snapshot: BTreeMap<JobId, ApprovalGateEntry>,
    pub collapsed_step_groups: HashSet<(JobId, usize)>,
    pub preview_artifact: Option<ArtifactPreview>,
}

impl AppState {
    pub fn new() -> Self {
        let ephemeral = std::env::var("AFTERLIFE_EPHEMERAL").is_ok()
            || std::env::var("AFTERLIFE_PERF_SCENARIO").is_ok();
        let perf_use_live_data = std::env::var("AFTERLIFE_PERF_USE_LIVE_DATA").is_ok();
        Self::new_with_mode(ephemeral, perf_use_live_data)
    }

    #[cfg(test)]
    pub(crate) fn new_ephemeral() -> Self {
        Self::new_with_mode(true, false)
    }

    fn new_with_mode(ephemeral: bool, perf_use_live_data: bool) -> Self {
        let backdrop_store = (!ephemeral).then(BackdropStore::open_default);

        let style = UiStyle::LiquidGlass;
        let mut glass = GlassSettings::default();
        let mut base_tile_size_px = glass.bg_tile_size_px;
        let mut display_tile_sizes = BTreeMap::new();
        let mut pause_animation_when_inactive = false;
        let mut fade_ui_after_secs = 0.0;
        let mut approval_notifications_enabled = true;
        let load_backdrop_store = backdrop_store
            .clone()
            .or_else(|| perf_use_live_data.then(BackdropStore::open_default));
        if let Some(ref store) = load_backdrop_store {
            match store.load() {
                Ok(backdrop) => {
                    apply_backdrop_config(
                        &mut glass,
                        &mut pause_animation_when_inactive,
                        &mut fade_ui_after_secs,
                        &mut approval_notifications_enabled,
                        &backdrop,
                    );
                    base_tile_size_px = backdrop.tuning.clamped().tile_size_px;
                    display_tile_sizes = sanitize_display_tile_sizes(backdrop.display_tile_sizes);
                }
                Err(e) => eprintln!("afterlife-ui: failed to load appearance config ({e})"),
            }
        }
        set_particle_wave_tuning(glass.particle_wave_tuning());
        let palette = Palette::from_style(style, &glass);
        let app_title_logo = Arc::new(Image::from_bytes(
            ImageFormat::Png,
            APP_TITLE_LOGO_PNG.to_vec(),
        ));

        let cfg = std::env::var("AFTERLIFE_CONFIG")
            .ok()
            .and_then(|p| AfterlifeConfig::from_toml_file(p).ok())
            .unwrap_or_else(|| AfterlifeConfig::desktop_embedded(&default_data_dir()));

        let owner_id = OwnerId::from(
            std::env::var("AFTERLIFE_OWNER_ID").unwrap_or_else(|_| "local-owner".to_string()),
        );
        let human_actor = Actor::Human {
            owner_id: owner_id.clone(),
            email: std::env::var("AFTERLIFE_OWNER_EMAIL")
                .unwrap_or_else(|_| "you@local".to_string()),
        };

        let (daemon, engine) = if ephemeral && !perf_use_live_data {
            (None, AfterlifeEngine::new_for_owner(owner_id.clone()))
        } else {
            match AppDaemonState::from_config(cfg) {
                Ok(d) => {
                    let eng = d
                        .ports
                        .store
                        .load_workspace(&owner_id)
                        .map(|snap| snap.engine)
                        .unwrap_or_else(|_| AfterlifeEngine::new_for_owner(owner_id.clone()));
                    let kept_daemon = (!ephemeral).then_some(d);
                    (kept_daemon, eng)
                }
                Err(e) => {
                    eprintln!(
                        "afterlife-ui: failed to initialize embedded daemon state ({e}); falling back to in-memory"
                    );
                    (None, AfterlifeEngine::new_for_owner(owner_id.clone()))
                }
            }
        };

        let selected_job_id = engine
            .jobs
            .first()
            .map(|j| j.id.clone())
            .unwrap_or_else(|| JobId::from(""));

        let approval_gate_snapshot = snapshot_approval_gates(&engine);

        Self {
            engine: Arc::new(engine),
            daemon,
            backdrop_store,
            owner_id,
            human_actor,
            app_title_logo,
            selected_job_id,
            selected_proposal_id: None,
            proposal_error: None,
            detail_open: false,
            detail_height: crate::app::window_state::DEFAULT_DETAIL_HEIGHT_PX,
            active_tab: TabId::Workspace,
            previous_tab: TabId::Workspace,
            show_centerline: false,
            style,
            glass,
            base_tile_size_px,
            display_tile_sizes,
            active_display_uuid: None,
            active_display_scale: 2.0,
            palette,
            settings_open: false,
            inspector_open: false,
            pause_animation_when_inactive,
            fade_ui_after_secs,
            approval_notifications_enabled,
            approval_gate_snapshot,
            collapsed_step_groups: HashSet::new(),
            preview_artifact: None,
        }
    }

    pub fn open_artifact_preview(&mut self, art: &Artifact) {
        self.preview_artifact = Some(load_artifact_preview(
            &self.owner_id,
            self.daemon.as_ref(),
            art,
        ));
    }

    pub fn close_artifact_preview(&mut self) {
        self.preview_artifact = None;
    }

    pub fn toggle_step_group(&mut self, job_id: JobId, group_idx: usize) {
        let key = (job_id, group_idx);
        if !self.collapsed_step_groups.remove(&key) {
            self.collapsed_step_groups.insert(key);
        }
    }

    pub fn engine_mut(&mut self) -> &mut AfterlifeEngine {
        Arc::make_mut(&mut self.engine)
    }

    pub fn reload_from_daemon(&mut self) {
        if let Some(daemon) = &self.daemon
            && let Ok(snap) = daemon.ports.store.load_workspace(&self.owner_id)
        {
            let arrived =
                diff_newly_entered_approval_gates(&self.approval_gate_snapshot, &snap.engine);
            self.approval_gate_snapshot = snapshot_approval_gates(&snap.engine);
            self.engine = Arc::new(snap.engine);
            self.sync_approval_badge();
            if self.approval_notifications_enabled {
                for target in &arrived {
                    deliver_approval_notification(target);
                }
            }
        }
    }

    pub fn sync_approval_badge(&self) {
        sync_dock_badge(
            self.engine.approval_jobs().len(),
            self.approval_notifications_enabled,
        );
    }

    pub fn toggle_approval_notifications(&mut self) {
        self.approval_notifications_enabled = !self.approval_notifications_enabled;
        self.sync_approval_badge();
        self.persist_appearance();
    }

    pub fn dispatch(&mut self, command: EngineCommand) -> Result<Option<JobId>, String> {
        let extract_job = |events: &[JobEvent]| -> Option<JobId> {
            events.iter().find_map(|e| match e {
                JobEvent::JobQueued { job_id, .. } | JobEvent::JobTransitioned { job_id, .. } => {
                    Some(job_id.clone())
                }
                _ => None,
            })
        };

        if let Some(daemon) = &self.daemon {
            let seq = UI_COMMAND_SEQ.fetch_add(1, Ordering::Relaxed);
            let command_id = CommandId::new(format!("ui-{}-{seq}", now_ms()));
            let envelope = CommandEnvelope {
                command_id: command_id.clone(),
                actor: self.human_actor.clone(),
                command,
            };
            match daemon.ports.store.execute_command(envelope) {
                Ok(outcome) => {
                    if !outcome.idempotent_replay {
                        for (idx, ev) in outcome.events.iter().enumerate() {
                            let rec = EventRecord {
                                seq: outcome.seq_start.saturating_add(idx as u64),
                                owner_id: self.owner_id.clone(),
                                command_id: command_id.clone(),
                                event: ev.clone(),
                            };
                            let _ = daemon.ports.telemetry.emit(&rec);
                            daemon.ports.event_bus.publish(rec);
                        }
                    }
                    let affected_job = extract_job(&outcome.events);
                    if let Ok(snap) = daemon.ports.store.load_workspace(&self.owner_id) {
                        self.approval_gate_snapshot = snapshot_approval_gates(&snap.engine);
                        self.engine = Arc::new(snap.engine);
                        self.sync_approval_badge();
                    }
                    return Ok(affected_job);
                }
                Err(e) => {
                    eprintln!("afterlife-ui: store command error ({e})");
                    return Err(e.to_string());
                }
            }
        }

        let actor = self.human_actor.clone();
        let events = self
            .engine_mut()
            .apply(&actor, command)
            .map_err(|e| e.to_string())?;
        self.approval_gate_snapshot = snapshot_approval_gates(&self.engine);
        self.sync_approval_badge();
        Ok(extract_job(&events))
    }

    pub fn sync_active_display(
        &mut self,
        display_uuid: Option<String>,
        scale_factor: f32,
    ) -> bool {
        let safe_scale = if scale_factor.is_finite() && scale_factor > 0.0 {
            scale_factor
        } else {
            2.0
        };
        let normalized_uuid = display_uuid
            .map(|u| u.trim().to_string())
            .filter(|u| !u.is_empty());
        let resolved = resolve_tile_size_for_display(
            self.base_tile_size_px,
            normalized_uuid.as_deref(),
            safe_scale,
            &self.display_tile_sizes,
        );
        let changed = self.active_display_uuid != normalized_uuid
            || (self.active_display_scale - safe_scale).abs() > 1e-3
            || (self.glass.bg_tile_size_px - resolved).abs() > 1e-4;
        self.active_display_uuid = normalized_uuid;
        self.active_display_scale = safe_scale;
        if (self.glass.bg_tile_size_px - resolved).abs() > 1e-4 {
            self.glass.bg_tile_size_px = resolved;
            self.refresh_palette();
        }
        changed
    }

    pub fn set_glass_param(&mut self, key: GlassParamKey, value: f32) {
        if key == GlassParamKey::InactiveFadeSecs {
            self.fade_ui_after_secs = if value.is_finite() {
                value.clamp(0.0, 60.0).round()
            } else {
                0.0
            };
            return;
        }
        if key == GlassParamKey::BgTileSizePx {
            let clamped = if value.is_finite() {
                value.clamp(2.0, 24.0).round()
            } else {
                self.glass.bg_tile_size_px.round().clamp(2.0, 24.0)
            };
            self.glass.bg_tile_size_px = clamped;
            if let Some(ref uuid) = self.active_display_uuid {
                self.display_tile_sizes.insert(uuid.clone(), clamped);
            }
            if self.active_display_scale >= 1.5 || self.active_display_uuid.is_none() {
                self.base_tile_size_px = clamped;
            }
        } else {
            key.set(&mut self.glass, value);
        }
        self.refresh_palette();
    }

    pub fn inactive_fade_delay_secs(&self) -> Option<u64> {
        let secs = self.fade_ui_after_secs.round().clamp(0.0, 60.0) as u64;
        (secs > 0).then_some(secs)
    }

    pub fn reset_glass_settings(&mut self, preset: GlassSettings) {
        self.glass = preset;
        self.base_tile_size_px = preset.bg_tile_size_px;
        self.display_tile_sizes.clear();
        let resolved = resolve_tile_size_for_display(
            self.base_tile_size_px,
            self.active_display_uuid.as_deref(),
            self.active_display_scale,
            &self.display_tile_sizes,
        );
        self.glass.bg_tile_size_px = resolved;
        self.refresh_palette();
    }

    pub fn refresh_palette(&mut self) {
        self.palette = Palette::from_style(self.style, &self.glass);
        set_particle_wave_tuning(self.glass.particle_wave_tuning());
        crate::platform::apple_music::sync_watcher_enabled(
            self.glass.bg_apple_music_artwork_enabled,
        );
        crate::platform::youtube_mirror::sync_receiver_enabled(
            self.glass.bg_youtube_mirror_enabled,
        );
    }

    pub fn reload_backdrop_tuning(&mut self) -> bool {
        let Some(ref store) = self.backdrop_store else {
            return false;
        };
        let Ok(cfg) = store.load() else {
            return false;
        };
        let before = (
            self.glass,
            self.pause_animation_when_inactive,
            self.fade_ui_after_secs,
            self.approval_notifications_enabled,
            self.base_tile_size_px,
            self.display_tile_sizes.clone(),
        );
        apply_backdrop_config(
            &mut self.glass,
            &mut self.pause_animation_when_inactive,
            &mut self.fade_ui_after_secs,
            &mut self.approval_notifications_enabled,
            &cfg,
        );
        self.base_tile_size_px = cfg.tuning.clamped().tile_size_px;
        self.display_tile_sizes = sanitize_display_tile_sizes(cfg.display_tile_sizes);
        self.glass.bg_tile_size_px = resolve_tile_size_for_display(
            self.base_tile_size_px,
            self.active_display_uuid.as_deref(),
            self.active_display_scale,
            &self.display_tile_sizes,
        );
        let after = (
            self.glass,
            self.pause_animation_when_inactive,
            self.fade_ui_after_secs,
            self.approval_notifications_enabled,
            self.base_tile_size_px,
            self.display_tile_sizes.clone(),
        );
        if before != after {
            self.refresh_palette();
            self.sync_approval_badge();
            true
        } else {
            false
        }
    }

    pub fn persist_appearance(&self) {
        let Some(ref store) = self.backdrop_store else {
            return;
        };
        let mut tuning = snapshot_tuning(&self.glass);
        tuning.tile_size_px = self.base_tile_size_px.clamp(2.0, 24.0);
        if let Err(e) = store.save_appearance_with_displays(
            tuning,
            snapshot_glass_appearance(
                &self.glass,
                self.pause_animation_when_inactive,
                self.fade_ui_after_secs,
                self.approval_notifications_enabled,
            ),
            self.display_tile_sizes.clone(),
        ) {
            eprintln!("afterlife-ui: failed to save appearance config ({e})");
        }
    }

    pub fn toggle_style(&mut self) {
        self.style = match self.style {
            UiStyle::LiquidGlass => UiStyle::Wireframe,
            UiStyle::Wireframe => UiStyle::LiquidGlass,
        };
        self.refresh_palette();
    }

    pub fn select_tab(&mut self, tab: TabId) {
        if tab != TabId::Guard {
            self.previous_tab = tab;
        }
        self.active_tab = tab;
    }

    pub fn toggle_guard_view(&mut self) {
        if self.active_tab == TabId::Guard {
            self.active_tab = if self.previous_tab == TabId::Guard {
                TabId::Workspace
            } else {
                self.previous_tab
            };
        } else {
            self.previous_tab = self.active_tab;
            self.active_tab = TabId::Guard;
        }
    }

    pub fn toggle_appearance_drawer(&mut self) {
        self.settings_open = !self.settings_open;
        if self.settings_open {
            self.inspector_open = false;
        }
    }

    pub fn toggle_inspector_popover(&mut self) {
        self.inspector_open = !self.inspector_open;
        if self.inspector_open {
            self.settings_open = false;
        }
    }

    pub fn glass_opt(&self) -> Option<&GlassSettings> {
        (self.style == UiStyle::LiquidGlass).then_some(&self.glass)
    }

    pub fn selected_job(&self) -> Option<&Job> {
        self.engine
            .jobs
            .iter()
            .find(|j| j.id == self.selected_job_id)
            .or_else(|| self.engine.jobs.first())
    }
}

pub struct SharedAppState {
    pub state: Entity<AppState>,
    pub perf: Option<PerfSession>,
}

impl gpui::Global for SharedAppState {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_displays_applies_scale_and_preserves_per_display_tile_memory() {
        let mut st = AppState::new_ephemeral();
        st.display_tile_sizes.clear();
        st.base_tile_size_px = 3.0;
        st.glass.bg_tile_size_px = 3.0;

        st.sync_active_display(Some("retina-builtin".to_string()), 2.0);
        assert_eq!(st.glass.bg_tile_size_px, 3.0);

        st.sync_active_display(Some("external-lg-4k".to_string()), 1.0);
        assert_eq!(st.glass.bg_tile_size_px, 2.0);

        st.set_glass_param(GlassParamKey::BgTileSizePx, 3.8);
        assert_eq!(st.glass.bg_tile_size_px, 4.0);
        assert_eq!(st.base_tile_size_px, 3.0);
        assert_eq!(st.display_tile_sizes.get("external-lg-4k"), Some(&4.0));

        st.sync_active_display(Some("retina-builtin".to_string()), 2.0);
        assert_eq!(st.glass.bg_tile_size_px, 3.0);

        st.sync_active_display(Some("external-lg-4k".to_string()), 1.0);
        assert_eq!(st.glass.bg_tile_size_px, 4.0);
    }

    #[test]
    fn inspector_toggles_guard_tab_and_exclusivity_with_appearance() {
        let mut st = AppState::new_ephemeral();
        assert!(!st.inspector_open);
        assert!(!st.settings_open);
        assert!(st.active_tab == TabId::Workspace);

        st.toggle_appearance_drawer();
        assert!(st.settings_open);
        assert!(!st.inspector_open);

        st.toggle_inspector_popover();
        assert!(st.inspector_open);
        assert!(!st.settings_open);

        st.select_tab(TabId::Templates);
        assert!(st.active_tab == TabId::Templates);

        st.toggle_guard_view();
        assert!(st.active_tab == TabId::Guard);

        st.toggle_guard_view();
        assert!(st.active_tab == TabId::Templates);

        st.toggle_guard_view();
        assert!(st.active_tab == TabId::Guard);
        st.select_tab(TabId::Workspace);
        assert!(st.active_tab == TabId::Workspace);

        assert_eq!(st.inactive_fade_delay_secs(), None);
        assert_eq!(GlassParamKey::InactiveFadeSecs.format_value(0.0), "Off");
        st.set_glass_param(GlassParamKey::InactiveFadeSecs, 12.6);
        assert_eq!(st.inactive_fade_delay_secs(), Some(13));
        assert_eq!(GlassParamKey::InactiveFadeSecs.format_value(13.0), "13s");
        st.set_glass_param(GlassParamKey::InactiveFadeSecs, 999.0);
        assert_eq!(st.inactive_fade_delay_secs(), Some(60));
        st.set_glass_param(GlassParamKey::InactiveFadeSecs, -5.0);
        assert_eq!(st.inactive_fade_delay_secs(), None);
    }
}
