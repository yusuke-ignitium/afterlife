use std::{cell::Cell, rc::Rc};

use afterlife_design::{
    FadePx, FontPx, GlassSettings, GlassSurface, RadiusPx, RowHeightPx, ScrollFadeExt, SpacePx,
    single_line, with_alpha,
};
use gpui::{
    AnyElement, Bounds, Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, Pixels, SharedString, StatefulInteractiveElement, Styled, canvas, div,
    prelude::FluentBuilder, px, relative,
};

use crate::{
    app::{
        motion::{MotionSnapshot, lerp_rgba},
        state::{AppState, GlassParamKey, GlassParamSection},
    },
    views::root::AfterlifeApp,
};

const DRAWER_WIDTH_PX: f32 = 308.0;
const DRAWER_TOP_OFFSET_PX: f32 = 96.0;
const DRAWER_BOTTOM_MARGIN_PX: f32 = 16.0;
const DRAWER_HEADER_AND_PADDING_PX: f32 = 68.0;

const TINT_PRESETS: [(&str, [u8; 3]); 5] = [
    ("#07090E", [7, 9, 14]),
    ("#0A0A0A", [10, 10, 10]),
    ("#1B2D38", [27, 45, 56]),
    ("#1E2C47", [30, 44, 71]),
    ("#D8E2F4", [216, 226, 244]),
];

pub(crate) fn render_text_action(
    st: &AppState,
    motion: &MotionSnapshot,
    id: &'static str,
    label: &'static str,
    pulse_on_click: bool,
    on_click: impl Fn(&mut AppState) + 'static,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    let pulse = motion.pulse_strength(id);
    let fg = lerp_rgba(p.text_tertiary, p.accent, pulse);
    div()
        .id(id)
        .flex()
        .items_center()
        .h(RowHeightPx::H22.px())
        .px(SpacePx::S4.px())
        .text_size(FontPx::F11.px())
        .text_color(fg)
        .cursor_pointer()
        .hover(move |s| s.text_color(p.text_primary))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.state.update(cx, |st, _| {
                on_click(st);
            });
            if pulse_on_click {
                this.motion.trigger_pulse(id);
            }
            cx.stop_propagation();
            cx.notify();
        }))
        .child(label)
        .into_any_element()
}

fn render_drawer_header(
    st: &AppState,
    motion: &MotionSnapshot,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
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
                    [SharedString::from("Appearance").into_any_element()],
                ))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(SpacePx::S4.px())
                        .child(render_text_action(
                            st,
                            motion,
                            "glass-voxel-def",
                            "Preset",
                            true,
                            |st| {
                                st.reset_glass_settings(GlassSettings::overworld_exact_defaults());
                                st.persist_appearance();
                            },
                            cx,
                        ))
                        .child(render_text_action(
                            st,
                            motion,
                            "glass-reset",
                            "Reset",
                            true,
                            |st| {
                                st.reset_glass_settings(GlassSettings::default());
                                st.persist_appearance();
                            },
                            cx,
                        ))
                        .child(render_text_action(
                            st,
                            motion,
                            "glass-close",
                            "Close",
                            false,
                            |st| {
                                st.settings_open = false;
                            },
                            cx,
                        )),
                ),
        )
        .child(
            div()
                .w_full()
                .h(px(1.0))
                .bg(with_alpha(p.text_primary, 0.08)),
        )
        .into_any_element()
}

fn render_section_label(st: &AppState, title: &'static str, top_pad: SpacePx) -> AnyElement {
    let p = st.palette;
    div()
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .pt(top_pad.px())
        .text_size(FontPx::F11.px())
        .text_color(p.text_tertiary)
        .child(title)
        .into_any_element()
}

pub(crate) fn render_toggle_row(
    st: &AppState,
    id: &'static str,
    label: &'static str,
    active: bool,
    progress: f32,
    on_toggle: impl Fn(&mut AppState) + 'static,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    let t = progress.clamp(0.0, 1.0);
    let track_bg = lerp_rgba(
        with_alpha(p.text_primary, 0.09),
        with_alpha(p.text_primary, 0.24),
        t,
    );
    let thumb_bg = lerp_rgba(with_alpha(p.text_primary, 0.45), p.text_primary, t);
    let status_color = lerp_rgba(p.text_tertiary, p.text_secondary, t);
    let grp = SharedString::from(format!("toggle-grp-{}", id));

    div()
        .id(id)
        .group(grp.clone())
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .h(RowHeightPx::H24.px())
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.state.update(cx, |st, _| {
                on_toggle(st);
            });
            cx.stop_propagation();
            cx.notify();
        }))
        .child(
            div()
                .text_size(FontPx::F12.px())
                .text_color(p.text_secondary)
                .group_hover(grp, move |s| s.text_color(p.text_primary))
                .child(label),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(SpacePx::S8.px())
                .child(
                    div()
                        .w(px(22.0))
                        .text_size(FontPx::F11.px())
                        .font_family("SF Mono")
                        .text_color(status_color)
                        .child(if active { "On" } else { "Off" }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .w(px(28.0))
                        .h(px(16.0))
                        .p(px(2.0))
                        .rounded(px(8.0))
                        .bg(track_bg)
                        .child(
                            div()
                                .ml(px(12.0 * t))
                                .w(px(12.0))
                                .h(px(12.0))
                                .rounded(px(6.0))
                                .bg(thumb_bg),
                        ),
                ),
        )
        .into_any_element()
}

