mod store;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use store::{BackdropStore, default_backdrop_data_dir};

pub const DEFAULT_IMAGE_ENABLED: bool = false;
pub const DEFAULT_APPLE_MUSIC_ARTWORK_ENABLED: bool = true;
pub const DEFAULT_YOUTUBE_MIRROR_ENABLED: bool = true;
pub const DEFAULT_TILE_SIZE_PX: f32 = 6.0;
pub const DEFAULT_IMAGE_HOLD_SECS: f32 = 10.0;
pub const DEFAULT_IMAGE_BRIGHTNESS: f32 = 0.40;
pub const DEFAULT_IMAGE_PEAK_LUMINANCE: f32 = 0.55;
pub const DEFAULT_IMAGE_SATURATION: f32 = 0.64;
pub const DEFAULT_MASTER_BRIGHTNESS: f32 = 1.50;
pub const DEFAULT_FADE_UI_TRANSPARENCY_PCT: f32 = 76.0;
pub const MAX_FADE_UI_AFTER_SECS: f32 = 120.0;
pub const MAX_FADE_UI_TRANSPARENCY_PCT: f32 = 100.0;


fn finite_clamp(val: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if val.is_finite() {
        val.clamp(min, max)
    } else {
        fallback
    }
}

pub fn sanitize_fade_ui_after_secs(val: f32) -> f32 {
    finite_clamp(val, 0.0, MAX_FADE_UI_AFTER_SECS, 0.0).round()
}

