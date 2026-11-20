use std::sync::Arc;

use afterlife_design::{
    FontPx, GlassSettings, Palette, RadiusPx, RowHeightPx, SpacePx, UiStyle, glass_shadows,
    single_line,
};
use gpui::{
    AnyElement, App, AppContext, Application, Bounds, ClickEvent, Context, Image, ImageFormat,
    ImageSource, InteractiveElement, IntoElement, ObjectFit, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, StyledImage, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, div, img, liquid_glass_styled,
    particle_wave, point, px, size,
};

const APP_TITLE_LOGO_PNG: &[u8] =
    include_bytes!("../../../../crates/afterlife-ui/assets/afterlife-logo.png");

#[derive(Clone, Copy, PartialEq, Eq)]
enum EdgePattern {
    RazorPlatinum,
    IceChromeBevel,
    BackdropSynced,
    StealthObsidian,
}

impl EdgePattern {
    fn edge_mode(self) -> u8 {
        match self {
            Self::RazorPlatinum => 5,
            Self::IceChromeBevel => 6,
            Self::BackdropSynced => 7,
            Self::StealthObsidian => 8,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::RazorPlatinum => "1. Razor Platinum",
            Self::IceChromeBevel => "2. Ice Chrome Bevel",
            Self::BackdropSynced => "3. Backdrop-Synced",
            Self::StealthObsidian => "4. Stealth Obsidian",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Self::RazorPlatinum => "0.5px極細プラチナ加算 / 無彩色で背景に完全同化",
            Self::IceChromeBevel => "明線＋暗線の2段カットベベル / 彫刻的なキレ",
            Self::BackdropSynced => "背後の波色相を直接拾ってエッジ発光に同化",
            Self::StealthObsidian => "漆黒の暗縁＋波頂点の一点に鋭い白グリント",
        }
    }
}

struct EdgePaletteDemoApp {
    logo: Arc<Image>,
    glass: GlassSettings,
    palette: Palette,
    selected_idx: usize,
}

impl EdgePaletteDemoApp {
    fn new() -> Self {
        let glass = GlassSettings::default();
        let palette = Palette::from_style(UiStyle::LiquidGlass, &glass);
        let logo = Arc::new(Image::from_bytes(
            ImageFormat::Png,
            APP_TITLE_LOGO_PNG.to_vec(),
        ));
        Self {
            logo,
            glass,
            palette,
            selected_idx: 1,
        }
    }

    fn glass_bg(&self, pat: EdgePattern, tint: gpui::Rgba, accent_kind: f32) -> gpui::Background {
        liquid_glass_styled(
            tint,
            self.glass.disp_scale,
            self.glass.reach_px(),
            self.glass.chroma,
            self.glass.highlight_alpha,
            self.glass.rim_width,
            self.glass.blur_std_dev,
            pat.edge_mode(),
            accent_kind,
        )
    }

    fn glass_btn_bg(
        &self,
        pat: EdgePattern,
        tint: gpui::Rgba,
        accent_kind: f32,
    ) -> gpui::Background {
        liquid_glass_styled(
            tint,
            self.glass.disp_scale * 0.75,
            self.glass.reach_px().min(14.0),
            self.glass.chroma,
            self.glass.highlight_alpha,
            self.glass.rim_width,
            self.glass.blur_std_dev,
            pat.edge_mode(),
            accent_kind,
        )
    }

