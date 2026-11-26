use afterlife_design::GlassSettings;
use afterlife_protocol::{
    BackdropConfig, BackdropGlassAppearance, BackdropTuning, MAX_FADE_UI_AFTER_SECS,
    MAX_FADE_UI_TRANSPARENCY_PCT, sanitize_fade_ui_after_secs, sanitize_fade_ui_transparency_pct,
};

use crate::app::state::AppState;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiPreferences {
    pub pause_animation_when_inactive: bool,
    pub fade_ui_after_secs: f32,
    pub fade_ui_transparency_pct: f32,
    pub approval_notifications_enabled: bool,
    pub compact_districts: bool,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            pause_animation_when_inactive: false,
            fade_ui_after_secs: 0.0,
            fade_ui_transparency_pct: afterlife_protocol::DEFAULT_FADE_UI_TRANSPARENCY_PCT,
            approval_notifications_enabled: true,
            compact_districts: false,
        }
    }
}

pub(crate) fn apply_backdrop_config(
    glass: &mut GlassSettings,
    prefs: &mut UiPreferences,
    cfg: &BackdropConfig,
) {
    let tuning = cfg.tuning.clamped();
    glass.bg_image_enabled = tuning.image_enabled;
    glass.bg_image_shuffle = tuning.image_shuffle;
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
        prefs.pause_animation_when_inactive = saved.pause_animation_when_inactive;
        prefs.fade_ui_after_secs = saved.fade_ui_after_secs;
        prefs.fade_ui_transparency_pct = saved.fade_ui_transparency_pct;
        prefs.approval_notifications_enabled = saved.approval_notifications_enabled;
        prefs.compact_districts = saved.compact_districts;
    }
}

