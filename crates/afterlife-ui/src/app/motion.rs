use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use afterlife_core::{Job, JobId, ProposalId};
use afterlife_design::UiStyle;
use gpui::Rgba;

use crate::app::state::{AppState, TabId};

pub const DRAWER_OPEN_MS: u64 = 280;
pub const DRAWER_CLOSE_MS: u64 = 220;
pub const DETAIL_OPEN_MS: u64 = 320;
pub const DETAIL_CLOSE_MS: u64 = 240;
pub const DETAIL_SWAP_MS: u64 = 240;
pub const TAB_ENTER_MS: u64 = 260;
pub const SELECTION_MS: u64 = 280;
pub const TOGGLE_MS: u64 = 220;
pub const PULSE_MS: u64 = 300;
pub const INACTIVE_FADE_IN_MS: u64 = 1400;
pub const INACTIVE_FADE_OUT_MS: u64 = 260;
pub const INACTIVE_UI_MIN_OPACITY: f32 = 0.24;

pub fn ease_out_cubic(t: f32) -> f32 {
    let clamped = t.clamp(0.0, 1.0);
    1.0 - (1.0 - clamped).powi(3)
}

pub fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

pub fn lerp_rgba(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let u = t.clamp(0.0, 1.0);
    Rgba {
        r: a.r + (b.r - a.r) * u,
        g: a.g + (b.g - a.g) * u,
        b: a.b + (b.b - a.b) * u,
        a: a.a + (b.a - a.a) * u,
    }
}

pub fn lerp_glass_rgba(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let u = t.clamp(0.0, 1.0);
    let out_a = a.a + (b.a - a.a) * u;
    if out_a <= 1e-4 {
        return Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        };
    }
    let ra = a.r * a.a;
    let ga = a.g * a.a;
    let ba = a.b * a.a;
    let rb = b.r * b.a;
    let gb = b.g * b.a;
    let bb = b.b * b.a;
    Rgba {
        r: ((ra + (rb - ra) * u) / out_a).clamp(0.0, 1.0),
        g: ((ga + (gb - ga) * u) / out_a).clamp(0.0, 1.0),
        b: ((ba + (bb - ba) * u) / out_a).clamp(0.0, 1.0),
        a: out_a.clamp(0.0, 1.0),
    }
}

#[derive(Clone, Debug)]
pub struct ScalarTransition {
    from: f32,
    to: f32,
    started_at: Instant,
    duration: Duration,
}

impl ScalarTransition {
    pub fn settled(initial: f32, now: Instant) -> Self {
        Self {
            from: initial,
            to: initial,
            started_at: now,
            duration: Duration::ZERO,
        }
    }

    pub fn set_target(&mut self, target: f32, duration: Duration, now: Instant) {
        if (self.to - target).abs() <= 1e-4 {
            return;
        }
        let current = self.value(now);
        self.from = current;
        self.to = target;
        self.started_at = now;
        self.duration = duration;
    }

    pub fn value(&self, now: Instant) -> f32 {
        if self.duration.is_zero() || (self.to - self.from).abs() <= 1e-4 {
            return self.to;
        }
        let raw = now.saturating_duration_since(self.started_at).as_secs_f32()
            / self.duration.as_secs_f32();
        if raw >= 1.0 {
            return self.to;
        }
        lerp_f32(self.from, self.to, ease_out_cubic(raw))
    }

    pub fn is_active(&self, now: Instant) -> bool {
        !self.duration.is_zero()
            && (self.to - self.from).abs() > 1e-4
            && now.saturating_duration_since(self.started_at) < self.duration
    }
}

pub struct MotionSnapshot {
    pub drawer_progress: f32,
    pub inspector_progress: f32,
    pub detail_progress: f32,
    pub detail_swap_progress: f32,
    pub detail_job: Option<Job>,
    pub detail_proposal_id: Option<ProposalId>,
    pub tab_progress: f32,
    pub toggle_apple_music_progress: f32,
    pub toggle_youtube_mirror_progress: f32,
    pub toggle_backdrop_progress: f32,
    pub toggle_pause_progress: f32,
    pub toggle_liquid_glass_progress: f32,
    pub toggle_centerline_progress: f32,
    pub toggle_px_guard_progress: f32,
    pub inactive_ui_opacity: f32,
    pub selection_weights: HashMap<JobId, f32>,
    pub pulses: HashMap<String, f32>,
    pub animating: bool,
}

