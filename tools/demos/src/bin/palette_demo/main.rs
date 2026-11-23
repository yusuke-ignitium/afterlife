mod data;

use std::sync::Arc;

use afterlife_design::{
    FontPx, GlassSettings, RadiusPx, RowHeightPx, SpacePx, glass_shadows, single_line,
};
use gpui::{
    AnyElement, App, AppContext, Application, Bounds, ClickEvent, Context, Image, ImageFormat,
    ImageSource, InteractiveElement, IntoElement, ObjectFit, ParentElement, Render, Rgba,
    SharedString, StatefulInteractiveElement, Styled, StyledImage, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, div, img, liquid_glass_styled,
    particle_wave_with, point, px, size,
};

const APP_TITLE_LOGO_PNG: &[u8] =
    include_bytes!("../../../../../crates/afterlife-ui/assets/afterlife-logo.png");


use data::{ColorRole, backdrops, base_roles, with_alpha};

struct SixteenColorDemoApp {
    logo: Arc<Image>,
    glass: GlassSettings,
    roles: [ColorRole; 8],
    backdrop_idx: usize,
    active_cards: [bool; 8],
}

impl SixteenColorDemoApp {
    fn new() -> Self {
        let glass = GlassSettings::default();
        let logo = Arc::new(Image::from_bytes(
            ImageFormat::Png,
            APP_TITLE_LOGO_PNG.to_vec(),
        ));
        Self {
            logo,
            glass,
            roles: base_roles(),
            backdrop_idx: 0,
            active_cards: [true; 8],
        }
    }

    fn backdrop_synced_bg(&self, tint: Rgba, accent_kind: f32) -> gpui::Background {
        liquid_glass_styled(
            tint,
            self.glass.disp_scale,
            self.glass.reach_px(),
            self.glass.chroma,
            self.glass.highlight_alpha,
            self.glass.rim_width,
            self.glass.blur_std_dev,
            7,
            accent_kind,
        )
    }

    fn backdrop_synced_btn(&self, tint: Rgba, accent_kind: f32) -> gpui::Background {
        liquid_glass_styled(
            tint,
            self.glass.disp_scale * 0.75,
            self.glass.reach_px().min(14.0),
            self.glass.chroma,
            self.glass.highlight_alpha,
            self.glass.rim_width,
            self.glass.blur_std_dev,
            7,
            accent_kind,
        )
    }