fn render_slider_row(
    st: &AppState,
    idx: usize,
    key: GlassParamKey,
    cell: &Rc<Cell<Bounds<Pixels>>>,
    is_dragging: bool,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    let cell_for_canvas = cell.clone();
    let cell_for_down = cell.clone();
    let (label, min, max, _) = key.spec();
    let val = key.get(st);
    let frac = ((val - min) / (max - min)).clamp(0.0, 1.0);
    let val_text = key.format_value(val);
    let dimmed = key.is_dimmed(&st.glass);
    let grp = SharedString::from(format!("slider-grp-{}", idx));
    let thumb_size = if is_dragging { 10.0 } else { 8.0 };
    let thumb_half = thumb_size * 0.5;
    let fill_alpha = if is_dragging { 0.88 } else { 0.62 };
    let val_color = if is_dragging {
        p.text_primary
    } else {
        p.text_tertiary
    };
    let label_color = if is_dragging {
        p.text_primary
    } else {
        p.text_secondary
    };

    div()
        .group(grp.clone())
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(px(2.0))
        .when(dimmed, |d| d.opacity(0.35))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(FontPx::F12.px())
                        .text_color(label_color)
                        .group_hover(grp.clone(), move |s| s.text_color(p.text_primary))
                        .child(label),
                )
                .child(
                    div()
                        .text_size(FontPx::F11.px())
                        .font_family("SF Mono")
                        .text_color(val_color)
                        .group_hover(grp.clone(), move |s| s.text_color(p.text_secondary))
                        .child(val_text),
                ),
        )
        .child(
            div()
                .id(SharedString::from(format!("glass-slider-{}", idx)))
                .relative()
                .flex()
                .items_center()
                .w_full()
                .h(px(14.0))
                .px(px(5.0))
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        let b = cell_for_down.get();
                        this.apply_slider_at(key, &b, ev.position.x, cx);
                        this.active_slider = Some((key, cell_for_down.clone()));
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .relative()
                        .w_full()
                        .h(px(2.0))
                        .rounded(px(1.0))
                        .bg(with_alpha(p.text_primary, 0.14))
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .w(relative(frac))
                                .rounded(px(1.0))
                                .bg(with_alpha(p.text_primary, fill_alpha))
                                .group_hover(grp, move |s| s.bg(with_alpha(p.text_primary, 0.80))),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(relative(frac))
                                .top(px(1.0 - thumb_half))
                                .ml(px(-thumb_half))
                                .w(px(thumb_size))
                                .h(px(thumb_size))
                                .rounded(px(thumb_half))
                                .bg(p.text_primary),
                        )
                        .child(
                            canvas(
                                move |bounds, _, _| {
                                    cell_for_canvas.set(bounds);
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .inset_0(),
                        ),
                ),
        )
        .into_any_element()
}

fn render_tint_section(st: &AppState, cx: &mut Context<AfterlifeApp>) -> AnyElement {
    let p = st.palette;
    let current_hex = format!(
        "#{:02X}{:02X}{:02X}",
        st.glass.tint_rgb[0], st.glass.tint_rgb[1], st.glass.tint_rgb[2]
    );
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(SpacePx::S8.px())
        .pt(SpacePx::S4.px())
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(FontPx::F12.px())
                        .text_color(p.text_secondary)
                        .child("Tint"),
                )
                .child(
                    div()
                        .text_size(FontPx::F11.px())
                        .font_family("SF Mono")
                        .text_color(p.text_tertiary)
                        .child(current_hex),
                ),
        )
        .child(div().flex().flex_row().gap(SpacePx::S8.px()).children(
            TINT_PRESETS.into_iter().map(|(label, rgb_arr)| {
                let active = st.glass.tint_rgb == rgb_arr;
                let swatch = gpui::Rgba {
                    r: rgb_arr[0] as f32 / 255.0,
                    g: rgb_arr[1] as f32 / 255.0,
                    b: rgb_arr[2] as f32 / 255.0,
                    a: 0.92,
                };
                div()
                    .id(SharedString::from(format!("tint-{}", label)))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(4.0))
                    .flex_1()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.update(cx, |st, _| {
                            st.glass.tint_rgb = rgb_arr;
                            st.refresh_palette();
                            st.persist_appearance();
                        });
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w_full()
                            .h(px(18.0))
                            .rounded(px(4.0))
                            .bg(swatch)
                            .when(!active, |d| d.opacity(0.72).hover(|s| s.opacity(0.94))),
                    )
                    .child(div().w(px(14.0)).h(px(2.0)).rounded(px(1.0)).bg(if active {
                        with_alpha(p.text_primary, 0.85)
                    } else {
                        with_alpha(p.text_primary, 0.0)
                    }))
            }),
        ))
        .into_any_element()
}

