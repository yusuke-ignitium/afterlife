use afterlife_design::{
    FontPx, GlassSurface, RadiusPx, RowHeightPx, SpacePx, UiStyle, single_line, with_alpha,
};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, MouseButton, ParentElement, SharedString,
    Styled, div, px,
};

use crate::{
    app::{
        motion::MotionSnapshot,
        state::{AppState, TabId},
    },
    views::{
        appearance_drawer::{render_text_action, render_toggle_row},
        root::AfterlifeApp,
    },
};

const INSPECTOR_WIDTH_PX: f32 = 268.0;

pub fn render_inspector_popover(
    st: &AppState,
    motion: &MotionSnapshot,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;

    GlassSurface::popover(&p, &st.glass)
        .build()
        .w(px(INSPECTOR_WIDTH_PX))
        .overflow_hidden()
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |_, _, cx| {
            cx.stop_propagation();
        })
        .child(
            div()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .gap(SpacePx::S8.px())
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(single_line(
                            RowHeightPx::H22,
                            SpacePx::S0,
                            SpacePx::S0,
                            FontPx::F13,
                            p.radius(RadiusPx::R2),
                            None,
                            None,
                            p.text_primary,
                            st.show_centerline,
                            p.centerline,
                            [SharedString::from("Inspector").into_any_element()],
                        ))
                        .child(render_text_action(
                            st,
                            motion,
                            "inspector-close",
                            "Close",
                            false,
                            |st| {
                                st.inspector_open = false;
                            },
                            cx,
                        )),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(1.0))
                        .bg(with_alpha(p.text_primary, 0.08)),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(SpacePx::S8.px())
                .child(render_toggle_row(
                    st,
                    "toggle-inspector-liquid-glass",
                    "Liquid Glass",
                    st.style == UiStyle::LiquidGlass,
                    motion.toggle_liquid_glass_progress,
                    |st| st.toggle_style(),
                    cx,
                ))
                .child(render_toggle_row(
                    st,
                    "toggle-inspector-centerline",
                    "Centerline",
                    st.show_centerline,
                    motion.toggle_centerline_progress,
                    |st| st.show_centerline = !st.show_centerline,
                    cx,
                ))
                .child(render_toggle_row(
                    st,
                    "toggle-inspector-px-guard",
                    "Px & Align Guard",
                    st.active_tab == TabId::Guard,
                    motion.toggle_px_guard_progress,
                    |st| st.toggle_guard_view(),
                    cx,
                )),
        )
        .into_any_element()
}