    fn render_pattern_col(&self, pat: EdgePattern, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let g = &self.glass;
        let r_stage = px(g.radius * 1.25);
        let r_card = px(g.radius);
        let r_btn = px(g.radius * 0.8);

        let cards = [
            (
                "JOB-042",
                "SQLite WAL state-lock benchmark",
                0.0f32,
                "Z3 Verifier",
            ),
            (
                "JOB-041",
                "Enforce deterministic px & centering",
                0.5f32,
                "Claude Duet",
            ),
            (
                "JOB-044",
                "Approve UDS vs Mach Port IPC spec",
                1.0f32,
                "You",
            ),
        ];

        div()
            .flex()
            .flex_col()
            .flex_1()
            .gap(SpacePx::S12.px())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(SpacePx::S4.px())
                    .child(single_line(
                        RowHeightPx::H24,
                        SpacePx::S0,
                        SpacePx::S4,
                        FontPx::F14,
                        RadiusPx::R0,
                        None,
                        None,
                        p.text_primary,
                        false,
                        p.centerline,
                        [SharedString::from(pat.title()).into_any_element()],
                    ))
                    .child(
                        div()
                            .text_size(FontPx::F11.px())
                            .text_color(p.text_secondary)
                            .child(pat.subtitle()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p(SpacePx::S12.px())
                    .gap(SpacePx::S12.px())
                    .rounded(r_stage)
                    .bg(self.glass_bg(pat, p.stage_base, 0.0))
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
                                p.text_primary,
                                false,
                                p.centerline,
                                [SharedString::from("2. Implement").into_any_element()],
                            ))
                            .child(single_line(
                                RowHeightPx::H22,
                                SpacePx::S0,
                                SpacePx::S4,
                                FontPx::F11,
                                RadiusPx::R0,
                                None,
                                None,
                                p.text_tertiary,
                                false,
                                p.centerline,
                                [SharedString::from("Dots").into_any_element()],
                            )),
                    )
                    .children(cards.into_iter().enumerate().map(
                        |(idx, (id, title, kind, runner))| {
                            let is_selected = self.selected_idx == idx;
                            let is_gate = kind > 0.75;
                            let effective_kind = if is_selected {
                                0.5
                            } else if is_gate {
                                1.0
                            } else {
                                0.0
                            };

                            let tint = if is_selected {
                                gpui::Rgba {
                                    r: 28.0 / 255.0,
                                    g: 44.0 / 255.0,
                                    b: 74.0 / 255.0,
                                    a: 0.16,
                                }
                            } else if is_gate {
                                gpui::Rgba {
                                    r: 46.0 / 255.0,
                                    g: 34.0 / 255.0,
                                    b: 12.0 / 255.0,
                                    a: 0.12,
                                }
                            } else {
                                p.glass_card
                            };

                            div()
                                .id(SharedString::from(format!(
                                    "ep-{}-{}",
                                    pat.edge_mode(),
                                    idx
                                )))
                                .relative()
                                .flex()
                                .flex_col()
                                .p(SpacePx::S12.px())
                                .gap(SpacePx::S4.px())
                                .rounded(r_card)
                                .bg(self.glass_bg(pat, tint, effective_kind))
                                .shadow(glass_shadows())
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.selected_idx = idx;
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
                                            if is_gate {
                                                p.amber
                                            } else if is_selected {
                                                p.accent
                                            } else {
                                                p.text_secondary
                                            },
                                            false,
                                            p.centerline,
                                            [SharedString::from(id).into_any_element()],
                                        ))
                                        .child(single_line(
                                            RowHeightPx::H22,
                                            SpacePx::S0,
                                            SpacePx::S4,
                                            FontPx::F11,
                                            RadiusPx::R0,
                                            None,
                                            None,
                                            p.text_tertiary,
                                            false,
                                            p.centerline,
                                            [SharedString::from(runner).into_any_element()],
                                        )),
                                )
                                .child(
                                    div()
                                        .text_size(FontPx::F12.px())
                                        .text_color(p.text_primary)
                                        .child(title),
                                )
                        },
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(SpacePx::S8.px())
                            .pt(SpacePx::S4.px())
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h(RowHeightPx::H28.px())
                                    .px(SpacePx::S12.px())
                                    .rounded(r_btn)
                                    .bg(self.glass_btn_bg(
                                        pat,
                                        gpui::Rgba {
                                            r: 28.0 / 255.0,
                                            g: 44.0 / 255.0,
                                            b: 74.0 / 255.0,
                                            a: 0.22,
                                        },
                                        0.5,
                                    ))
                                    .text_size(FontPx::F11.px())
                                    .font_family("SF Mono")
                                    .text_color(p.text_on_accent)
                                    .child("Approve"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h(RowHeightPx::H28.px())
                                    .px(SpacePx::S12.px())
                                    .rounded(r_btn)
                                    .bg(self.glass_btn_bg(
                                        pat,
                                        gpui::Rgba {
                                            r: 10.0 / 255.0,
                                            g: 12.0 / 255.0,
                                            b: 18.0 / 255.0,
                                            a: 0.08,
                                        },
                                        0.0,
                                    ))
                                    .text_size(FontPx::F11.px())
                                    .font_family("SF Mono")
                                    .text_color(p.text_primary)
                                    .child("Rework"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h(RowHeightPx::H28.px())
                                    .px(SpacePx::S12.px())
                                    .rounded(r_btn)
                                    .bg(self.glass_btn_bg(
                                        pat,
                                        gpui::Rgba {
                                            r: 10.0 / 255.0,
                                            g: 12.0 / 255.0,
                                            b: 18.0 / 255.0,
                                            a: 0.08,
                                        },
                                        0.0,
                                    ))
                                    .text_size(FontPx::F11.px())
                                    .font_family("SF Mono")
                                    .text_color(p.text_secondary)
                                    .child("Runner Pull"),
                            ),
                    ),
            )
            .into_any_element()
    }
}

impl Render for EdgePaletteDemoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.request_animation_frame();
        let p = self.palette;

        div()
            .id("ep-demo-root")
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(particle_wave(p.window_backdrop))
            .text_color(p.text_primary)
            .pt(px(40.0))
            .px(SpacePx::S16.px())
            .pb(SpacePx::S16.px())
            .gap(SpacePx::S16.px())
            .child(
                div()
                    .id("ep-titlebar-hitbox")
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
                    .child(single_line(
                        RowHeightPx::H24,
                        SpacePx::S0,
                        SpacePx::S4,
                        FontPx::F11,
                        RadiusPx::R0,
                        None,
                        None,
                        p.text_secondary,
                        false,
                        p.centerline,
                        [SharedString::from(
                            "Sharp Edge & Highlight Palette Demo (Click cards to compare)",
                        )
                        .into_any_element()],
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .gap(SpacePx::S16.px())
                    .child(self.render_pattern_col(EdgePattern::RazorPlatinum, cx))
                    .child(self.render_pattern_col(EdgePattern::IceChromeBevel, cx))
                    .child(self.render_pattern_col(EdgePattern::BackdropSynced, cx))
                    .child(self.render_pattern_col(EdgePattern::StealthObsidian, cx)),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.activate(true);
        let bounds = Bounds::centered(None, size(px(1520.0), px(560.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Afterlife Edge Palette Demo".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(14.0), px(14.0))),
                }),
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            },
            |_, cx| cx.new(|_| EdgePaletteDemoApp::new()),
        )
        .unwrap();
    });
}
