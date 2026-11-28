use gpui::{BoxShadow, Div, Rgba, Styled, div, liquid_glass_decoupled, point, px, rgba};

use crate::{
    theme::{Palette, UiStyle},
    tokens::{BORDER_PX, EdgeAccent, GlassSettings, SpacePx},
};

pub fn glass_shadows() -> Vec<BoxShadow> {
    vec![
        BoxShadow {
            color: rgba(0x00000033).into(),
            offset: point(px(0.0), px(6.0)),
            blur_radius: px(6.0),
            spread_radius: px(0.0),
        },
        BoxShadow {
            color: rgba(0x0000001a).into(),
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(20.0),
            spread_radius: px(0.0),
        },
    ]
}

pub fn sheet_shadows() -> Vec<BoxShadow> {
    vec![
        BoxShadow {
            color: rgba(0x0000004d).into(),
            offset: point(px(0.0), px(-6.0)),
            blur_radius: px(28.0),
            spread_radius: px(0.0),
        },
        BoxShadow {
            color: rgba(0x00000033).into(),
            offset: point(px(0.0), px(2.0)),
            blur_radius: px(10.0),
            spread_radius: px(0.0),
        },
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShadowPreset {
    Card,
    Sheet,
}

pub struct GlassSurface<'a> {
    palette: &'a Palette,
    glass: &'a GlassSettings,
    pad: SpacePx,
    gap: SpacePx,
    radius_factor: f32,
    border_color: Option<Rgba>,
    bg: Option<Rgba>,
    liquid_part: bool,
    accent: EdgeAccent,
    blur_floor: f32,
    disp_cap: Option<f32>,
    shadow_preset: ShadowPreset,
}

impl<'a> GlassSurface<'a> {
    pub fn panel(palette: &'a Palette, glass: &'a GlassSettings) -> Self {
        Self {
            palette,
            glass,
            pad: SpacePx::S12,
            gap: SpacePx::S12,
            radius_factor: 1.4,
            border_color: Some(palette.border_subtle),
            bg: Some(palette.panel_base),
            liquid_part: false,
            accent: EdgeAccent::Neutral,
            blur_floor: 0.0,
            disp_cap: None,
            shadow_preset: ShadowPreset::Card,
        }
    }

    pub fn card(palette: &'a Palette, glass: &'a GlassSettings) -> Self {
        Self {
            palette,
            glass,
            pad: SpacePx::S12,
            gap: SpacePx::S4,
            radius_factor: 1.0,
            border_color: Some(palette.border_rim),
            bg: Some(palette.glass_card),
            liquid_part: true,
            accent: EdgeAccent::Selected,
            blur_floor: 0.0,
            disp_cap: None,
            shadow_preset: ShadowPreset::Card,
        }
    }

    pub fn popover(palette: &'a Palette, glass: &'a GlassSettings) -> Self {
        Self {
            palette,
            glass,
            pad: SpacePx::S16,
            gap: SpacePx::S12,
            radius_factor: 1.2,
            border_color: Some(palette.border_rim),
            bg: Some(palette.glass_popover),
            liquid_part: true,
            accent: EdgeAccent::Neutral,
            blur_floor: 11.0,
            disp_cap: Some(115.0),
            shadow_preset: ShadowPreset::Card,
        }
    }

    pub fn sheet(palette: &'a Palette, glass: &'a GlassSettings) -> Self {
        Self {
            palette,
            glass,
            pad: SpacePx::S12,
            gap: SpacePx::S12,
            radius_factor: 1.2,
            border_color: Some(palette.border_rim),
            bg: Some(palette.glass_sheet),
            liquid_part: true,
            accent: EdgeAccent::Neutral,
            blur_floor: 14.0,
            disp_cap: Some(95.0),
            shadow_preset: ShadowPreset::Sheet,
        }
    }

    pub fn pad(mut self, pad: SpacePx) -> Self {
        self.pad = pad;
        self
    }

    pub fn gap(mut self, gap: SpacePx) -> Self {
        self.gap = gap;
        self
    }

    pub fn radius_factor(mut self, factor: f32) -> Self {
        self.radius_factor = factor;
        self
    }

    pub fn border(mut self, color: Option<Rgba>) -> Self {
        self.border_color = color;
        self
    }

    pub fn bg(mut self, color: Option<Rgba>) -> Self {
        self.bg = color;
        self
    }

    pub fn liquid(mut self, enabled: bool) -> Self {
        self.liquid_part = enabled;
        self
    }

    pub fn accent(mut self, accent: EdgeAccent) -> Self {
        self.accent = accent;
        self
    }

    pub fn build(self) -> Div {
        glass_box_ex(
            self.palette,
            self.glass,
            self.pad,
            self.gap,
            self.radius_factor,
            self.border_color,
            self.bg,
            self.liquid_part,
            self.accent.shader_value(),
            self.blur_floor,
            self.disp_cap,
            self.shadow_preset,
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub fn glass_box(
    palette: &Palette,
    glass: &GlassSettings,
    pad: SpacePx,
    gap: SpacePx,
    radius_factor: f32,
    border_color: Option<Rgba>,
    bg: Option<Rgba>,
    liquid_part: bool,
    edge_tint_kind: f32,
) -> Div {
    glass_box_ex(
        palette,
        glass,
        pad,
        gap,
        radius_factor,
        border_color,
        bg,
        liquid_part,
        edge_tint_kind,
        0.0,
        None,
        ShadowPreset::Card,
    )
}

#[allow(clippy::too_many_arguments)]
fn glass_box_ex(
    palette: &Palette,
    glass: &GlassSettings,
    pad: SpacePx,
    gap: SpacePx,
    radius_factor: f32,
    border_color: Option<Rgba>,
    bg: Option<Rgba>,
    liquid_part: bool,
    edge_tint_kind: f32,
    blur_floor: f32,
    disp_cap: Option<f32>,
    shadow_preset: ShadowPreset,
) -> Div {
    let r = palette.radius_f32(glass, radius_factor);
    let mut d = div()
        .relative()
        .flex()
        .flex_col()
        .p(pad.px())
        .gap(gap.px())
        .rounded(r);

    if palette.style == UiStyle::Wireframe
        && let Some(bc) = border_color
    {
        d = d.border(px(BORDER_PX)).border_color(bc);
    }

    if palette.style == UiStyle::LiquidGlass && liquid_part {
        let tint = bg.unwrap_or(palette.glass_card);
        let disp_scale = disp_cap.map_or(glass.disp_scale, |cap| glass.disp_scale.min(cap));
        let blur_std_dev = glass.blur_std_dev.max(blur_floor);
        let shadows = match shadow_preset {
            ShadowPreset::Card => glass_shadows(),
            ShadowPreset::Sheet => sheet_shadows(),
        };
        d = d
            .bg(liquid_glass_decoupled(
                tint,
                disp_scale,
                glass.pull_reach_px(),
                glass.edge_thickness_px(),
                glass.chroma,
                glass.highlight_alpha,
                glass.rim_width,
                blur_std_dev,
                edge_tint_kind,
            ))
            .shadow(shadows);
    } else if let Some(b) = bg {
        d = d.bg(b);
    }

    d
}
