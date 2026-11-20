use gpui::{ParticleWaveTuning, Rgba};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeAccent {
    Neutral,
    Selected,
    Gate,
}

impl EdgeAccent {
    pub const fn shader_value(self) -> f32 {
        match self {
            Self::Neutral => 0.0,
            Self::Selected => 0.5,
            Self::Gate => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlassSettings {
    pub bg_image_enabled: bool,
    pub bg_apple_music_artwork_enabled: bool,
    pub bg_brightness: f32,
    pub bg_tile_size_px: f32,
    pub bg_image_hold_secs: f32,
    pub bg_image_brightness: f32,
    pub bg_peak_luminance: f32,
    pub bg_image_saturation: f32,
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
}

impl Default for GlassSettings {
    fn default() -> Self {
        Self {
            bg_image_enabled: false,
            bg_apple_music_artwork_enabled: true,
            bg_brightness: 1.5,
            bg_tile_size_px: 6.0,
            bg_image_hold_secs: 10.0,
            bg_image_brightness: 0.40,
            bg_peak_luminance: 0.55,
            bg_image_saturation: 0.64,
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
        }
    }
}

impl GlassSettings {
    pub fn overworld_exact_defaults() -> Self {
        Self {
            bg_image_enabled: false,
            bg_apple_music_artwork_enabled: true,
            bg_brightness: 1.0,
            bg_tile_size_px: 6.0,
            bg_image_hold_secs: 10.0,
            bg_image_brightness: 0.40,
            bg_peak_luminance: 0.55,
            bg_image_saturation: 0.64,
            disp_scale: 111.5,
            rim_thickness: 0.0956,
            refract_reach: 392.9,
            chroma: 0.0,
            highlight_alpha: 0.1556,
            rim_width: 1.275,
            blur_std_dev: 0.0,
            bg_alpha: 0.757,
            radius: 2.7,
            tint_rgb: [10, 10, 10],
        }
    }

    pub fn particle_wave_tuning(&self) -> ParticleWaveTuning {
        ParticleWaveTuning {
            image_enabled: self.bg_image_enabled,
            apple_music_artwork_enabled: self.bg_apple_music_artwork_enabled,
            tile_size_px: self.bg_tile_size_px,
            image_hold_secs: self.bg_image_hold_secs,
            image_brightness: self.bg_image_brightness,
            image_peak_luminance: self.bg_peak_luminance,
            image_saturation: self.bg_image_saturation,
        }
        .clamped()
    }

    pub fn reach_px(&self) -> f32 {
        (self.rim_thickness * self.refract_reach).max(0.5)
    }

    pub fn tint_rgba(&self, alpha_scale: f32) -> Rgba {
        Rgba {
            r: self.tint_rgb[0] as f32 / 255.0,
            g: self.tint_rgb[1] as f32 / 255.0,
            b: self.tint_rgb[2] as f32 / 255.0,
            a: (self.bg_alpha * alpha_scale).clamp(0.0, 1.0),
        }
    }
}