    fn render_palette_matrix(&self, cx: &mut Context<Self>) -> AnyElement {
        let bg = backdrops()[self.backdrop_idx];
        let centerline = self.roles[7].active;
        let neutral = self.roles[0];

        div()
            .flex()
            .flex_col()
            .p(SpacePx::S12.px())
            .gap(SpacePx::S8.px())
            .rounded(px(self.glass.radius))
            .bg(self.backdrop_synced_bg(with_alpha(bg.bg_base, 0.08), 0.0))
            .shadow(glass_shadows())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(single_line(
                        RowHeightPx::H22,
                        SpacePx::S0,
                        SpacePx::S4,
                        FontPx::F12,
                        RadiusPx::R0,
                        None,
                        None,
                        neutral.active,
                        false,
                        centerline,
                        [
                            SharedString::from(
                                "16-Color Base Palette (8 Roles x Active / Inactive)",
                            )
                            .into_any_element(),
                        ],
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(SpacePx::S8.px())
                            .child(
                                single_line(
                                    RowHeightPx::H24,
                                    SpacePx::S8,
                                    SpacePx::S4,
                                    FontPx::F11,
                                    RadiusPx::R4,
                                    None,
                                    None,
                                    neutral.active,
                                    false,
                                    centerline,
                                    [SharedString::from("All Active").into_any_element()],
                                )
                                .id("all-active")
                                .cursor_pointer()
                                .bg(self.backdrop_synced_btn(with_alpha(bg.bg_base, 0.14), 0.0))
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.active_cards = [true; 8];
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                single_line(
                                    RowHeightPx::H24,
                                    SpacePx::S8,
                                    SpacePx::S4,
                                    FontPx::F11,
                                    RadiusPx::R4,
                                    None,
                                    None,
                                    neutral.inactive,
                                    false,
                                    centerline,
                                    [SharedString::from("All Inactive").into_any_element()],
                                )
                                .id("all-inactive")
                                .cursor_pointer()
                                .bg(self.backdrop_synced_btn(with_alpha(bg.bg_base, 0.08), 0.0))
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.active_cards = [false; 8];
                                        cx.notify();
                                    },
                                )),
                            ),
                    ),
            )
            .child(div().flex().flex_col().gap(SpacePx::S8.px()).children(
                [0..4, 4..8].into_iter().map(|range| {
                    div()
                        .flex()
                        .flex_row()
                        .gap(SpacePx::S12.px())
                        .children(range.map(|idx| {
                            let role = self.roles[idx];
                            let is_act = self.active_cards[idx];
                            div()
                                .id(SharedString::from(format!("swatch-{}", idx)))
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .flex_1()
                                .p(SpacePx::S8.px())
                                .rounded(px(self.glass.radius * 0.8))
                                .bg(with_alpha(bg.bg_base, if is_act { 0.30 } else { 0.14 }))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.active_cards[idx] = !this.active_cards[idx];
                                    cx.notify();
                                }))
                                .child(
                                    div()
                                        .text_size(FontPx::F11.px())
                                        .font_family("SF Mono")
                                        .text_color(if is_act {
                                            role.active
                                        } else {
                                            role.inactive
                                        })
                                        .child(role.role),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap(SpacePx::S8.px())
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap(SpacePx::S4.px())
                                                .child(
                                                    div()
                                                        .w(px(12.0))
                                                        .h(px(12.0))
                                                        .rounded(px(2.0))
                                                        .bg(role.active),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F11.px())
                                                        .font_family("SF Mono")
                                                        .text_color(role.active)
                                                        .child(role.active_hex),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap(SpacePx::S4.px())
                                                .child(
                                                    div()
                                                        .w(px(12.0))
                                                        .h(px(12.0))
                                                        .rounded(px(2.0))
                                                        .bg(role.inactive),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F11.px())
                                                        .font_family("SF Mono")
                                                        .text_color(role.inactive)
                                                        .child(role.inactive_hex),
                                                ),
                                        ),
                                )
                        }))
                }),
            ))
            .into_any_element()
    }

    fn render_job_panel(&self, idx: usize, cx: &mut Context<Self>) -> AnyElement {
        let role = self.roles[idx];
        let is_act = self.active_cards[idx];
        let bg = backdrops()[self.backdrop_idx];
        let neutral = self.roles[0];
        let centerline = self.roles[7].active;

        let card_tint = if is_act {
            with_alpha(role.inactive, 0.22)
        } else {
            with_alpha(bg.bg_base, 0.05)
        };
        let edge_kind = if is_act { 0.5 } else { 0.0 };

        let id_col = if is_act { role.active } else { role.inactive };
        let title_col = if is_act {
            neutral.active
        } else {
            neutral.inactive
        };
        let meta_col = if is_act {
            role.inactive
        } else {
            neutral.inactive
        };
        let state_label = if is_act { "Active" } else { "Inactive" };

        div()
            .id(SharedString::from(format!("job-glass-{}", idx)))
            .flex()
            .flex_col()
            .flex_1()
            .p(SpacePx::S12.px())
            .gap(SpacePx::S8.px())
            .rounded(px(self.glass.radius))
            .bg(self.backdrop_synced_bg(card_tint, edge_kind))
            .shadow(glass_shadows())
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.active_cards[idx] = !this.active_cards[idx];
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(single_line(
                        RowHeightPx::H22,
                        SpacePx::S0,
                        SpacePx::S4,
                        FontPx::F11,
                        RadiusPx::R0,
                        None,
                        None,
                        id_col,
                        false,
                        centerline,
                        [
                            SharedString::from(format!("{} · {}", role.sample_id, role.role))
                                .into_any_element(),
                        ],
                    ))
                    .child(single_line(
                        RowHeightPx::H22,
                        SpacePx::S8,
                        SpacePx::S4,
                        FontPx::F11,
                        RadiusPx::R4,
                        None,
                        Some(with_alpha(role.inactive, if is_act { 0.34 } else { 0.14 })),
                        id_col,
                        false,
                        centerline,
                        [SharedString::from(state_label).into_any_element()],
                    )),
            )
            .child(
                div()
                    .text_size(FontPx::F12.px())
                    .text_color(title_col)
                    .child(role.sample_title),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .pt(SpacePx::S4.px())
                    .child(single_line(
                        RowHeightPx::H22,
                        SpacePx::S0,
                        SpacePx::S4,
                        FontPx::F11,
                        RadiusPx::R0,
                        None,
                        None,
                        meta_col,
                        false,
                        centerline,
                        [SharedString::from(role.sample_runner).into_any_element()],
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(SpacePx::S8.px())
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h(RowHeightPx::H24.px())
                                    .px(SpacePx::S8.px())
                                    .rounded(px(self.glass.radius * 0.8))
                                    .bg(self.backdrop_synced_btn(
                                        with_alpha(role.inactive, if is_act { 0.34 } else { 0.14 }),
                                        edge_kind,
                                    ))
                                    .text_size(FontPx::F11.px())
                                    .font_family("SF Mono")
                                    .text_color(id_col)
                                    .child("Action"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h(RowHeightPx::H24.px())
                                    .px(SpacePx::S8.px())
                                    .rounded(px(self.glass.radius * 0.8))
                                    .bg(self.backdrop_synced_btn(with_alpha(bg.bg_base, 0.08), 0.0))
                                    .text_size(FontPx::F11.px())
                                    .font_family("SF Mono")
                                    .text_color(neutral.inactive)
                                    .child("Toggle"),
                            ),
                    ),
            )
            .into_any_element()
    }
}

