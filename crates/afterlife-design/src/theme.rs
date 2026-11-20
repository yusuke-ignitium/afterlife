use gpui::{Pixels, Rgba, px, rgb};

use crate::tokens::{ColorRole, ColorState, GlassSettings, RadiusPx, with_alpha};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiStyle {
    LiquidGlass,
    Wireframe,
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub style: UiStyle,
    pub window_backdrop: Rgba,
    pub wave_a: Rgba,
    pub wave_b: Rgba,
    pub panel_base: Rgba,
    pub stage_base: Rgba,
    pub glass_card: Rgba,
    pub glass_popover: Rgba,
    pub glass_sheet: Rgba,
    pub glass_selected: Rgba,
    pub glass_gate: Rgba,
    pub border_rim: Rgba,
    pub border_subtle: Rgba,
    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub text_tertiary: Rgba,
    pub accent: Rgba,
    pub accent_inactive: Rgba,
    pub active_bg: Rgba,
    pub btn_bg: Rgba,
    pub text_on_accent: Rgba,
    pub amber: Rgba,
    pub amber_inactive: Rgba,
    pub purple: Rgba,
    pub purple_inactive: Rgba,
    pub green: Rgba,
    pub green_inactive: Rgba,
    pub coral: Rgba,
    pub coral_inactive: Rgba,
    pub centerline: Rgba,
}

impl Palette {
    pub fn from_style(style: UiStyle, glass: &GlassSettings) -> Self {
        match style {
            UiStyle::LiquidGlass => Self::abyssal_glass(glass),
            UiStyle::Wireframe => Self::dark_wireframe(),
        }
    }

    pub fn abyssal_glass(glass: &GlassSettings) -> Self {
        let wave_a = ColorRole::IceCyan.rgba(ColorState::Inactive);
        let wave_b = ColorRole::Cobalt.rgba(ColorState::Inactive);
        let cyan_inact = ColorRole::IceCyan.rgba(ColorState::Inactive);
        let champ_inact = ColorRole::Champagne.rgba(ColorState::Inactive);

        Self {
            style: UiStyle::LiquidGlass,
            window_backdrop: rgb(0x07090e),
            wave_a,
            wave_b,
            panel_base: Rgba {
                r: 7.0 / 255.0,
                g: 9.0 / 255.0,
                b: 14.0 / 255.0,
                a: 0.14,
            },
            stage_base: glass.tint_rgba(0.90),
            glass_card: with_alpha(
                ColorRole::Platinum.rgba(ColorState::Active),
                (glass.bg_alpha + 0.05).clamp(0.08, 0.35),
            ),
            glass_popover: with_alpha(
                ColorRole::Platinum.rgba(ColorState::Active),
                (glass.bg_alpha + 0.05).clamp(0.08, 0.35),
            ),
            glass_sheet: with_alpha(rgb(0x07090e), 0.48),
            glass_selected: with_alpha(cyan_inact, (glass.bg_alpha + 0.30).clamp(0.32, 0.80)),
            glass_gate: with_alpha(champ_inact, (glass.bg_alpha + 0.22).clamp(0.24, 0.72)),
            border_rim: with_alpha(ColorRole::Platinum.rgba(ColorState::Active), 0.22),
            border_subtle: with_alpha(ColorRole::Platinum.rgba(ColorState::Inactive), 0.35),
            text_primary: ColorRole::Neutral.rgba(ColorState::Active),
            text_secondary: ColorRole::Neutral.rgba(ColorState::Inactive),
            text_tertiary: ColorRole::Platinum.rgba(ColorState::Inactive),
            accent: ColorRole::IceCyan.rgba(ColorState::Active),
            accent_inactive: cyan_inact,
            active_bg: with_alpha(cyan_inact, (glass.bg_alpha + 0.28).clamp(0.28, 0.80)),
            btn_bg: glass.tint_rgba(1.2),
            text_on_accent: ColorRole::IceCyan.rgba(ColorState::Active),
            amber: ColorRole::Champagne.rgba(ColorState::Active),
            amber_inactive: champ_inact,
            purple: ColorRole::Indigo.rgba(ColorState::Active),
            purple_inactive: ColorRole::Indigo.rgba(ColorState::Inactive),
            green: ColorRole::Mint.rgba(ColorState::Active),
            green_inactive: ColorRole::Mint.rgba(ColorState::Inactive),
            coral: ColorRole::Coral.rgba(ColorState::Active),
            coral_inactive: ColorRole::Coral.rgba(ColorState::Inactive),
            centerline: ColorRole::Coral.rgba(ColorState::Active),
        }
    }

    pub fn dark_wireframe() -> Self {
        Self {
            style: UiStyle::Wireframe,
            window_backdrop: rgb(0x090a0d),
            wave_a: ColorRole::Platinum.rgba(ColorState::Inactive),
            wave_b: ColorRole::Platinum.rgba(ColorState::Inactive),
            panel_base: rgb(0x111318),
            stage_base: rgb(0x0c0e12),
            glass_card: rgb(0x161920),
            glass_popover: rgb(0x161920),
            glass_sheet: rgb(0x161920),
            glass_selected: rgb(0x1b2b34),
            glass_gate: rgb(0x282317),
            border_rim: ColorRole::Platinum.rgba(ColorState::Inactive),
            border_subtle: rgb(0x252a33),
            text_primary: ColorRole::Neutral.rgba(ColorState::Active),
            text_secondary: ColorRole::Neutral.rgba(ColorState::Inactive),
            text_tertiary: ColorRole::Platinum.rgba(ColorState::Inactive),
            accent: ColorRole::IceCyan.rgba(ColorState::Active),
            accent_inactive: ColorRole::IceCyan.rgba(ColorState::Inactive),
            active_bg: ColorRole::IceCyan.rgba(ColorState::Inactive),
            btn_bg: rgb(0x161920),
            text_on_accent: ColorRole::Neutral.rgba(ColorState::Active),
            amber: ColorRole::Champagne.rgba(ColorState::Active),
            amber_inactive: ColorRole::Champagne.rgba(ColorState::Inactive),
            purple: ColorRole::Indigo.rgba(ColorState::Active),
            purple_inactive: ColorRole::Indigo.rgba(ColorState::Inactive),
            green: ColorRole::Mint.rgba(ColorState::Active),
            green_inactive: ColorRole::Mint.rgba(ColorState::Inactive),
            coral: ColorRole::Coral.rgba(ColorState::Active),
            coral_inactive: ColorRole::Coral.rgba(ColorState::Inactive),
            centerline: ColorRole::Coral.rgba(ColorState::Active),
        }
    }

    pub fn radius(&self, glass_r: RadiusPx) -> RadiusPx {
        match self.style {
            UiStyle::LiquidGlass => glass_r,
            UiStyle::Wireframe => RadiusPx::R0,
        }
    }

    pub fn radius_f32(&self, glass: &GlassSettings, factor: f32) -> Pixels {
        match self.style {
            UiStyle::LiquidGlass => px((glass.radius * factor).max(0.0)),
            UiStyle::Wireframe => px(0.0),
        }
    }
}
