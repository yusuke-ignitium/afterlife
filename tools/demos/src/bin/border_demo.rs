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
enum DemoPlan {
    PlanA,
    PlanB,
    PlanC,
    PlanD,
}

impl DemoPlan {
    fn edge_mode(self) -> u8 {
        match self {
            Self::PlanA => 1,
            Self::PlanB => 2,
            Self::PlanC => 3,
            Self::PlanD => 4,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::PlanA => "Plan A · Optical Fresnel",
            Self::PlanB => "Plan B · Top-Fade Hairline",
            Self::PlanC => "Plan C · Borderless + Bar",
            Self::PlanD => "Plan D · Wave-Synced Light",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Self::PlanA => "固定の左上光源による光学リム＋影",
            Self::PlanB => "上辺1pxカットエッジが下へ溶ける",
            Self::PlanC => "リムなし / 面濃淡と左端2pxバー",
            Self::PlanD => "Bの切断面＋背景の波と連動する動く光源",
        }
    }
}

struct BorderDemoApp {
    logo: Arc<Image>,
    glass: GlassSettings,
    palette: Palette,
    selected_idx: usize,
}

impl BorderDemoApp {
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

    fn glass_bg(&self, plan: DemoPlan, tint: gpui::Rgba, accent_kind: f32) -> gpui::Background {
        liquid_glass_styled(
            tint,
            self.glass.disp_scale,
            self.glass.reach_px(),
            self.glass.chroma,
            self.glass.highlight_alpha,
            self.glass.rim_width,
            self.glass.blur_std_dev,
            plan.edge_mode(),
            accent_kind,
        )
    }

    fn glass_btn_bg(&self, plan: DemoPlan, tint: gpui::Rgba, accent_kind: f32) -> gpui::Background {
        liquid_glass_styled(
            tint,
            self.glass.disp_scale * 0.75,
            self.glass.reach_px().min(14.0),
            self.glass.chroma,
            self.glass.highlight_alpha,
            self.glass.rim_width,
            self.glass.blur_std_dev,
            plan.edge_mode(),
            accent_kind,
        )
    }

    fn render_plan_column(&self, plan: DemoPlan, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let g = &self.glass;
        let r_stage = px(g.radius * 1.25);
        let r_card = px(g.radius);
        let r_btn = px(g.radius * 0.8);

        let cards = [
            ("JOB-042", "SQLite WAL state-lock benchmark", 0.0f32, "Z3 Verifier"),
            ("JOB-041", "Enforce deterministic px & centering", 0.5f32, "Claude Duet"),
            ("JOB-044", "Approve UDS vs Mach Port IPC spec", 1.0f32, "You"),
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
                        [SharedString::from(plan.title()).into_any_element()],
                    ))
                    .child(
                        div()
                            .text_size(FontPx::F11.px())
                            .text_color(p.text_secondary)
                            .child(plan.subtitle()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p(SpacePx::S12.px())
                    .gap(SpacePx::S12.px())
                    .rounded(r_stage)
                    .bg(self.glass_bg(plan, p.stage_base, 0.0))
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
                    .children(cards.into_iter().enumerate().map(|(idx, (id, title, kind, runner))| {
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
                                r: 43.0 / 255.0,
                                g: 63.0 / 255.0,
                                b: 99.0 / 255.0,
                                a: 0.24,
                            }
                        } else if is_gate {
                            gpui::Rgba {
                                r: 58.0 / 255.0,
                                g: 42.0 / 255.0,
                                b: 14.0 / 255.0,
                                a: 0.16,
                            }
                        } else if plan == DemoPlan::PlanC {
                            gpui::Rgba {
                                r: 18.0 / 255.0,
                                g: 24.0 / 255.0,
                                b: 36.0 / 255.0,
                                a: 0.22,
                            }
                        } else {
                            p.glass_card
                        };

                        let mut card = div()
                            .id(SharedString::from(format!("demo-{}-{}", plan.edge_mode(), idx)))
                            .relative()
                            .flex()
                            .flex_col()
                            .p(SpacePx::S12.px())
                            .gap(SpacePx::S4.px())
                            .rounded(r_card)
                            .bg(self.glass_bg(plan, tint, effective_kind))
                            .shadow(glass_shadows())
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_idx = idx;
                                cx.notify();
                            }));

                        if plan == DemoPlan::PlanC && (is_selected || is_gate) {
                            card = card.child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .top(px(6.0))
                                    .bottom(px(6.0))
                                    .w(px(2.0))
                                    .bg(if is_gate { p.amber } else { p.accent }),
                            );
                        }

                        card.child(
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
                    }))
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
                                        plan,
                                        gpui::Rgba {
                                            r: 43.0 / 255.0,
                                            g: 63.0 / 255.0,
                                            b: 99.0 / 255.0,
                                            a: 0.36,
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
                                        plan,
                                        gpui::Rgba {
                                            r: 18.0 / 255.0,
                                            g: 22.0 / 255.0,
                                            b: 32.0 / 255.0,
                                            a: if plan == DemoPlan::PlanC { 0.28 } else { 0.12 },
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
                                        plan,
                                        gpui::Rgba {
                                            r: 18.0 / 255.0,
                                            g: 22.0 / 255.0,
                                            b: 32.0 / 255.0,
                                            a: if plan == DemoPlan::PlanC { 0.28 } else { 0.12 },
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

impl Render for BorderDemoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.request_animation_frame();
        let p = self.palette;

        div()
            .id("demo-root")
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
                    .id("demo-titlebar-hitbox")
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
                            "Border Comparison Demo A / B / C / D (Click cards to compare)",
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
                    .child(self.render_plan_column(DemoPlan::PlanA, cx))
                    .child(self.render_plan_column(DemoPlan::PlanB, cx))
                    .child(self.render_plan_column(DemoPlan::PlanC, cx))
                    .child(self.render_plan_column(DemoPlan::PlanD, cx)),
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
                    title: Some("Afterlife Edge Style Demo".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(14.0), px(14.0))),
                }),
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            },
            |_, cx| cx.new(|_| BorderDemoApp::new()),
        )
        .unwrap();
    });
}