pub fn render_appearance_drawer(
    app: &AfterlifeApp,
    st: &AppState,
    motion: &MotionSnapshot,
    window_height: f32,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    let drawer_max_h = (window_height - DRAWER_TOP_OFFSET_PX - DRAWER_BOTTOM_MARGIN_PX).max(200.0);
    let scroll_max_h = (drawer_max_h - DRAWER_HEADER_AND_PADDING_PX).max(120.0);
    let dragging_key = app.active_slider.as_ref().map(|(k, _)| *k);

    GlassSurface::popover(&p, &st.glass)
        .build()
        .w(px(DRAWER_WIDTH_PX))
        .max_h(px(drawer_max_h))
        .overflow_hidden()
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |_, _, cx| {
            cx.stop_propagation();
        })
        .child(render_drawer_header(st, motion, cx))
        .child(
            div()
                .id("appearance-drawer-scroll")
                .flex()
                .flex_col()
                .flex_initial()
                .min_h(px(0.0))
                .max_h(px(scroll_max_h))
                .overflow_y_scroll()
                .restrict_scroll_to_axis()
                .overflow_fade_bottom(FadePx::F16)
                .pb(SpacePx::S12.px())
                .gap(SpacePx::S8.px())
                .child(render_section_label(st, "Backdrop", SpacePx::S0))
                .child(render_toggle_row(
                    st,
                    "toggle-apple-music-artwork",
                    "Apple Music Artwork",
                    st.glass.bg_apple_music_artwork_enabled,
                    motion.toggle_apple_music_progress,
                    |st| {
                        st.glass.bg_apple_music_artwork_enabled =
                            !st.glass.bg_apple_music_artwork_enabled;
                        st.refresh_palette();
                        st.persist_appearance();
                    },
                    cx,
                ))
                .child(render_toggle_row(
                    st,
                    "toggle-youtube-mirror",
                    "YouTube Mirror",
                    st.glass.bg_youtube_mirror_enabled,
                    motion.toggle_youtube_mirror_progress,
                    |st| {
                        st.glass.bg_youtube_mirror_enabled = !st.glass.bg_youtube_mirror_enabled;
                        st.refresh_palette();
                        st.persist_appearance();
                    },
                    cx,
                ))
                .child(render_toggle_row(
                    st,
                    "toggle-backdrop-images",
                    "Backdrop Images",
                    st.glass.bg_image_enabled,
                    motion.toggle_backdrop_progress,
                    |st| {
                        st.glass.bg_image_enabled = !st.glass.bg_image_enabled;
                        st.refresh_palette();
                        st.persist_appearance();
                    },
                    cx,
                ))
                .children(
                    app.slider_bounds
                        .iter()
                        .enumerate()
                        .filter(|(_, (key, _))| key.section() == GlassParamSection::Backdrop)
                        .map(|(idx, (key, cell))| {
                            render_slider_row(st, idx, *key, cell, dragging_key == Some(*key), cx)
                        }),
                )
                .child(render_section_label(st, "Surface", SpacePx::S8))
                .children(
                    app.slider_bounds
                        .iter()
                        .enumerate()
                        .filter(|(_, (key, _))| key.section() == GlassParamSection::Surface)
                        .map(|(idx, (key, cell))| {
                            render_slider_row(st, idx, *key, cell, dragging_key == Some(*key), cx)
                        }),
                )
                .child(render_tint_section(st, cx))
                .child(render_section_label(st, "Inactive", SpacePx::S8))
                .children(
                    app.slider_bounds
                        .iter()
                        .enumerate()
                        .filter(|(_, (key, _))| key.section() == GlassParamSection::Inactive)
                        .map(|(idx, (key, cell))| {
                            render_slider_row(st, idx, *key, cell, dragging_key == Some(*key), cx)
                        }),
                )
                .child(render_toggle_row(
                    st,
                    "toggle-pause-inactive",
                    "Pause When Inactive",
                    st.pause_animation_when_inactive,
                    motion.toggle_pause_progress,
                    |st| {
                        st.pause_animation_when_inactive = !st.pause_animation_when_inactive;
                        st.persist_appearance();
                    },
                    cx,
                )),
        )
        .into_any_element()
}
