use std::sync::Arc;

use afterlife_design::{
    FontPx, GlassSettings, Palette, RadiusPx, RowHeightPx, SpacePx, UiStyle, single_line,
    with_alpha,
};
use gpui::{
    AnyElement, App, AppContext, Application, Bounds, BoxShadow, Context, Image, ImageFormat,
    ImageSource, InteractiveElement, IntoElement, ObjectFit, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, StyledImage, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, div, img, linear_color_stop,
    linear_gradient, liquid_glass_styled, particle_wave_with, point, px, rgba, size,
};

const APP_TITLE_LOGO_PNG: &[u8] =
    include_bytes!("../../../../crates/afterlife-ui/assets/afterlife-logo.png");

const DETAIL_HEIGHT_PX: f32 = 248.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum OverlapPattern {
    WhisperFrost,
    FloatingAmbient,
    ClearRefractVeil,
    DockedMatte,
}

impl OverlapPattern {
    const ALL: [Self; 4] = [
        Self::WhisperFrost,
        Self::FloatingAmbient,
        Self::ClearRefractVeil,
        Self::DockedMatte,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::WhisperFrost => "1. Whisper Frost",
            Self::FloatingAmbient => "2. Floating Ambient",
            Self::ClearRefractVeil => "3. Clear Glass + Veil",
            Self::DockedMatte => "4. Docked Matte",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Self::WhisperFrost => "flush overlap · 11px frost · soft 2-tier upward shadow",
            Self::FloatingAmbient => "4px inset card · 14px frost · wide ambient lift",
            Self::ClearRefractVeil => "5px light frost · subtle refraction + 16px shadow veil",
            Self::DockedMatte => "bottom-docked · 16px deep matte · tight crisp shadow",
        }
    }

    fn id_prefix(self) -> &'static str {
        match self {
            Self::WhisperFrost => "p1",
            Self::FloatingAmbient => "p2",
            Self::ClearRefractVeil => "p3",
            Self::DockedMatte => "p4",
        }
    }
}

struct SampleJob {
    id: &'static str,
    ticket: Option<&'static str>,
    title: &'static str,
    spec: &'static str,
}

const SAMPLE_JOBS: &[SampleJob] = &[
    SampleJob {
        id: "JOB-041",
        ticket: Some("YKZ-104"),
        title: "UI の px トークンと縦中央揃えの決定論的ガードを適用",
        spec: "Guard SpacePx / FontPx / RowHeightPx / FadePx across all desktop panes.",
    },
    SampleJob {
        id: "JOB-042",
        ticket: Some("YKZ-108"),
        title: "特許の件に「AI マッチング」とは別の、人材に寄り添う提案だと伝わる呼称をつける",
        spec: "Explore candidate naming grounded in human-centered curation rather than algorithmic matching.",
    },
    SampleJob {
        id: "JOB-043",
        ticket: Some("ENG-219"),
        title: "SQLite WAL 競合ロックとリース期限切れの自動検証スイートを拡充",
        spec: "Add concurrent 16-worker claim contention and zombie fencing token rejection checks.",
    },
    SampleJob {
        id: "JOB-044",
        ticket: Some("YKZ-112"),
        title: "OAuth 2.1 Broker のスコープ分離とトークン失効パスを監査",
        spec: "Verify human approval routes stay fail-closed under agent-scoped tokens.",
    },
    SampleJob {
        id: "JOB-045",
        ticket: None,
        title: "ローカル MCP 接続のレスポンス p95 とメモリ RSS を定点計測",
        spec: "Capture steady-state telemetry under 30Hz EventBus bursts with zero idle frames.",
    },
    SampleJob {
        id: "JOB-046",
        ticket: Some("OPS-087"),
        title: "District 02 のリサーチ手順書 v3 へフィードバックを統合",
        spec: "Incorporate rework feedback into Philosophy v2 and ProcedureSpec v3 draft.",
    },
    SampleJob {
        id: "JOB-047",
        ticket: Some("YKZ-121"),
        title: "マルチモニタ環境でのウィンドウ復元と最小サイズクランプを検証",
        spec: "Ensure disconnected display fallback restores safely inside primary visibleFrame.",
    },
];

