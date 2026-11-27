use afterlife_core::{Job, JobLocation};
use afterlife_design::{
    EdgeAccent, FadePx, FontPx, GlassSurface, RadiusPx, RowHeightPx, ScrollFadeExt, SpacePx,
    single_line, with_alpha,
};
use gpui::{
    AnyElement, BoxShadow, Context, CursorStyle, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Rgba, SharedString, StatefulInteractiveElement, Styled, div,
    point, prelude::FluentBuilder, px,
};

use crate::{
    app::{
        confirm::{ConfirmSnapshot, ConfirmTone},
        motion::{MotionSnapshot, lerp_glass_rgba, lerp_rgba},
        state::AppState,
        window_state::DEFAULT_DETAIL_HEIGHT_PX,
    },
    views::root::{AfterlifeApp, DetailResizeDrag},
};

pub const STICKY_FOOTER_SCROLL_BOTTOM_PAD_PX: f32 = 64.0;
pub const STICKY_FOOTER_WITH_NOTICE_BOTTOM_PAD_PX: f32 = 104.0;
pub const DETAIL_TOGGLE_WIDTH_PX: f32 = 76.0;

pub fn detail_footer_visible_for_job(job: &Job) -> bool {
    !matches!(job.location, JobLocation::Archived)
}

pub fn detail_scroll_bottom_pad_px(has_footer: bool, has_notice: bool) -> f32 {
    if has_footer && has_notice {
        STICKY_FOOTER_WITH_NOTICE_BOTTOM_PAD_PX
    } else if has_footer {
        STICKY_FOOTER_SCROLL_BOTTOM_PAD_PX
    } else {
        SpacePx::S12.val()
    }
}

pub struct DetailSheetChrome {
    pub sheet_id: &'static str,
    pub resize_handle_id: &'static str,
    pub resize_group_id: &'static str,
    pub scroll_id: &'static str,
    pub close_id: &'static str,
    pub title: &'static str,
    pub item_id: String,
    pub item_id_color: Rgba,
    pub detail_target_h: f32,
    pub is_resizing_detail: bool,
}

pub fn borderless_action_chip(
    st: &AppState,
    id: impl Into<SharedString>,
    label: &'static str,
    bg: Rgba,
    fg: Rgba,
    fixed_width: Option<f32>,
    on_click: impl Fn(&mut AppState) + 'static,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    div()
        .id(id.into())
        .cursor_pointer()
        .h(RowHeightPx::H28.px())
        .when_some(fixed_width, |d, w| d.w(px(w)))
        .when(fixed_width.is_none(), |d| d.px(SpacePx::S12.px()))
        .flex()
        .items_center()
        .justify_center()
        .rounded(p.radius_f32(&st.glass, 0.75))
        .bg(bg)
        .hover(|s| s.opacity(0.88))
        .text_size(FontPx::F12.px())
        .text_color(fg)
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.disarm_confirm(cx);
            this.state.update(cx, |st, _| {
                on_click(st);
            });
            cx.stop_propagation();
            cx.notify();
        }))
        .into_any_element()
}

