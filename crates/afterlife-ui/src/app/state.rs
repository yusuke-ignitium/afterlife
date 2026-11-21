use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use afterlife_core::{
    Actor, AfterlifeEngine, CommandId, DistrictId, EngineCommand, Job, JobEvent, JobId, OwnerId,
    StageId, SubMachineTemplateId, TimestampMs,
};
use afterlife_daemon::AppDaemonState;
use afterlife_design::{GlassSettings, Palette, UiStyle};
use afterlife_protocol::{
    AfterlifeConfig, BackdropConfig, BackdropGlassAppearance, BackdropStore, BackdropTuning,
    CommandEnvelope, EventRecord, resolve_tile_size_for_display, sanitize_display_tile_sizes,
};
use gpui::{Entity, Image, ImageFormat, set_particle_wave_tuning};

use crate::perf::PerfSession;

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

fn apply_backdrop_config(
    glass: &mut GlassSettings,
    pause_animation_when_inactive: &mut bool,
    cfg: &BackdropConfig,
) {
    let tuning = cfg.tuning.clamped();
    glass.bg_image_enabled = tuning.image_enabled;
    glass.bg_apple_music_artwork_enabled = tuning.apple_music_artwork_enabled;
    glass.bg_youtube_mirror_enabled = tuning.youtube_mirror_enabled;
    glass.bg_brightness = tuning.master_brightness;
    glass.bg_tile_size_px = tuning.tile_size_px;
    glass.bg_image_hold_secs = tuning.image_hold_secs;
    glass.bg_image_brightness = tuning.image_brightness;
    glass.bg_peak_luminance = tuning.image_peak_luminance;
    glass.bg_image_saturation = tuning.image_saturation;
    if let Some(saved) = cfg.glass.map(BackdropGlassAppearance::clamped) {
        glass.disp_scale = saved.disp_scale;
        glass.rim_thickness = saved.rim_thickness;
        glass.refract_reach = saved.refract_reach;
        glass.chroma = saved.chroma;
        glass.highlight_alpha = saved.highlight_alpha;
        glass.rim_width = saved.rim_width;
        glass.blur_std_dev = saved.blur_std_dev;
        glass.bg_alpha = saved.bg_alpha;
        glass.radius = saved.radius;
        glass.tint_rgb = saved.tint_rgb;
        *pause_animation_when_inactive = saved.pause_animation_when_inactive;
    }
}

fn snapshot_tuning(glass: &GlassSettings) -> BackdropTuning {
    BackdropTuning {
        image_enabled: glass.bg_image_enabled,
        apple_music_artwork_enabled: glass.bg_apple_music_artwork_enabled,
        youtube_mirror_enabled: glass.bg_youtube_mirror_enabled,
        tile_size_px: glass.bg_tile_size_px,
        image_hold_secs: glass.bg_image_hold_secs,
        image_brightness: glass.bg_image_brightness,
        image_peak_luminance: glass.bg_peak_luminance,
        image_saturation: glass.bg_image_saturation,
        master_brightness: glass.bg_brightness,
    }
    .clamped()
}

fn snapshot_glass_appearance(
    glass: &GlassSettings,
    pause_animation_when_inactive: bool,
) -> BackdropGlassAppearance {
    BackdropGlassAppearance {
        disp_scale: glass.disp_scale,
        rim_thickness: glass.rim_thickness,
        refract_reach: glass.refract_reach,
        chroma: glass.chroma,
        highlight_alpha: glass.highlight_alpha,
        rim_width: glass.rim_width,
        blur_std_dev: glass.blur_std_dev,
        bg_alpha: glass.bg_alpha,
        radius: glass.radius,
        tint_rgb: glass.tint_rgb,
        pause_animation_when_inactive,
    }
}