struct DetailOverlapDemoApp {
    logo: Arc<Image>,
    glass: GlassSettings,
    palette: Palette,
    selected_idx: usize,
    detail_open: bool,
}

impl DetailOverlapDemoApp {
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
            detail_open: true,
        }
    }

    fn card_glass(&self, tint: gpui::Rgba, accent_kind: f32) -> gpui::Background {
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

    fn sheet_glass(&self, tint: gpui::Rgba, blur: f32, disp_cap: f32) -> gpui::Background {
        liquid_glass_styled(
            tint,
            self.glass.disp_scale.min(disp_cap),
            self.glass.reach_px(),
            self.glass.chroma,
            self.glass.highlight_alpha,
            self.glass.rim_width,
            self.glass.blur_std_dev.max(blur),
            7,
            0.0,
        )
    }

    fn render_sheet(&self, pat: OverlapPattern) -> AnyElement {
        let p = self.palette;
        let g = &self.glass;
        let job = &SAMPLE_JOBS[self.selected_idx];
        let r_panel = p.radius_f32(g, 1.4);
        let r_float = p.radius_f32(g, 1.2);
        let grip_color = with_alpha(p.text_primary, 0.18);

        let (bg, shadows, top_hairline, inset_px, round_bottom) = match pat {
            OverlapPattern::WhisperFrost => (
                self.sheet_glass(with_alpha(p.window_backdrop, 0.56), 11.0, 110.0),
                vec![
                    BoxShadow {
                        color: rgba(0x00000059).into(),
                        offset: point(px(0.0), px(-8.0)),
                        blur_radius: px(20.0),
                        spread_radius: px(0.0),
                    },
                    BoxShadow {
                        color: rgba(0x00000044).into(),
                        offset: point(px(0.0), px(-2.0)),
                        blur_radius: px(6.0),
                        spread_radius: px(0.0),
                    },
                ],
                Some(with_alpha(p.text_primary, 0.10)),
                0.0,
                true,
            ),
            OverlapPattern::FloatingAmbient => (
                self.sheet_glass(with_alpha(p.window_backdrop, 0.48), 14.0, 95.0),
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
                ],
                Some(with_alpha(p.text_primary, 0.14)),
                4.0,
                true,
            ),
            OverlapPattern::ClearRefractVeil => (
                self.sheet_glass(with_alpha(p.window_backdrop, 0.66), 5.0, 145.0),
                vec![BoxShadow {
                    color: rgba(0x00000045).into(),
                    offset: point(px(0.0), px(-4.0)),
                    blur_radius: px(14.0),
                    spread_radius: px(0.0),
                }],
                Some(with_alpha(p.text_primary, 0.08)),
                0.0,
                true,
            ),
            OverlapPattern::DockedMatte => (
                self.sheet_glass(with_alpha(p.window_backdrop, 0.78), 16.0, 80.0),
                vec![
                    BoxShadow {
                        color: rgba(0x00000066).into(),
                        offset: point(px(0.0), px(-6.0)),
                        blur_radius: px(14.0),
                        spread_radius: px(0.0),
                    },
                    BoxShadow {
                        color: rgba(0x0000004d).into(),
                        offset: point(px(0.0), px(-1.0)),
                        blur_radius: px(3.0),
                        spread_radius: px(0.0),
                    },
                ],
                Some(with_alpha(p.text_primary, 0.12)),
                0.0,
                false,
            ),
        };

        let mut sheet = div()
            .relative()
            .flex()
            .flex_col()
            .h(px(DETAIL_HEIGHT_PX))
            .p(SpacePx::S12.px())
            .gap(SpacePx::S8.px())
            .bg(bg)
            .shadow(shadows)
            .overflow_hidden();

        if inset_px > 0.0 {
            sheet = sheet.rounded(r_float);
        } else if round_bottom {
            sheet = sheet.rounded(r_panel);
        } else {
            sheet = sheet.rounded_t(r_panel);
        }

        if let Some(hairline) = top_hairline {
            sheet = sheet.child(
                div()
                    .absolute()
                    .top_0()
                    .left(SpacePx::S12.px())
                    .right(SpacePx::S12.px())
                    .h(px(1.0))
                    .bg(hairline),
            );
        }

        sheet = sheet
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().w(px(32.0)).h(px(2.0)).rounded(px(1.0)).bg(grip_color)),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .pt(SpacePx::S4.px())
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
                        [SharedString::from("Job Detail").into_any_element()],
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
                                p.accent,
                                false,
                                p.centerline,
                                [SharedString::from(job.id).into_any_element()],
                            ))
                            .child(single_line(
                                RowHeightPx::H22,
                                SpacePx::S0,
                                SpacePx::S0,
                                FontPx::F11,
                                RadiusPx::R0,
                                None,
                                None,
                                p.text_tertiary,
                                false,
                                p.centerline,
                                [SharedString::from("Close").into_any_element()],
                            )),
                    ),
            )
            .child(
                div()
                    .text_size(FontPx::F13.px())
                    .text_color(p.text_primary)
                    .child(if let Some(t) = job.ticket {
                        format!("{t} {}", job.title)
                    } else {
                        job.title.to_string()
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(SpacePx::S4.px())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(FontPx::F11.px())
                                    .text_color(p.text_tertiary)
                                    .child("State"),
                            )
                            .child(
                                div()
                                    .text_size(FontPx::F11.px())
                                    .text_color(p.text_secondary)
                                    .child("Jobs (Queued)"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(FontPx::F11.px())
                                    .text_color(p.text_tertiary)
                                    .child("Runner"),
                            )
                            .child(
                                div()
                                    .text_size(FontPx::F11.px())
                                    .text_color(p.text_secondary)
                                    .child("Unclaimed"),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(FontPx::F12.px())
                    .text_color(p.text_secondary)
                    .child(job.spec),
            );

        let mut wrapper = div()
            .absolute()
            .left(px(inset_px))
            .right(px(inset_px))
            .bottom(px(inset_px))
            .flex()
            .flex_col();

        if pat == OverlapPattern::ClearRefractVeil {
            wrapper = wrapper.child(div().h(px(16.0)).w_full().bg(linear_gradient(
                180.0,
                linear_color_stop(rgba(0x04060a00), 0.0),
                linear_color_stop(rgba(0x04060a52), 1.0),
            )));
        }

        wrapper.child(sheet).into_any_element()
    }

    fn render_pattern_col(&self, pat: OverlapPattern, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let g = &self.glass;
        let r_panel = p.radius_f32(g, 1.4);
        let r_card = p.radius_f32(g, 1.0);
        let bottom_pad = if self.detail_open {
            DETAIL_HEIGHT_PX + SpacePx::S24.val()
        } else {
            SpacePx::S24.val()
        };

        let mut pane = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .rounded(r_panel)
            .bg(p.panel_base)
            .overflow_hidden()
            .child(
                div()
                    .id(SharedString::from(format!("{}-scroll", pat.id_prefix())))
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .restrict_scroll_to_axis()
                    .pt(SpacePx::S12.px())
                    .px(SpacePx::S12.px())
                    .pb(px(bottom_pad))
                    .gap(SpacePx::S12.px())
                    .child(single_line(
                        RowHeightPx::H28,
                        SpacePx::S0,
                        SpacePx::S4,
                        FontPx::F14,
                        RadiusPx::R0,
                        None,
                        None,
                        p.text_primary,
                        false,
                        p.centerline,
                        [SharedString::from(format!("Jobs: {}", SAMPLE_JOBS.len()))
                            .into_any_element()],
                    ))
                    .children(SAMPLE_JOBS.iter().enumerate().map(|(idx, job)| {
                        let is_sel = self.detail_open && self.selected_idx == idx;
                        let card_bg = if is_sel {
                            p.glass_selected
                        } else {
                            p.glass_card
                        };
                        let id_color = if is_sel { p.accent } else { p.text_secondary };
                        let accent_kind = if is_sel { 0.5 } else { 0.0 };

                        div()
                            .id(SharedString::from(format!(
                                "{}-card-{}",
                                pat.id_prefix(),
                                idx
                            )))
                            .flex()
                            .flex_col()
                            .flex_shrink_0()
                            .p(SpacePx::S12.px())
                            .gap(SpacePx::S4.px())
                            .rounded(r_card)
                            .bg(self.card_glass(card_bg, accent_kind))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_idx = idx;
                                this.detail_open = true;
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
                                        id_color,
                                        false,
                                        p.centerline,
                                        [SharedString::from(job.id).into_any_element()],
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap(SpacePx::S4.px())
                                            .child(
                                                div()
                                                    .w(px(5.0))
                                                    .h(px(5.0))
                                                    .rounded(px(3.0))
                                                    .bg(p.text_tertiary),
                                            )
                                            .child(
                                                div()
                                                    .text_size(FontPx::F11.px())
                                                    .text_color(p.text_tertiary)
                                                    .child("Queued"),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .flex_wrap()
                                    .text_size(FontPx::F13.px())
                                    .text_color(p.text_primary)
                                    .child(if let Some(t) = job.ticket {
                                        format!("{t} {}", job.title)
                                    } else {
                                        job.title.to_string()
                                    }),
                            )
                    })),
            );

        if self.detail_open {
            pane = pane.child(self.render_sheet(pat));
        }

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(280.0))
            .h_full()
            .min_h(px(0.0))
            .gap(SpacePx::S8.px())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(SpacePx::S4.px())
                    .px(SpacePx::S4.px())
                    .child(
                        div()
                            .text_size(FontPx::F14.px())
                            .text_color(p.text_primary)
                            .child(pat.title()),
                    )
                    .child(
                        div()
                            .text_size(FontPx::F11.px())
                            .text_color(p.text_secondary)
                            .child(pat.subtitle()),
                    ),
            )
            .child(pane)
            .into_any_element()
    }
}