pub fn two_step_action_chip(
    st: &AppState,
    confirm: &ConfirmSnapshot,
    key: impl Into<String>,
    label: &'static str,
    tone: ConfirmTone,
    ghost_idle: bool,
    on_confirm: impl Fn(&mut AppState) + 'static,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    let key = key.into();
    let arm_t = confirm.weight(&key);

    let (idle_bg, idle_fg, accent) = match tone {
        ConfirmTone::Approve => (p.active_bg, p.accent, p.accent),
        ConfirmTone::Rework => (p.btn_bg, p.text_secondary, p.amber),
        ConfirmTone::Archive => (
            Rgba {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            },
            p.text_tertiary,
            p.coral,
        ),
    };
    let bg = lerp_glass_rgba(idle_bg, accent, arm_t);
    let fg = lerp_rgba(idle_fg, p.window_backdrop, arm_t);
    let glow = (arm_t > 1e-3).then(|| {
        vec![BoxShadow {
            color: with_alpha(accent, 0.28 * arm_t).into(),
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(12.0 * arm_t),
            spread_radius: px(0.0),
        }]
    });
    let font_size = if ghost_idle {
        FontPx::F11.px()
    } else {
        FontPx::F12.px()
    };
    let pad_x = if ghost_idle {
        SpacePx::S8.px()
    } else {
        SpacePx::S12.px()
    };
    let click_key = key.clone();

    div()
        .id(SharedString::from(key))
        .cursor_pointer()
        .h(RowHeightPx::H28.px())
        .px(pad_x)
        .flex()
        .items_center()
        .justify_center()
        .rounded(p.radius_f32(&st.glass, 0.75))
        .bg(bg)
        .when_some(glow, |d, shadows| d.shadow(shadows))
        .when(ghost_idle && arm_t < 0.05, move |d| {
            d.hover(move |s| {
                s.bg(with_alpha(p.text_primary, 0.08))
                    .text_color(p.text_primary)
            })
        })
        .when(!ghost_idle || arm_t >= 0.05, |d| {
            d.hover(|s| s.opacity(0.90))
        })
        .text_size(font_size)
        .text_color(fg)
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.trigger_confirm_button(click_key.clone(), |st| on_confirm(st), cx);
            cx.stop_propagation();
        }))
        .into_any_element()
}