impl MotionSnapshot {
    pub fn selection_weight(&self, job_id: &JobId, selected_now: bool) -> f32 {
        self.selection_weights
            .get(job_id)
            .copied()
            .unwrap_or(if selected_now { 1.0 } else { 0.0 })
    }

    pub fn pulse_strength(&self, key: &str) -> f32 {
        self.pulses.get(key).copied().unwrap_or(0.0)
    }
}

pub struct UiMotion {
    drawer: ScalarTransition,
    inspector: ScalarTransition,
    detail: ScalarTransition,
    last_detail_job: Option<Job>,
    last_detail_job_id: Option<JobId>,
    last_detail_proposal_id: Option<ProposalId>,
    detail_swap_started: Option<Instant>,
    last_tab: TabId,
    tab_enter_started: Option<Instant>,
    toggle_apple_music: ScalarTransition,
    toggle_youtube_mirror: ScalarTransition,
    toggle_backdrop: ScalarTransition,
    toggle_pause: ScalarTransition,
    toggle_liquid_glass: ScalarTransition,
    toggle_centerline: ScalarTransition,
    toggle_px_guard: ScalarTransition,
    inactive_ui_opacity: ScalarTransition,
    selection_weights: HashMap<JobId, ScalarTransition>,
    pulses: HashMap<String, Instant>,
    initialized: bool,
}

impl Default for UiMotion {
    fn default() -> Self {
        Self::new()
    }
}