impl Render for DetailOverlapDemoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.request_animation_frame();
        let p = self.palette;

        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(particle_wave_with(
                p.window_backdrop,
                p.wave_a,
                p.wave_b,
                self.glass.bg_brightness,
            ))
            .text_color(p.text_primary)
            .font_family(".AppleSystemUIFont")
            .child(
                div()
                    .h(px(40.0))
                    .flex_shrink_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .pl(px(88.0))
                    .pr(SpacePx::S16.px())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(SpacePx::S16.px())
                            .child(
                                img(ImageSource::Image(self.logo.clone()))
                                    .h(px(22.0))
                                    .w(px(86.0))
                                    .object_fit(ObjectFit::Contain),
                            )
                            .child(
                                div()
                                    .text_size(FontPx::F12.px())
                                    .text_color(p.text_secondary)
                                    .child("Detail Sheet Overlap & Shadow Patterns — scroll any column to inspect"),
                            ),
                    )
                    .child(
                        div()
                            .id("toggle-detail")
                            .cursor_pointer()
                            .px(SpacePx::S12.px())
                            .h(RowHeightPx::H24.px())
                            .flex()
                            .items_center()
                            .rounded(p.radius_f32(&self.glass, 0.8))
                            .bg(self.card_glass(
                                if self.detail_open {
                                    p.glass_selected
                                } else {
                                    p.glass_card
                                },
                                if self.detail_open { 0.5 } else { 0.0 },
                            ))
                            .text_size(FontPx::F12.px())
                            .text_color(p.text_primary)
                            .child(if self.detail_open {
                                "Detail: Open"
                            } else {
                                "Detail: Closed"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.detail_open = !this.detail_open;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h(px(0.0))
                    .p(SpacePx::S16.px())
                    .pt(SpacePx::S8.px())
                    .gap(SpacePx::S16.px())
                    .children(
                        OverlapPattern::ALL
                            .into_iter()
                            .map(|pat| self.render_pattern_col(pat, cx)),
                    ),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.activate(true);
        let bounds = Bounds::centered(None, size(px(1480.0), px(820.0)), cx);
        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_background: WindowBackgroundAppearance::Opaque,
                titlebar: Some(TitlebarOptions {
                    title: Some("Afterlife · Detail Overlap Demo".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(13.0), px(13.0))),
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| DetailOverlapDemoApp::new()),
        );
    });
}
