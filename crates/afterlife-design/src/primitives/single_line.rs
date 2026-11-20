use gpui::{AnyElement, Div, ParentElement, Pixels, Rgba, Styled, div, liquid_glass_styled, px};

use crate::tokens::{BORDER_PX, FontPx, GlassSettings, RadiusPx, RowHeightPx, SpacePx};

#[allow(clippy::too_many_arguments)]
pub fn single_line(
    height: RowHeightPx,
    pad_x: SpacePx,
    gap: SpacePx,
    font: FontPx,
    radius: RadiusPx,
    border_color: Option<Rgba>,
    bg: Option<Rgba>,
    text_color: Rgba,
    show_centerline: bool,
    centerline_color: Rgba,
    children: impl IntoIterator<Item = AnyElement>,
) -> Div {
    single_line_ex(
        height,
        pad_x,
        gap,
        font,
        radius.px(),
        border_color,
        bg,
        None,
        0.0,
        text_color,
        show_centerline,
        centerline_color,
        children,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn single_line_ex(
    height: RowHeightPx,
    pad_x: SpacePx,
    gap: SpacePx,
    font: FontPx,
    radius_px: Pixels,
    border_color: Option<Rgba>,
    bg: Option<Rgba>,
    glass_opt: Option<&GlassSettings>,
    edge_tint_kind: f32,
    text_color: Rgba,
    show_centerline: bool,
    centerline_color: Rgba,
    children: impl IntoIterator<Item = AnyElement>,
) -> Div {
    let draw_flat_border = glass_opt.is_none() && border_color.is_some();
    let inner_h = if draw_flat_border {
        (height.val() - 2.0 * BORDER_PX).max(1.0)
    } else {
        height.val()
    };

    let mut d = div()
        .relative()
        .flex()
        .flex_row()
        .items_center()
        .h(height.px())
        .min_h(height.px())
        .max_h(height.px())
        .px(pad_x.px())
        .py(SpacePx::S0.px())
        .gap(gap.px())
        .text_size(font.px())
        .line_height(px(inner_h))
        .cap_center()
        .whitespace_nowrap()
        .overflow_hidden()
        .rounded(radius_px)
        .text_color(text_color)
        .font_family("SF Mono");

    if draw_flat_border && let Some(bc) = border_color {
        d = d.border(px(BORDER_PX)).border_color(bc);
    }
    if let Some(g) = glass_opt {
        let tint = bg.unwrap_or_else(|| g.tint_rgba(1.0));
        d = d.bg(liquid_glass_styled(
            tint,
            g.disp_scale * 0.75,
            g.reach_px().min(14.0),
            g.chroma,
            g.highlight_alpha,
            g.rim_width,
            g.blur_std_dev,
            7,
            edge_tint_kind,
        ));
    } else if let Some(b) = bg {
        d = d.bg(b);
    }

    for child in children {
        d = d.child(child);
    }

    if show_centerline {
        d = d.child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(inner_h * 0.5 - 0.5))
                .h(px(1.0))
                .bg(centerline_color),
        );
    }

    d
}
