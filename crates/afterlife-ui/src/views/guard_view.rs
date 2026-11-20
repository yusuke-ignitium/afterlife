use afterlife_design::{
    ColorRole, ColorState, FontPx, GlassSurface, RadiusPx, RowHeightPx, SpacePx, single_line,
    single_line_ex, with_alpha,
};
use gpui::{AnyElement, IntoElement, ParentElement, SharedString, Styled, div, px};

use crate::app::state::AppState;

pub fn render_guard_tab(st: &AppState) -> AnyElement {
    let p = st.palette;
    let heights = [
        RowHeightPx::H22,
        RowHeightPx::H24,
        RowHeightPx::H28,
        RowHeightPx::H32,
        RowHeightPx::H36,
    ];

    div()
        .flex()
        .flex_row()
        .gap(SpacePx::S16.px())
        .child(
            GlassSurface::panel(&p, &st.glass)
                .gap(SpacePx::S8)
                .build()
                .flex_1()
                .child(single_line(
                    RowHeightPx::H28,
                    SpacePx::S0,
                    SpacePx::S4,
                    FontPx::F14,
                    p.radius(RadiusPx::R4),
                    None,
                    None,
                    p.text_primary,
                    st.show_centerline,
                    p.centerline,
                    [SharedString::from("1. Deterministic Px Scale & Glass Guard")
                        .into_any_element()],
                ))
                .child(
                    div()
                        .text_size(FontPx::F12.px())
                        .text_color(p.text_secondary)
                        .child("SpacePx / FontPx / RowHeightPx are closed enums; Glass uses Backdrop-Synced razor edges."),
                )
                .child(single_line_ex(
                    RowHeightPx::H24,
                    SpacePx::S8,
                    SpacePx::S0,
                    FontPx::F12,
                    p.radius_f32(&st.glass, 0.8),
                    Some(p.border_subtle),
                    Some(p.glass_card),
                    st.glass_opt(),
                    0.0,
                    p.text_primary,
                    st.show_centerline,
                    p.centerline,
                    [SharedString::from("SpacePx: 0, 4, 8, 12, 16, 24, 32").into_any_element()],
                ))
                .child(single_line_ex(
                    RowHeightPx::H24,
                    SpacePx::S8,
                    SpacePx::S0,
                    FontPx::F12,
                    p.radius_f32(&st.glass, 0.8),
                    Some(p.border_subtle),
                    Some(p.glass_card),
                    st.glass_opt(),
                    0.0,
                    p.text_primary,
                    st.show_centerline,
                    p.centerline,
                    [SharedString::from("FontPx: 11, 12, 13, 14, 16, 20").into_any_element()],
                ))
                .child(single_line_ex(
                    RowHeightPx::H24,
                    SpacePx::S8,
                    SpacePx::S0,
                    FontPx::F12,
                    p.radius_f32(&st.glass, 0.8),
                    Some(p.border_subtle),
                    Some(p.glass_card),
                    st.glass_opt(),
                    0.0,
                    p.text_primary,
                    st.show_centerline,
                    p.centerline,
                    [SharedString::from("RowHeightPx: 22, 24, 28, 32, 36").into_any_element()],
                ))
                .child(single_line_ex(
                    RowHeightPx::H24,
                    SpacePx::S8,
                    SpacePx::S0,
                    FontPx::F12,
                    p.radius_f32(&st.glass, 0.8),
                    Some(p.border_subtle),
                    Some(p.glass_card),
                    st.glass_opt(),
                    0.0,
                    p.text_primary,
                    st.show_centerline,
                    p.centerline,
                    [SharedString::from(
                        "Colors: 8 Roles x (Active / Inactive) = 16 Tokens (Abyssal Slate)",
                    )
                    .into_any_element()],
                ))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(SpacePx::S8.px())
                        .children(
                            [
                                ("Neutral", ColorRole::Neutral),
                                ("Platinum", ColorRole::Platinum),
                                ("IceCyan", ColorRole::IceCyan),
                                ("Cobalt", ColorRole::Cobalt),
                                ("Indigo", ColorRole::Indigo),
                                ("Mint", ColorRole::Mint),
                                ("Champagne", ColorRole::Champagne),
                                ("Coral", ColorRole::Coral),
                            ]
                            .into_iter()
                            .map(|(name, role)| {
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(SpacePx::S4.px())
                                    .px(SpacePx::S8.px())
                                    .h(RowHeightPx::H24.px())
                                    .rounded(p.radius_f32(&st.glass, 0.6))
                                    .bg(with_alpha(role.rgba(ColorState::Inactive), 0.22))
                                    .child(
                                        div()
                                            .w(px(10.0))
                                            .h(px(10.0))
                                            .rounded(px(2.0))
                                            .bg(role.rgba(ColorState::Active)),
                                    )
                                    .child(
                                        div()
                                            .w(px(10.0))
                                            .h(px(10.0))
                                            .rounded(px(2.0))
                                            .bg(role.rgba(ColorState::Inactive)),
                                    )
                                    .child(
                                        div()
                                            .text_size(FontPx::F11.px())
                                            .font_family("SF Mono")
                                            .text_color(role.rgba(ColorState::Active))
                                            .child(name),
                                    )
                            }),
                        ),
                ),
        )
        .child(
            GlassSurface::panel(&p, &st.glass)
                .gap(SpacePx::S8)
                .build()
                .flex_1()
                .child(single_line(
                    RowHeightPx::H28,
                    SpacePx::S0,
                    SpacePx::S4,
                    FontPx::F14,
                    p.radius(RadiusPx::R4),
                    None,
                    None,
                    p.text_primary,
                    st.show_centerline,
                    p.centerline,
                    [SharedString::from("2. Single-Line Vertical Center Proof").into_any_element()],
                ))
                .children(heights.into_iter().map(|h| {
                    single_line_ex(
                        h,
                        SpacePx::S12,
                        SpacePx::S8,
                        if h.val() >= 32.0 {
                            FontPx::F14
                        } else if h.val() >= 28.0 {
                            FontPx::F13
                        } else {
                            FontPx::F12
                        },
                        p.radius_f32(&st.glass, 0.8),
                        Some(p.border_rim),
                        Some(p.glass_card),
                        st.glass_opt(),
                        0.0,
                        p.text_primary,
                        true,
                        p.accent,
                        [SharedString::from(format!(
                            "Row Height {}px (cap-height centered)",
                            h.val() as u32
                        ))
                        .into_any_element()],
                    )
                })),
        )
        .into_any_element()
}
