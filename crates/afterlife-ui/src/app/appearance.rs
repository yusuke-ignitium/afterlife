use afterlife_design::GlassSettings;
use afterlife_protocol::{BackdropConfig, BackdropGlassAppearance, BackdropTuning};

use crate::app::state::AppState;

pub(crate) fn apply_backdrop_config(
    glass: &mut GlassSettings,
    pause_animation_when_inactive: &mut bool,
    fade_ui_after_secs: &mut f32,
    fade_ui_transparency_pct: &mut f32,
    approval_notifications_enabled: &mut bool,
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
        *fade_ui_after_secs = saved.fade_ui_after_secs;
        *fade_ui_transparency_pct = saved.fade_ui_transparency_pct;
        *approval_notifications_enabled = saved.approval_notifications_enabled;
    }
}

pub(crate) fn snapshot_tuning(glass: &GlassSettings) -> BackdropTuning {
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

pub(crate) fn snapshot_glass_appearance(
    glass: &GlassSettings,
    pause_animation_when_inactive: bool,
    fade_ui_after_secs: f32,
    fade_ui_transparency_pct: f32,
    approval_notifications_enabled: bool,
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
        fade_ui_after_secs,
        fade_ui_transparency_pct,
        approval_notifications_enabled,
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
            Self::InactiveFadeSecs => ("Fade UI After", 0.0, 60.0, 0),
            Self::InactiveFadeTransparencyPct => ("Fade Transparency", 0.0, 90.0, 0),
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
            let secs = val.round().clamp(0.0, 60.0) as u32;
            return if secs == 0 {
                "Off".to_string()
            } else {
                format!("{secs}s")
            };
        }
        if self == Self::InactiveFadeTransparencyPct {
            let pct = val.round().clamp(0.0, 90.0) as u32;
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
            snapshot_glass_appearance(
                &glass_def,
                false,
                0.0,
                afterlife_protocol::DEFAULT_FADE_UI_TRANSPARENCY_PCT,
                true,
            ),
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
            glass: Some(snapshot_glass_appearance(
                &original, true, 15.0, 55.0, false,
            )),
            display_tile_sizes: BTreeMap::new(),
        };

        let mut restored = GlassSettings::default();
        let mut pause_inactive = false;
        let mut fade_after_secs = 0.0;
        let mut fade_transparency_pct = 76.0;
        let mut approval_notifications = true;
        apply_backdrop_config(
            &mut restored,
            &mut pause_inactive,
            &mut fade_after_secs,
            &mut fade_transparency_pct,
            &mut approval_notifications,
            &cfg,
        );

        assert_eq!(restored, original);
        assert!(pause_inactive);
        assert_eq!(fade_after_secs, 15.0);
        assert_eq!(fade_transparency_pct, 55.0);
        assert!(!approval_notifications);
    }
}