pub fn render_detail_sheet_frame(
    st: &AppState,
    motion: &MotionSnapshot,
    chrome: DetailSheetChrome,
    body_nodes: Vec<AnyElement>,
    pinned_notice: Option<String>,
    footer_actions: Option<AnyElement>,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    let detail_t = motion.detail_progress;
    let swap_t = motion.detail_swap_progress;
    let inner_pt = (1.0 - detail_t) * 6.0 + (1.0 - swap_t) * 2.0;
    let has_footer = footer_actions.is_some();
    let has_notice = pinned_notice.is_some();
    let scroll_pb = detail_scroll_bottom_pad_px(has_footer, has_notice);
    let detail_target_h = chrome.detail_target_h;
    let is_resizing_detail = chrome.is_resizing_detail;

    let card_border = if is_resizing_detail {
        with_alpha(p.accent, 0.58)
    } else {
        p.border_rim
    };
    let grip_color = if is_resizing_detail {
        with_alpha(p.accent, 0.85)
    } else {
        with_alpha(p.text_primary, 0.16)
    };
    let grip_hover = with_alpha(p.text_primary, 0.44);

    let header = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .flex_shrink_0()
        .px(SpacePx::S12.px())
        .pt(px(SpacePx::S12.val() + inner_pt))
        .pb(SpacePx::S4.px())
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
            [SharedString::from(chrome.title).into_any_element()],
        ))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(SpacePx::S12.px())
                .child(single_line(
                    RowHeightPx::H22,
                    SpacePx::S0,
                    SpacePx::S4,
                    FontPx::F11,
                    RadiusPx::R0,
                    None,
                    None,
                    chrome.item_id_color,
                    st.show_centerline,
                    p.centerline,
                    [SharedString::from(chrome.item_id).into_any_element()],
                ))
                .child(
                    single_line(
                        RowHeightPx::H22,
                        SpacePx::S4,
                        SpacePx::S0,
                        FontPx::F11,
                        RadiusPx::R0,
                        None,
                        None,
                        p.text_tertiary,
                        st.show_centerline,
                        p.centerline,
                        [SharedString::from("Close").into_any_element()],
                    )
                    .id(chrome.close_id)
                    .cursor_pointer()
                    .hover(move |s| s.text_color(p.text_primary))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.update(cx, |st, _| {
                            st.close_detail();
                        });
                        cx.notify();
                    })),
                ),
        );

    let floating_footer = footer_actions.map(|actions| {
        let pill = GlassSurface::popover(&p, &st.glass)
            .pad(SpacePx::S8)
            .gap(SpacePx::S0)
            .radius_factor(1.0)
            .border(None)
            .bg(Some(p.glass_popover))
            .accent(EdgeAccent::Neutral)
            .build()
            .child(actions);

        div()
            .absolute()
            .left(SpacePx::S8.px())
            .right(SpacePx::S8.px())
            .bottom(SpacePx::S8.px())
            .flex()
            .flex_col()
            .gap(SpacePx::S4.px())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .when_some(pinned_notice, |d, err| {
                d.child(
                    div()
                        .px(SpacePx::S8.px())
                        .py(SpacePx::S4.px())
                        .rounded(p.radius_f32(&st.glass, 0.55))
                        .bg(with_alpha(p.coral, 0.16))
                        .text_size(FontPx::F11.px())
                        .text_color(p.coral)
                        .child(SharedString::from(err)),
                )
            })
            .child(pill)
    });

    GlassSurface::sheet(&p, &st.glass)
        .pad(SpacePx::S0)
        .gap(SpacePx::S0)
        .border(Some(card_border))
        .build()
        .id(chrome.sheet_id)
        .absolute()
        .left(SpacePx::S4.px())
        .right(SpacePx::S4.px())
        .bottom(SpacePx::S4.px())
        .h(px(detail_target_h * detail_t))
        .overflow_hidden()
        .opacity(detail_t)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id(chrome.resize_handle_id)
                .group(chrome.resize_group_id)
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(12.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor(CursorStyle::ResizeUpDown)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                        if ev.click_count == 2 {
                            this.detail_resize = None;
                            this.state.update(cx, |st, _| {
                                st.detail_height = DEFAULT_DETAIL_HEIGHT_PX;
                            });
                            this.persist_window_state(window, cx);
                        } else {
                            this.detail_resize = Some(DetailResizeDrag {
                                start_mouse_y: f32::from(ev.position.y),
                                start_height: detail_target_h,
                            });
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .w(px(if is_resizing_detail { 48.0 } else { 32.0 }))
                        .h(px(2.0))
                        .rounded(px(1.0))
                        .bg(grip_color)
                        .group_hover(chrome.resize_group_id, move |s| {
                            s.w(px(44.0)).bg(grip_hover)
                        }),
                ),
        )
        .child(header)
        .child(
            div()
                .id(chrome.scroll_id)
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .restrict_scroll_to_axis()
                .overflow_fade_bottom(FadePx::F24)
                .px(SpacePx::S12.px())
                .pt(SpacePx::S8.px())
                .pb(px(scroll_pb))
                .opacity(0.72 + 0.28 * swap_t)
                .gap(SpacePx::S12.px())
                .children(body_nodes),
        )
        .when_some(floating_footer, |d, footer| d.child(footer))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use afterlife_core::{AfterlifeEngine, DistrictId, EngineCommand};

    use super::*;

    #[test]
    fn detail_footer_visibility_and_scroll_padding_adapt_to_job_state() {
        let mut engine = AfterlifeEngine::new();
        let human = engine.local_actor();
        let dist = DistrictId::from("DIST-001");
        let jid = engine.queue_preset_job("Footer Visibility Job", Some(&dist));

        assert!(detail_footer_visible_for_job(engine.find_job(&jid).unwrap()));
        assert_eq!(detail_scroll_bottom_pad_px(true, false), 64.0);
        assert_eq!(detail_scroll_bottom_pad_px(false, false), 12.0);
        assert!(detail_scroll_bottom_pad_px(true, true) > 64.0);

        engine.assign_job_to_district(&jid, &dist).unwrap();
        assert!(detail_footer_visible_for_job(engine.find_job(&jid).unwrap()));

        engine
            .apply(
                &human,
                EngineCommand::ArchiveJob {
                    job_id: jid.clone(),
                    reason: Some("Archive check".into()),
                    expected_revision: None,
                    fencing_token: None,
                    at: 1_900_000_000_000,
                },
            )
            .unwrap();
        assert!(!detail_footer_visible_for_job(engine.find_job(&jid).unwrap()));
    }
}