const APP_TITLE_LOGO_PNG: &[u8] = include_bytes!("../../assets/afterlife-logo.png");

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TabId {
    Workspace,
    Templates,
    Guard,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GlassParamKey {
    BgBrightness,
    BgTileSizePx,
    BgImageHoldSecs,
    BgImageBrightness,
    BgPeakLuminance,
    BgImageSaturation,
    DispScale,
    RimThickness,
    RefractReach,
    Chroma,
    HighlightAlpha,
    RimWidth,
    BlurStdDev,
    BgAlpha,
    Radius,
}

impl GlassParamKey {
    pub const ALL: [GlassParamKey; 15] = [
        Self::BgBrightness,
        Self::BgTileSizePx,
        Self::BgImageHoldSecs,
        Self::BgImageBrightness,
        Self::BgPeakLuminance,
        Self::BgImageSaturation,
        Self::DispScale,
        Self::RimThickness,
        Self::RefractReach,
        Self::Chroma,
        Self::HighlightAlpha,
        Self::RimWidth,
        Self::BlurStdDev,
        Self::BgAlpha,
        Self::Radius,
    ];

    pub fn is_backdrop_param(self) -> bool {
        matches!(
            self,
            Self::BgBrightness
                | Self::BgTileSizePx
                | Self::BgImageHoldSecs
                | Self::BgImageBrightness
                | Self::BgPeakLuminance
                | Self::BgImageSaturation
        )
    }

    pub fn is_dimmed(self, glass: &GlassSettings) -> bool {
        match self {
            Self::BgImageHoldSecs => !glass.bg_image_enabled,
            Self::BgImageBrightness | Self::BgPeakLuminance | Self::BgImageSaturation => {
                !glass.bg_image_enabled
                    && !glass.bg_apple_music_artwork_enabled
                    && !glass.bg_youtube_mirror_enabled
            }
            _ => false,
        }
    }

    pub fn spec(self) -> (&'static str, f32, f32, usize) {
        match self {
            Self::BgBrightness => ("Brightness", 0.2, 2.0, 2),
            Self::BgTileSizePx => ("Tile Size", 2.0, 24.0, 0),
            Self::BgImageHoldSecs => ("Image Hold (s)", 0.5, 60.0, 1),
            Self::BgImageBrightness => ("Image Brightness", 0.00, 2.00, 2),
            Self::BgPeakLuminance => ("Peak Luminance", 0.00, 1.00, 2),
            Self::BgImageSaturation => ("Saturation", 0.00, 1.50, 2),
            Self::DispScale => ("Refraction", 0.0, 800.0, 0),
            Self::RimThickness => ("Edge Thickness", 0.01, 0.5, 3),
            Self::RefractReach => ("Refraction Depth", 1.0, 600.0, 0),
            Self::Chroma => ("Chromatic Shift", 0.0, 1.0, 2),
            Self::HighlightAlpha => ("Specular", 0.0, 1.0, 2),
            Self::RimWidth => ("Rim Width", 0.2, 8.0, 2),
            Self::BlurStdDev => ("Frost Blur", 0.0, 20.0, 1),
            Self::BgAlpha => ("Glass Opacity", 0.0, 0.85, 2),
            Self::Radius => ("Corner Radius", 0.0, 20.0, 1),
        }
    }

    pub fn get(self, g: &GlassSettings) -> f32 {
        match self {
            Self::BgBrightness => g.bg_brightness,
            Self::BgTileSizePx => g.bg_tile_size_px,
            Self::BgImageHoldSecs => g.bg_image_hold_secs,
            Self::BgImageBrightness => g.bg_image_brightness,
            Self::BgPeakLuminance => g.bg_peak_luminance,
            Self::BgImageSaturation => g.bg_image_saturation,
            Self::DispScale => g.disp_scale,
            Self::RimThickness => g.rim_thickness,
            Self::RefractReach => g.refract_reach,
            Self::Chroma => g.chroma,
            Self::HighlightAlpha => g.highlight_alpha,
            Self::RimWidth => g.rim_width,
            Self::BlurStdDev => g.blur_std_dev,
            Self::BgAlpha => g.bg_alpha,
            Self::Radius => g.radius,
        }
    }

    pub fn set(self, g: &mut GlassSettings, val: f32) {
        let (_, min, max, _) = self.spec();
        let v = val.clamp(min, max);
        match self {
            Self::BgBrightness => g.bg_brightness = v,
            Self::BgTileSizePx => g.bg_tile_size_px = v,
            Self::BgImageHoldSecs => g.bg_image_hold_secs = v,
            Self::BgImageBrightness => g.bg_image_brightness = v,
            Self::BgPeakLuminance => g.bg_peak_luminance = v,
            Self::BgImageSaturation => g.bg_image_saturation = v,
            Self::DispScale => g.disp_scale = v,
            Self::RimThickness => g.rim_thickness = v,
            Self::RefractReach => g.refract_reach = v,
            Self::Chroma => g.chroma = v,
            Self::HighlightAlpha => g.highlight_alpha = v,
            Self::RimWidth => g.rim_width = v,
            Self::BlurStdDev => g.blur_std_dev = v,
            Self::BgAlpha => g.bg_alpha = v,
            Self::Radius => g.radius = v,
        }
    }
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
    pub detail_open: bool,
    pub detail_height: f32,
    pub active_tab: TabId,
    pub show_centerline: bool,
    pub style: UiStyle,
    pub glass: GlassSettings,
    pub base_tile_size_px: f32,
    pub display_tile_sizes: BTreeMap<String, f32>,
    pub active_display_uuid: Option<String>,
    pub active_display_scale: f32,
    pub palette: Palette,
    pub settings_open: bool,
    pub pause_animation_when_inactive: bool,
}

impl AppState {
    pub fn new() -> Self {
        let ephemeral = std::env::var("AFTERLIFE_EPHEMERAL").is_ok()
            || std::env::var("AFTERLIFE_PERF_SCENARIO").is_ok();
        let perf_use_live_data = std::env::var("AFTERLIFE_PERF_USE_LIVE_DATA").is_ok();
        let backdrop_store = (!ephemeral).then(BackdropStore::open_default);

        let style = UiStyle::LiquidGlass;
        let mut glass = GlassSettings::default();
        let mut base_tile_size_px = glass.bg_tile_size_px;
        let mut display_tile_sizes = BTreeMap::new();
        let mut pause_animation_when_inactive = false;
        let load_backdrop_store = backdrop_store
            .clone()
            .or_else(|| perf_use_live_data.then(BackdropStore::open_default));
        if let Some(ref store) = load_backdrop_store {
            match store.load() {
                Ok(backdrop) => {
                    apply_backdrop_config(
                        &mut glass,
                        &mut pause_animation_when_inactive,
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
            .unwrap_or_else(|| JobId::from("JOB-041"));

        Self {
            engine: Arc::new(engine),
            daemon,
            backdrop_store,
            owner_id,
            human_actor,
            app_title_logo,
            selected_job_id,
            detail_open: false,
            detail_height: crate::app::window_state::DEFAULT_DETAIL_HEIGHT_PX,
            active_tab: TabId::Workspace,
            show_centerline: false,
            style,
            glass,
            base_tile_size_px,
            display_tile_sizes,
            active_display_uuid: None,
            active_display_scale: 2.0,
            palette,
            settings_open: false,
            pause_animation_when_inactive,
        }
    }

    pub fn engine_mut(&mut self) -> &mut AfterlifeEngine {
        Arc::make_mut(&mut self.engine)
    }

    pub fn reload_from_daemon(&mut self) {
        if let Some(daemon) = &self.daemon
            && let Ok(snap) = daemon.ports.store.load_workspace(&self.owner_id)
        {
            self.engine = Arc::new(snap.engine);
        }
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
                        self.engine = Arc::new(snap.engine);
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
        Ok(extract_job(&events))
    }

    pub fn next_sub_template_for_stage(
        &self,
        district_id: &DistrictId,
        stage_id: &StageId,
    ) -> Option<Option<SubMachineTemplateId>> {
        let district = self.engine.find_district(district_id)?;
        let stage = district
            .active_spec()
            .stages
            .iter()
            .find(|s| &s.id == stage_id)?;
        let current = stage.sub_template_id.as_ref();
        let ids: Vec<SubMachineTemplateId> = self.engine.templates.keys().cloned().collect();
        let next = match current {
            None => ids.first().cloned(),
            Some(cur) => {
                let pos = ids.iter().position(|id| id == cur);
                match pos {
                    Some(i) if i + 1 < ids.len() => Some(ids[i + 1].clone()),
                    _ => None,
                }
            }
        };
        Some(next)
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
            self.base_tile_size_px,
            self.display_tile_sizes.clone(),
        );
        apply_backdrop_config(
            &mut self.glass,
            &mut self.pause_animation_when_inactive,
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
            self.base_tile_size_px,
            self.display_tile_sizes.clone(),
        );
        if before != after {
            self.refresh_palette();
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
            snapshot_glass_appearance(&self.glass, self.pause_animation_when_inactive),
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
    use gpui::ParticleWaveTuning;

    use super::*;

    #[test]
    fn default_appearance_tokens_stay_in_sync_across_crates() {
        let glass_def = GlassSettings::default();
        let tuning_def = BackdropTuning::default();
        let saved_glass_def = BackdropGlassAppearance::default();
        let wave_def = ParticleWaveTuning::default();

        assert_eq!(snapshot_tuning(&glass_def), tuning_def);
        assert_eq!(
            snapshot_glass_appearance(&glass_def, false),
            saved_glass_def
        );
        assert_eq!(glass_def.particle_wave_tuning(), wave_def);
    }

    #[test]
    fn backdrop_config_round_trip_preserves_every_appearance_field() {
        let original = GlassSettings {
            bg_image_enabled: true,
            bg_apple_music_artwork_enabled: false,
            bg_youtube_mirror_enabled: false,
            bg_brightness: 1.25,
            bg_tile_size_px: 10.0,
            bg_image_hold_secs: 24.0,
            bg_image_brightness: 0.72,
            bg_peak_luminance: 0.68,
            bg_image_saturation: 0.91,
            disp_scale: 240.0,
            rim_thickness: 0.12,
            refract_reach: 180.0,
            chroma: 0.35,
            highlight_alpha: 0.42,
            rim_width: 2.4,
            blur_std_dev: 6.5,
            bg_alpha: 0.31,
            radius: 11.2,
            tint_rgb: [27, 45, 56],
        };

        let cfg = BackdropConfig {
            images: Vec::new(),
            tuning: snapshot_tuning(&original),
            glass: Some(snapshot_glass_appearance(&original, true)),
            display_tile_sizes: BTreeMap::new(),
        };

        let mut restored = GlassSettings::default();
        let mut pause_inactive = false;
        apply_backdrop_config(&mut restored, &mut pause_inactive, &cfg);

        assert_eq!(restored, original);
        assert!(pause_inactive);
    }

    #[test]
    fn switching_displays_applies_scale_and_preserves_per_display_tile_memory() {
        let mut st = AppState::new();
        st.backdrop_store = None;
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
}