impl UiMotion {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            drawer: ScalarTransition::settled(0.0, now),
            inspector: ScalarTransition::settled(0.0, now),
            detail: ScalarTransition::settled(0.0, now),
            last_detail_job: None,
            last_detail_job_id: None,
            last_detail_proposal_id: None,
            detail_swap_started: None,
            last_tab: TabId::Workspace,
            tab_enter_started: None,
            toggle_apple_music: ScalarTransition::settled(1.0, now),
            toggle_youtube_mirror: ScalarTransition::settled(1.0, now),
            toggle_backdrop: ScalarTransition::settled(0.0, now),
            toggle_pause: ScalarTransition::settled(1.0, now),
            toggle_liquid_glass: ScalarTransition::settled(1.0, now),
            toggle_centerline: ScalarTransition::settled(0.0, now),
            toggle_px_guard: ScalarTransition::settled(0.0, now),
            inactive_ui_opacity: ScalarTransition::settled(1.0, now),
            selection_weights: HashMap::new(),
            pulses: HashMap::new(),
            initialized: false,
        }
    }

    pub fn trigger_pulse(&mut self, key: impl Into<String>) {
        self.pulses.insert(key.into(), Instant::now());
    }

    pub fn sync(
        &mut self,
        st: &AppState,
        inactive_dim_active: bool,
        now: Instant,
    ) -> MotionSnapshot {
        let selected_proposal = st
            .detail_open
            .then(|| st.selected_proposal_id.clone())
            .flatten();
        let selected_job = (st.detail_open && selected_proposal.is_none())
            .then(|| st.selected_job().cloned())
            .flatten();
        let selected_id = selected_job.as_ref().map(|j| j.id.clone());
        let should_dim_ui = st.inactive_fade_delay_secs().is_some() && inactive_dim_active;

        if !self.initialized {
            self.drawer = ScalarTransition::settled(if st.settings_open { 1.0 } else { 0.0 }, now);
            self.inspector =
                ScalarTransition::settled(if st.inspector_open { 1.0 } else { 0.0 }, now);
            self.detail = ScalarTransition::settled(
                if selected_proposal.is_some() || selected_job.is_some() {
                    1.0
                } else {
                    0.0
                },
                now,
            );
            self.last_detail_job = selected_job.clone();
            self.last_detail_job_id = selected_id.clone();
            self.last_detail_proposal_id = selected_proposal.clone();
            self.last_tab = st.active_tab;
            self.toggle_apple_music = ScalarTransition::settled(
                if st.glass.bg_apple_music_artwork_enabled {
                    1.0
                } else {
                    0.0
                },
                now,
            );
            self.toggle_youtube_mirror = ScalarTransition::settled(
                if st.glass.bg_youtube_mirror_enabled {
                    1.0
                } else {
                    0.0
                },
                now,
            );
            self.toggle_backdrop =
                ScalarTransition::settled(if st.glass.bg_image_enabled { 1.0 } else { 0.0 }, now);
            self.toggle_pause = ScalarTransition::settled(
                if st.pause_animation_when_inactive {
                    1.0
                } else {
                    0.0
                },
                now,
            );
            self.toggle_liquid_glass = ScalarTransition::settled(
                if st.style == UiStyle::LiquidGlass {
                    1.0
                } else {
                    0.0
                },
                now,
            );
            self.toggle_centerline =
                ScalarTransition::settled(if st.show_centerline { 1.0 } else { 0.0 }, now);
            self.toggle_px_guard = ScalarTransition::settled(
                if st.active_tab == TabId::Guard {
                    1.0
                } else {
                    0.0
                },
                now,
            );
            self.inactive_ui_opacity = ScalarTransition::settled(
                if should_dim_ui {
                    INACTIVE_UI_MIN_OPACITY
                } else {
                    1.0
                },
                now,
            );
            if let Some(id) = &selected_id {
                self.selection_weights
                    .insert(id.clone(), ScalarTransition::settled(1.0, now));
            }
            self.initialized = true;
        } else {
            let drawer_target = if st.settings_open { 1.0 } else { 0.0 };
            let drawer_ms = if st.settings_open {
                DRAWER_OPEN_MS
            } else {
                DRAWER_CLOSE_MS
            };
            self.drawer
                .set_target(drawer_target, Duration::from_millis(drawer_ms), now);

            let inspector_target = if st.inspector_open { 1.0 } else { 0.0 };
            let inspector_ms = if st.inspector_open {
                DRAWER_OPEN_MS
            } else {
                DRAWER_CLOSE_MS
            };
            self.inspector
                .set_target(inspector_target, Duration::from_millis(inspector_ms), now);

            let detail_visible = selected_proposal.is_some() || selected_job.is_some();
            let detail_target = if detail_visible { 1.0 } else { 0.0 };
            let detail_ms = if detail_visible {
                DETAIL_OPEN_MS
            } else {
                DETAIL_CLOSE_MS
            };
            let was_open = self.detail.value(now) > 0.05;
            self.detail
                .set_target(detail_target, Duration::from_millis(detail_ms), now);

            if let Some(pid) = selected_proposal {
                if was_open && self.last_detail_proposal_id.as_ref() != Some(&pid) {
                    self.detail_swap_started = Some(now);
                }
                self.last_detail_proposal_id = Some(pid);
                self.last_detail_job = None;
                self.last_detail_job_id = None;
            } else if let Some(job) = selected_job {
                if was_open
                    && (self.last_detail_proposal_id.is_some()
                        || self.last_detail_job_id.as_ref() != Some(&job.id))
                {
                    self.detail_swap_started = Some(now);
                }
                self.last_detail_proposal_id = None;
                self.last_detail_job_id = Some(job.id.clone());
                self.last_detail_job = Some(job);
            } else if self.detail.value(now) <= 0.005 {
                self.last_detail_job = None;
                self.last_detail_job_id = None;
                self.last_detail_proposal_id = None;
            }

            if self.last_tab != st.active_tab {
                self.last_tab = st.active_tab;
                self.tab_enter_started = Some(now);
            }

            self.toggle_apple_music.set_target(
                if st.glass.bg_apple_music_artwork_enabled {
                    1.0
                } else {
                    0.0
                },
                Duration::from_millis(TOGGLE_MS),
                now,
            );
            self.toggle_youtube_mirror.set_target(
                if st.glass.bg_youtube_mirror_enabled {
                    1.0
                } else {
                    0.0
                },
                Duration::from_millis(TOGGLE_MS),
                now,
            );
            self.toggle_backdrop.set_target(
                if st.glass.bg_image_enabled { 1.0 } else { 0.0 },
                Duration::from_millis(TOGGLE_MS),
                now,
            );
            self.toggle_pause.set_target(
                if st.pause_animation_when_inactive {
                    1.0
                } else {
                    0.0
                },
                Duration::from_millis(TOGGLE_MS),
                now,
            );
            self.toggle_liquid_glass.set_target(
                if st.style == UiStyle::LiquidGlass {
                    1.0
                } else {
                    0.0
                },
                Duration::from_millis(TOGGLE_MS),
                now,
            );
            self.toggle_centerline.set_target(
                if st.show_centerline { 1.0 } else { 0.0 },
                Duration::from_millis(TOGGLE_MS),
                now,
            );
            self.toggle_px_guard.set_target(
                if st.active_tab == TabId::Guard {
                    1.0
                } else {
                    0.0
                },
                Duration::from_millis(TOGGLE_MS),
                now,
            );
            let (inactive_target, inactive_ms) = if should_dim_ui {
                (INACTIVE_UI_MIN_OPACITY, INACTIVE_FADE_IN_MS)
            } else {
                (1.0, INACTIVE_FADE_OUT_MS)
            };
            self.inactive_ui_opacity.set_target(
                inactive_target,
                Duration::from_millis(inactive_ms),
                now,
            );

            let sel_dur = Duration::from_millis(SELECTION_MS);
            if let Some(id) = &selected_id {
                self.selection_weights
                    .entry(id.clone())
                    .or_insert_with(|| ScalarTransition::settled(0.0, now))
                    .set_target(1.0, sel_dur, now);
            }
            for (id, tr) in &mut self.selection_weights {
                if selected_id.as_ref() != Some(id) {
                    tr.set_target(0.0, sel_dur, now);
                }
            }
            self.selection_weights
                .retain(|id, tr| selected_id.as_ref() == Some(id) || tr.value(now) > 0.002);
        }

        let detail_swap_progress = match self.detail_swap_started {
            Some(started) => {
                let raw = now.saturating_duration_since(started).as_secs_f32()
                    / Duration::from_millis(DETAIL_SWAP_MS).as_secs_f32();
                if raw >= 1.0 {
                    self.detail_swap_started = None;
                    1.0
                } else {
                    ease_out_cubic(raw)
                }
            }
            None => 1.0,
        };

        let tab_progress = match self.tab_enter_started {
            Some(started) => {
                let raw = now.saturating_duration_since(started).as_secs_f32()
                    / Duration::from_millis(TAB_ENTER_MS).as_secs_f32();
                if raw >= 1.0 {
                    self.tab_enter_started = None;
                    1.0
                } else {
                    ease_out_cubic(raw)
                }
            }
            None => 1.0,
        };

        let pulse_dur = Duration::from_millis(PULSE_MS).as_secs_f32();
        let mut active_pulses = HashMap::new();
        self.pulses.retain(|key, started| {
            let raw = now.saturating_duration_since(*started).as_secs_f32() / pulse_dur;
            if raw >= 1.0 {
                false
            } else {
                let decay = (1.0 - raw.clamp(0.0, 1.0)).powi(2);
                active_pulses.insert(key.clone(), decay);
                true
            }
        });

        let mut selection_weights = HashMap::with_capacity(self.selection_weights.len());
        let mut any_selection_active = false;
        for (id, tr) in &self.selection_weights {
            selection_weights.insert(id.clone(), tr.value(now));
            if tr.is_active(now) {
                any_selection_active = true;
            }
        }

        let animating = self.drawer.is_active(now)
            || self.inspector.is_active(now)
            || self.detail.is_active(now)
            || self.detail_swap_started.is_some()
            || self.tab_enter_started.is_some()
            || self.toggle_apple_music.is_active(now)
            || self.toggle_youtube_mirror.is_active(now)
            || self.toggle_backdrop.is_active(now)
            || self.toggle_pause.is_active(now)
            || self.toggle_liquid_glass.is_active(now)
            || self.toggle_centerline.is_active(now)
            || self.toggle_px_guard.is_active(now)
            || self.inactive_ui_opacity.is_active(now)
            || any_selection_active
            || !active_pulses.is_empty();

        MotionSnapshot {
            drawer_progress: self.drawer.value(now),
            inspector_progress: self.inspector.value(now),
            detail_progress: self.detail.value(now),
            detail_swap_progress,
            detail_job: self.last_detail_job.clone(),
            detail_proposal_id: self.last_detail_proposal_id.clone(),
            tab_progress,
            toggle_apple_music_progress: self.toggle_apple_music.value(now),
            toggle_youtube_mirror_progress: self.toggle_youtube_mirror.value(now),
            toggle_backdrop_progress: self.toggle_backdrop.value(now),
            toggle_pause_progress: self.toggle_pause.value(now),
            toggle_liquid_glass_progress: self.toggle_liquid_glass.value(now),
            toggle_centerline_progress: self.toggle_centerline.value(now),
            toggle_px_guard_progress: self.toggle_px_guard.value(now),
            inactive_ui_opacity: self.inactive_ui_opacity.value(now),
            selection_weights,
            pulses: active_pulses,
            animating,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_transition_interpolates_and_reverses_without_snapping() {
        let t0 = Instant::now();
        let mut tr = ScalarTransition::settled(0.0, t0);
        assert!(!tr.is_active(t0));

        tr.set_target(1.0, Duration::from_millis(280), t0);
        assert!(tr.is_active(t0 + Duration::from_millis(90)));
        let mid = tr.value(t0 + Duration::from_millis(90));
        assert!(mid > 0.5 && mid < 1.0);

        tr.set_target(
            0.0,
            Duration::from_millis(220),
            t0 + Duration::from_millis(90),
        );
        let rev_start = tr.value(t0 + Duration::from_millis(90));
        assert!((rev_start - mid).abs() < 1e-4);

        let rev_end = tr.value(t0 + Duration::from_millis(320));
        assert_eq!(rev_end, 0.0);
        assert!(!tr.is_active(t0 + Duration::from_millis(320)));
    }

    #[test]
    fn premultiplied_glass_lerp_stays_monotonic_without_white_spike() {
        let card = Rgba {
            r: 0.886,
            g: 0.910,
            b: 0.941,
            a: 0.15,
        };
        let selected = Rgba {
            r: 0.176,
            g: 0.345,
            b: 0.400,
            a: 0.40,
        };
        let mid = lerp_glass_rgba(card, selected, 0.5);
        let premul_r_card = card.r * card.a;
        let premul_r_mid = mid.r * mid.a;
        let premul_r_sel = selected.r * selected.a;
        assert!(premul_r_mid <= premul_r_card + 1e-5);
        assert!(premul_r_mid >= premul_r_sel - 1e-5);
    }

    #[test]
    fn inactive_fade_transitions_smoothly_and_preserves_visibility_floor() {
        let mut st = AppState::new();
        st.fade_ui_after_secs = 0.0;
        let t0 = Instant::now();
        let mut motion = UiMotion::new();

        let snap0 = motion.sync(&st, true, t0);
        assert_eq!(snap0.inactive_ui_opacity, 1.0);
        assert!(!snap0.animating);

        st.fade_ui_after_secs = 10.0;
        let t1 = t0 + Duration::from_millis(10);
        let snap1 = motion.sync(&st, true, t1);
        assert!(snap1.animating);

        let t_mid = t1 + Duration::from_millis(INACTIVE_FADE_IN_MS / 2);
        let snap_mid = motion.sync(&st, true, t_mid);
        assert!(snap_mid.inactive_ui_opacity < 0.8);
        assert!(snap_mid.inactive_ui_opacity > INACTIVE_UI_MIN_OPACITY);
        assert!(snap_mid.animating);

        let t_settled = t1 + Duration::from_millis(INACTIVE_FADE_IN_MS + 50);
        let snap_settled = motion.sync(&st, true, t_settled);
        assert!((snap_settled.inactive_ui_opacity - INACTIVE_UI_MIN_OPACITY).abs() < 1e-4);
        assert!(snap_settled.inactive_ui_opacity > 0.15);
        assert!(!snap_settled.animating);

        let t_wake = t_settled + Duration::from_millis(10);
        let _ = motion.sync(&st, false, t_wake);
        let snap_waking = motion.sync(&st, false, t_wake + Duration::from_millis(100));
        assert!(snap_waking.inactive_ui_opacity > INACTIVE_UI_MIN_OPACITY);
        assert!(snap_waking.inactive_ui_opacity < 1.0);
        assert!(snap_waking.animating);

        let snap_restored = motion.sync(
            &st,
            false,
            t_wake + Duration::from_millis(INACTIVE_FADE_OUT_MS + 50),
        );
        assert_eq!(snap_restored.inactive_ui_opacity, 1.0);
        assert!(!snap_restored.animating);
    }
}