impl Render for SixteenColorDemoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.request_animation_frame();
        let bg = backdrops()[self.backdrop_idx];
        let neutral = self.roles[0];
        let centerline = self.roles[7].active;

        div()
            .id("pal16-demo-root")
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(particle_wave_with(bg.bg_base, bg.wave_a, bg.wave_b, 1.0))
            .text_color(neutral.active)
            .pt(px(40.0))
            .px(SpacePx::S16.px())
            .pb(SpacePx::S16.px())
            .gap(SpacePx::S12.px())
            .child(
                div()
                    .id("pal16-titlebar-hitbox")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(40.0))
                    .on_click(|ev: &ClickEvent, window, _| {
                        if ev.click_count() == 2 {
                            window.zoom_window();
                        }
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        img(ImageSource::Image(self.logo.clone()))
                            .w(px(140.0))
                            .h(px(36.0))
                            .object_fit(ObjectFit::Contain),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(SpacePx::S8.px())
                            .children(backdrops().into_iter().enumerate().map(|(i, b)| {
                                let is_cur = self.backdrop_idx == i;
                                single_line(
                                    RowHeightPx::H28,
                                    SpacePx::S12,
                                    SpacePx::S4,
                                    FontPx::F11,
                                    RadiusPx::R4,
                                    None,
                                    None,
                                    if is_cur {
                                        neutral.active
                                    } else {
                                        neutral.inactive
                                    },
                                    false,
                                    centerline,
                                    [SharedString::from(b.name).into_any_element()],
                                )
                                .id(SharedString::from(format!("bg-var-{}", i)))
                                .cursor_pointer()
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.backdrop_idx = i;
                                        cx.notify();
                                    },
                                ))
                            })),
                    ),
            )
            .child(self.render_palette_matrix(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .gap(SpacePx::S12.px())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_1()
                            .gap(SpacePx::S12.px())
                            .children((0..4).map(|i| self.render_job_panel(i, cx))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_1()
                            .gap(SpacePx::S12.px())
                            .children((4..8).map(|i| self.render_job_panel(i, cx))),
                    ),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.activate(true);
        let bounds = Bounds::centered(None, size(px(1440.0), px(740.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Afterlife 16-Color Palette Demo".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(14.0), px(14.0))),
                }),
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            },
            |_, cx| cx.new(|_| SixteenColorDemoApp::new()),
        )
        .unwrap();
    });
}