pub(crate) fn snapshot_tuning(glass: &GlassSettings) -> BackdropTuning {
    BackdropTuning {
        image_enabled: glass.bg_image_enabled,
        image_shuffle: glass.bg_image_shuffle,
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

pub(crate) fn snapshot_glass_appearance(
    glass: &GlassSettings,
    prefs: UiPreferences,
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
        pause_animation_when_inactive: prefs.pause_animation_when_inactive,
        fade_ui_after_secs: prefs.fade_ui_after_secs,
        fade_ui_transparency_pct: prefs.fade_ui_transparency_pct,
        approval_notifications_enabled: prefs.approval_notifications_enabled,
        compact_districts: prefs.compact_districts,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GlassParamSection {
    Backdrop,
    Surface,
    Inactive,
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
    InactiveFadeSecs,
    InactiveFadeTransparencyPct,
}

impl GlassParamKey {
    pub const ALL: [GlassParamKey; 17] = [
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
        Self::InactiveFadeSecs,
        Self::InactiveFadeTransparencyPct,
    ];

    pub fn section(self) -> GlassParamSection {
        match self {
            Self::BgBrightness
            | Self::BgTileSizePx
            | Self::BgImageHoldSecs
            | Self::BgImageBrightness
            | Self::BgPeakLuminance
            | Self::BgImageSaturation => GlassParamSection::Backdrop,
            Self::InactiveFadeSecs | Self::InactiveFadeTransparencyPct => {
                GlassParamSection::Inactive
            }
            _ => GlassParamSection::Surface,
        }
    }

    pub fn is_dimmed(self, st: &AppState) -> bool {
        let glass = &st.glass;
        match self {
            Self::BgImageHoldSecs => !glass.bg_image_enabled,
            Self::BgImageBrightness | Self::BgPeakLuminance | Self::BgImageSaturation => {
                !glass.bg_image_enabled
                    && !glass.bg_apple_music_artwork_enabled
                    && !glass.bg_youtube_mirror_enabled
            }
            Self::InactiveFadeTransparencyPct => st.inactive_fade_delay_secs().is_none(),
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
            Self::InactiveFadeSecs => ("Fade UI After", 0.0, MAX_FADE_UI_AFTER_SECS, 0),
            Self::InactiveFadeTransparencyPct => {
                ("Fade Transparency", 0.0, MAX_FADE_UI_TRANSPARENCY_PCT, 0)
            }
        }
    }

    pub fn get(self, st: &AppState) -> f32 {
        let g = &st.glass;
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
            Self::InactiveFadeSecs => st.fade_ui_after_secs,
            Self::InactiveFadeTransparencyPct => st.fade_ui_transparency_pct,
        }
    }

    pub fn format_value(self, val: f32) -> String {
        if self == Self::InactiveFadeSecs {
            let secs = sanitize_fade_ui_after_secs(val) as u32;
            return if secs == 0 {
                "Off".to_string()
            } else {
                format!("{secs}s")
            };
        }
        if self == Self::InactiveFadeTransparencyPct {
            let pct = sanitize_fade_ui_transparency_pct(val) as u32;
            return format!("{pct}%");
        }
        let (_, _, _, dp) = self.spec();
        match dp {
            0 => format!("{:.0}", val),
            1 => format!("{:.1}", val),
            _ => format!("{:.2}", val),
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
            Self::InactiveFadeSecs | Self::InactiveFadeTransparencyPct => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

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
            snapshot_glass_appearance(&glass_def, UiPreferences::default()),
            saved_glass_def
        );
        assert_eq!(glass_def.particle_wave_tuning(), wave_def);
        assert_eq!(GlassParamKey::InactiveFadeSecs.spec().2, MAX_FADE_UI_AFTER_SECS);
        assert_eq!(
            GlassParamKey::InactiveFadeTransparencyPct.spec().2,
            MAX_FADE_UI_TRANSPARENCY_PCT
        );
    }

    #[test]
    fn backdrop_config_round_trip_preserves_every_appearance_field() {
        let original = GlassSettings {
            bg_image_enabled: true,
            bg_image_shuffle: false,
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
        let original_prefs = UiPreferences {
            pause_animation_when_inactive: true,
            fade_ui_after_secs: 15.0,
            fade_ui_transparency_pct: 55.0,
            approval_notifications_enabled: false,
            compact_districts: true,
        };

        let cfg = BackdropConfig {
            images: Vec::new(),
            tuning: snapshot_tuning(&original),
            glass: Some(snapshot_glass_appearance(&original, original_prefs)),
            display_tile_sizes: BTreeMap::new(),
        };

        let mut restored = GlassSettings::default();
        let mut restored_prefs = UiPreferences::default();
        apply_backdrop_config(&mut restored, &mut restored_prefs, &cfg);

        assert_eq!(restored, original);
        assert_eq!(restored_prefs, original_prefs);
    }

    #[test]
    fn app_state_persists_and_reloads_compact_districts_across_restart_and_glass_reset() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "afterlife_ui_compact_persist_{}_{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let store = afterlife_protocol::BackdropStore::open(&dir);

        let mut st = AppState::new_ephemeral();
        st.backdrop_store = Some(store.clone());
        assert!(!st.compact_districts);

        st.toggle_compact_districts();
        assert!(st.compact_districts);

        st.reset_glass_settings(GlassSettings::overworld_exact_defaults());
        st.persist_appearance();
        assert!(st.compact_districts);

        let mut restarted = AppState::new_ephemeral();
        restarted.backdrop_store = Some(store);
        assert!(!restarted.compact_districts);
        assert!(restarted.reload_backdrop_tuning());
        assert!(restarted.compact_districts);
        assert!(!restarted.reload_backdrop_tuning());

        restarted.toggle_compact_districts();
        assert!(!restarted.compact_districts);

        let mut restarted_off = AppState::new_ephemeral();
        restarted_off.backdrop_store = Some(afterlife_protocol::BackdropStore::open(&dir));
        restarted_off.compact_districts = true;
        assert!(restarted_off.reload_backdrop_tuning());
        assert!(!restarted_off.compact_districts);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
