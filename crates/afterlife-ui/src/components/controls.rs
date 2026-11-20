use afterlife_design::{
    FontPx, RadiusPx, RowHeightPx, SpacePx, single_line, single_line_ex, with_alpha,
};
use gpui::{
    AnyElement, Context, Div, InteractiveElement, IntoElement, ParentElement, Pixels, Rgba,
    SharedString, Stateful, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::{app::state::AppState, views::root::AfterlifeApp};

pub fn card_hover_sheen(group: SharedString, radius: Pixels, tint: Rgba) -> AnyElement {
    div()
        .absolute()
        .inset_0()
        .rounded(radius)
        .group_hover(group, move |s| s.bg(tint))
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
pub fn chip_btn(
    st: &AppState,
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    height: RowHeightPx,
    pad_x: SpacePx,
    font: FontPx,
    active: bool,
    on_click: impl Fn(&mut AppState, &mut Window, &mut Context<AppState>) + 'static,
    cx: &mut Context<AfterlifeApp>,
) -> Stateful<Div> {
    let p = st.palette;
    let border = if active { p.accent } else { p.border_subtle };
    let bg = if active { p.active_bg } else { p.btn_bg };
    let fg = if active {
        p.text_on_accent
    } else {
        p.text_primary
    };
    let tint_kind = if active { 0.5 } else { 0.0 };
    let radius = p.radius_f32(&st.glass, 0.8);
    let elem_id: gpui::ElementId = id.into();
    let group_name = SharedString::from(format!("chip-grp-{:?}", elem_id));

    single_line_ex(
        height,
        pad_x,
        SpacePx::S4,
        font,
        radius,
        Some(border),
        Some(bg),
        st.glass_opt(),
        tint_kind,
        fg,
        st.show_centerline,
        p.centerline,
        [label.into().into_any_element()],
    )
    .relative()
    .overflow_hidden()
    .id(elem_id)
    .group(group_name.clone())
    .cursor_pointer()
    .child(
        div()
            .absolute()
            .inset_0()
            .rounded(radius)
            .group_hover(group_name, move |s| {
                s.bg(with_alpha(p.accent_inactive, 0.24))
            }),
    )
    .on_click(cx.listener(move |this, _, window, cx| {
        this.state.update(cx, |st, scx| {
            on_click(st, window, scx);
        });
        cx.stop_propagation();
        cx.notify();
    }))
}

pub fn nav_item(
    st: &AppState,
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    pad_x: SpacePx,
    active: bool,
    on_click: impl Fn(&mut AppState, &mut Window, &mut Context<AppState>) + 'static,
    cx: &mut Context<AfterlifeApp>,
) -> impl IntoElement {
    let p = st.palette;
    let fg = if active {
        p.text_primary
    } else {
        p.text_tertiary
    };
    let indicator_bg = if active {
        with_alpha(p.text_primary, 0.78)
    } else {
        with_alpha(p.text_primary, 0.0)
    };

    div()
        .id(id)
        .flex()
        .flex_col()
        .items_center()
        .cursor_pointer()
        .text_color(fg)
        .hover(move |s| s.text_color(p.text_primary))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.state.update(cx, |st, scx| {
                on_click(st, window, scx);
            });
            cx.stop_propagation();
            cx.notify();
        }))
        .child(single_line(
            RowHeightPx::H24,
            pad_x,
            SpacePx::S4,
            FontPx::F12,
            p.radius(RadiusPx::R4),
            None,
            None,
            fg,
            st.show_centerline,
            p.centerline,
            [label.into().into_any_element()],
        ))
        .child(
            div()
                .w(px(14.0))
                .h(px(2.0))
                .rounded(px(1.0))
                .bg(indicator_bg),
        )
}

pub fn status_dot_label(
    dot_color: Rgba,
    label: impl Into<SharedString>,
    text_color: Rgba,
    show_centerline: bool,
    centerline: Rgba,
) -> AnyElement {
    single_line(
        RowHeightPx::H22,
        SpacePx::S0,
        SpacePx::S4,
        FontPx::F11,
        RadiusPx::R0,
        None,
        None,
        text_color,
        show_centerline,
        centerline,
        [
            div()
                .w(px(6.0))
                .h(px(6.0))
                .rounded(px(3.0))
                .bg(dot_color)
                .into_any_element(),
            label.into().into_any_element(),
        ],
    )
    .into_any_element()
}