pub fn sanitize_fade_ui_transparency_pct(val: f32) -> f32 {
    finite_clamp(
        val,
        0.0,
        MAX_FADE_UI_TRANSPARENCY_PCT,
        DEFAULT_FADE_UI_TRANSPARENCY_PCT,
    )
    .round()
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackdropTuning {
    pub image_enabled: bool,
    pub apple_music_artwork_enabled: bool,
    pub youtube_mirror_enabled: bool,
    pub tile_size_px: f32,
    pub image_hold_secs: f32,
    pub image_brightness: f32,
    pub image_peak_luminance: f32,
    pub image_saturation: f32,
    pub master_brightness: f32,
}

impl Default for BackdropTuning {
    fn default() -> Self {
        Self {
            image_enabled: DEFAULT_IMAGE_ENABLED,
            apple_music_artwork_enabled: DEFAULT_APPLE_MUSIC_ARTWORK_ENABLED,
            youtube_mirror_enabled: DEFAULT_YOUTUBE_MIRROR_ENABLED,
            tile_size_px: DEFAULT_TILE_SIZE_PX,
            image_hold_secs: DEFAULT_IMAGE_HOLD_SECS,
            image_brightness: DEFAULT_IMAGE_BRIGHTNESS,
            image_peak_luminance: DEFAULT_IMAGE_PEAK_LUMINANCE,
            image_saturation: DEFAULT_IMAGE_SATURATION,
            master_brightness: DEFAULT_MASTER_BRIGHTNESS,
        }
    }
}

impl BackdropTuning {
    pub fn clamped(self) -> Self {
        let def = Self::default();
        Self {
            image_enabled: self.image_enabled,
            apple_music_artwork_enabled: self.apple_music_artwork_enabled,
            youtube_mirror_enabled: self.youtube_mirror_enabled,
            tile_size_px: finite_clamp(self.tile_size_px, 2.0, 24.0, def.tile_size_px).round(),
            image_hold_secs: finite_clamp(self.image_hold_secs, 0.5, 600.0, def.image_hold_secs),
            image_brightness: finite_clamp(self.image_brightness, 0.0, 2.0, def.image_brightness),
            image_peak_luminance: finite_clamp(
                self.image_peak_luminance,
                0.0,
                1.0,
                def.image_peak_luminance,
            ),
            image_saturation: finite_clamp(self.image_saturation, 0.0, 1.5, def.image_saturation),
            master_brightness: finite_clamp(
                self.master_brightness,
                0.2,
                2.0,
                def.master_brightness,
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BackdropTuningPatch {
    pub image_enabled: Option<bool>,
    pub apple_music_artwork_enabled: Option<bool>,
    pub youtube_mirror_enabled: Option<bool>,
    pub tile_size_px: Option<f32>,
    pub image_hold_secs: Option<f32>,
    pub image_brightness: Option<f32>,
    pub image_peak_luminance: Option<f32>,
    pub image_saturation: Option<f32>,
    pub master_brightness: Option<f32>,
    pub reset: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackdropGlassAppearance {
    pub disp_scale: f32,
    pub rim_thickness: f32,
    pub refract_reach: f32,
    pub chroma: f32,
    pub highlight_alpha: f32,
    pub rim_width: f32,
    pub blur_std_dev: f32,
    pub bg_alpha: f32,
    pub radius: f32,
    pub tint_rgb: [u8; 3],
    pub pause_animation_when_inactive: bool,
    pub fade_ui_after_secs: f32,
    pub fade_ui_transparency_pct: f32,
    pub approval_notifications_enabled: bool,
    pub compact_districts: bool,
}

impl Default for BackdropGlassAppearance {
    fn default() -> Self {
        Self {
            disp_scale: 193.0,
            rim_thickness: 0.06,
            refract_reach: 50.0,
            chroma: 0.0,
            highlight_alpha: 0.24,
            rim_width: 1.3,
            blur_std_dev: 0.0,
            bg_alpha: 0.04,
            radius: 6.6,
            tint_rgb: [7, 9, 14],
            pause_animation_when_inactive: false,
            fade_ui_after_secs: 0.0,
            fade_ui_transparency_pct: DEFAULT_FADE_UI_TRANSPARENCY_PCT,
            approval_notifications_enabled: true,
            compact_districts: false,
        }
    }
}

impl BackdropGlassAppearance {
    pub fn clamped(self) -> Self {
        let def = Self::default();
        Self {
            disp_scale: finite_clamp(self.disp_scale, 0.0, 800.0, def.disp_scale),
            rim_thickness: finite_clamp(self.rim_thickness, 0.01, 0.5, def.rim_thickness),
            refract_reach: finite_clamp(self.refract_reach, 1.0, 600.0, def.refract_reach),
            chroma: finite_clamp(self.chroma, 0.0, 1.0, def.chroma),
            highlight_alpha: finite_clamp(self.highlight_alpha, 0.0, 1.0, def.highlight_alpha),
            rim_width: finite_clamp(self.rim_width, 0.2, 8.0, def.rim_width),
            blur_std_dev: finite_clamp(self.blur_std_dev, 0.0, 20.0, def.blur_std_dev),
            bg_alpha: finite_clamp(self.bg_alpha, 0.0, 0.85, def.bg_alpha),
            radius: finite_clamp(self.radius, 0.0, 20.0, def.radius),
            tint_rgb: self.tint_rgb,
            pause_animation_when_inactive: self.pause_animation_when_inactive,
            fade_ui_after_secs: sanitize_fade_ui_after_secs(self.fade_ui_after_secs),
            fade_ui_transparency_pct: sanitize_fade_ui_transparency_pct(
                self.fade_ui_transparency_pct,
            ),
            approval_notifications_enabled: self.approval_notifications_enabled,
            compact_districts: self.compact_districts,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackdropImageEntry {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackdropConfig {
    pub images: Vec<BackdropImageEntry>,
    pub tuning: BackdropTuning,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glass: Option<BackdropGlassAppearance>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub display_tile_sizes: BTreeMap<String, f32>,
}

pub fn sanitize_display_tile_sizes(map: BTreeMap<String, f32>) -> BTreeMap<String, f32> {
    map.into_iter()
        .filter_map(|(k, v)| {
            let trimmed = k.trim();
            if trimmed.is_empty() || !v.is_finite() {
                return None;
            }
            Some((trimmed.to_string(), v.clamp(2.0, 24.0).round()))
        })
        .collect()
}

pub fn resolve_tile_size_for_display(
    base_tile_size_px: f32,
    display_uuid: Option<&str>,
    scale_factor: f32,
    overrides: &BTreeMap<String, f32>,
) -> f32 {
    if let Some(uuid) = display_uuid.map(str::trim).filter(|s| !s.is_empty())
        && let Some(&saved) = overrides.get(uuid)
        && saved.is_finite()
    {
        return saved.clamp(2.0, 24.0).round();
    }
    let base = finite_clamp(base_tile_size_px, 2.0, 24.0, DEFAULT_TILE_SIZE_PX).round();
    let safe_scale = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        2.0
    };
    (base * (safe_scale / 2.0)).round().clamp(2.0, 24.0)
}

impl BackdropConfig {
    pub fn sanitized(mut self) -> Self {
        self.tuning = self.tuning.clamped();
        self.glass = self.glass.map(BackdropGlassAppearance::clamped);
        self.display_tile_sizes = sanitize_display_tile_sizes(self.display_tile_sizes);
        self
    }
}
