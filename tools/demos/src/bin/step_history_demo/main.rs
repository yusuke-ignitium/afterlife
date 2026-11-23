mod data;
mod patterns;

use std::collections::HashSet;
use std::sync::Arc;

use afterlife_design::{
    FadePx, FontPx, GlassSettings, GlassSurface, Palette, RadiusPx, RowHeightPx, ScrollFadeExt,
    SpacePx, UiStyle, single_line, with_alpha,
};
use gpui::{
    AnyElement, App, AppContext, Application, Bounds, Context, Image, ImageFormat, ImageSource,
    InteractiveElement, IntoElement, ObjectFit, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, StyledImage, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, div, img, liquid_glass_styled,
    particle_wave_with, point, px, size,
};

const APP_TITLE_LOGO_PNG: &[u8] =
    include_bytes!("../../../../../crates/afterlife-ui/assets/afterlife-logo.png");


use data::SheetHeightMode;

pub(crate) struct StepHistoryDemoApp {
    logo: Arc<Image>,
    pub(crate) glass: GlassSettings,
    pub(crate) palette: Palette,
    sheet_mode: SheetHeightMode,
    a_collapsed: HashSet<&'static str>,
    b_expanded: HashSet<&'static str>,
    c_collapsed: HashSet<&'static str>,
}

impl StepHistoryDemoApp {
    fn new() -> Self {
        let glass = GlassSettings::default();
        let palette = Palette::from_style(UiStyle::LiquidGlass, &glass);
        let logo = Arc::new(Image::from_bytes(
            ImageFormat::Png,
            APP_TITLE_LOGO_PNG.to_vec(),
        ));
        let mut b_expanded = HashSet::new();
        b_expanded.insert("baseline_guard");
        b_expanded.insert("verify");
        Self {
            logo,
            glass,
            palette,
            sheet_mode: SheetHeightMode::Expanded,
            a_collapsed: HashSet::new(),
            b_expanded,
            c_collapsed: HashSet::new(),
        }
    }

    pub(crate) fn pill_glass(&self, tint: gpui::Rgba, accent: f32) -> gpui::Background {
        liquid_glass_styled(
            tint,
            self.glass.disp_scale,
            self.glass.reach_px(),
            self.glass.chroma,
            self.glass.highlight_alpha,
            self.glass.rim_width,
            self.glass.blur_std_dev,
            7,
            accent,
        )
    }

    pub(crate) fn quiet_btn(&self, id: String, label: &'static str, primary: bool) -> AnyElement {
        let p = self.palette;
        let bg = if primary {
            with_alpha(p.accent, 0.24)
        } else {
            with_alpha(p.text_primary, 0.06)
        };
        let fg = if primary {
            p.text_primary
        } else {
            p.text_secondary
        };
        div()
            .id(SharedString::from(id))
            .cursor_pointer()
            .px(SpacePx::S12.px())
            .h(RowHeightPx::H28.px())
            .flex()
            .items_center()
            .rounded(p.radius_f32(&self.glass, 0.7))
            .bg(bg)
            .text_size(FontPx::F12.px())
            .text_color(fg)
            .child(label)
            .into_any_element()
    }

    pub(crate) fn artifact_row(&self, label: &str) -> AnyElement {
        let p = self.palette;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(SpacePx::S8.px())
            .pt(px(2.0))
            .child(
                div()
                    .w(px(4.0))
                    .h(px(4.0))
                    .rounded(px(2.0))
                    .bg(with_alpha(p.accent, 0.75)),
            )
            .child(
                div()
                    .text_size(FontPx::F11.px())
                    .text_color(p.accent)
                    .child(label.to_string()),
            )
            .into_any_element()
    }

    fn render_native_detail_header(&self) -> AnyElement {
        let p = self.palette;
        div()
            .flex()
            .flex_col()
            .gap(SpacePx::S8.px())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(single_line(
                        RowHeightPx::H28,
                        SpacePx::S0,
                        SpacePx::S4,
                        FontPx::F14,
                        p.radius(RadiusPx::R4),
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
                            .gap(SpacePx::S8.px())
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
                                [SharedString::from("JOB-014").into_any_element()],
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
                                [SharedString::from("Close").into_any_element()],
                            )),
                    ),
            )
            .child(
                div()
                    .text_size(FontPx::F13.px())
                    .text_color(p.text_primary)
                    .child("ステップごとの判断・実行ログと必須証跡契約を導入"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
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
                                p.text_tertiary,
                                false,
                                p.centerline,
                                [SharedString::from("State").into_any_element()],
                            ))
                            .child(single_line(
                                RowHeightPx::H22,
                                SpacePx::S0,
                                SpacePx::S4,
                                FontPx::F11,
                                RadiusPx::R0,
                                None,
                                None,
                                p.amber,
                                false,
                                p.centerline,
                                [SharedString::from("Human Gate").into_any_element()],
                            )),
                    )
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
                                p.text_tertiary,
                                false,
                                p.centerline,
                                [SharedString::from("District").into_any_element()],
                            ))
                            .child(single_line(
                                RowHeightPx::H22,
                                SpacePx::S0,
                                SpacePx::S4,
                                FontPx::F11,
                                RadiusPx::R0,
                                None,
                                None,
                                p.text_secondary,
                                false,
                                p.centerline,
                                [SharedString::from("Product Delivery").into_any_element()],
                            )),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn render_pattern_frame(
        &self,
        title: &'static str,
        subtitle: &'static str,
        scroll_id: &'static str,
        footer_prefix: &'static str,
        body: AnyElement,
    ) -> AnyElement {
        let p = self.palette;
        let sheet_h = self.sheet_mode.height_px();

        div()
            .flex()
            .flex_col()
            .w(px(368.0))
            .min_w(px(368.0))
            .h_full()
            .min_h(px(0.0))
            .gap(SpacePx::S8.px())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .px(SpacePx::S4.px())
                    .child(
                        div()
                            .text_size(FontPx::F13.px())
                            .text_color(p.text_primary)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(FontPx::F11.px())
                            .text_color(p.text_tertiary)
                            .child(subtitle),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .rounded(p.radius_f32(&self.glass, 1.4))
                    .bg(p.panel_base)
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .p(SpacePx::S12.px())
                            .gap(SpacePx::S8.px())
                            .opacity(0.38)
                            .child(
                                div()
                                    .text_size(FontPx::F12.px())
                                    .text_color(p.amber)
                                    .child("Approvals: 1"),
                            )
                            .child(
                                div()
                                    .p(SpacePx::S8.px())
                                    .rounded(p.radius_f32(&self.glass, 0.9))
                                    .bg(with_alpha(p.amber, 0.08))
                                    .text_size(FontPx::F11.px())
                                    .text_color(p.text_secondary)
                                    .child("JOB-014 · Waiting at Human Gate"),
                            ),
                    )
                    .child(
                        GlassSurface::sheet(&p, &self.glass)
                            .border(Some(p.border_rim))
                            .build()
                            .absolute()
                            .left(SpacePx::S4.px())
                            .right(SpacePx::S4.px())
                            .bottom(SpacePx::S4.px())
                            .h(px(sheet_h))
                            .px(SpacePx::S12.px())
                            .pt(SpacePx::S8.px())
                            .pb(SpacePx::S12.px())
                            .flex()
                            .flex_col()
                            .gap(SpacePx::S8.px())
                            .overflow_hidden()
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .justify_center()
                                    .child(
                                        div()
                                            .w(px(32.0))
                                            .h(px(2.0))
                                            .rounded(px(1.0))
                                            .bg(with_alpha(p.text_primary, 0.18)),
                                    ),
                            )
                            .child(
                                div()
                                    .id(scroll_id)
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h(px(0.0))
                                    .overflow_y_scroll()
                                    .restrict_scroll_to_axis()
                                    .overflow_fade_bottom(FadePx::F16)
                                    .gap(SpacePx::S12.px())
                                    .pb(SpacePx::S12.px())
                                    .child(self.render_native_detail_header())
                                    .child(body),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(SpacePx::S8.px())
                                    .pt(SpacePx::S4.px())
                                    .child(self.quiet_btn(
                                        format!("{footer_prefix}-approve"),
                                        "Approve",
                                        true,
                                    ))
                                    .child(self.quiet_btn(
                                        format!("{footer_prefix}-rework"),
                                        "Rework",
                                        false,
                                    ))
                                    .child(self.quiet_btn(
                                        format!("{footer_prefix}-archive"),
                                        "Archive",
                                        false,
                                    )),
                            ),
                    ),
            )
            .into_any_element()
    }
}

impl Render for StepHistoryDemoApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(px(46.0))
                    .pl(px(86.0))
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
                                    .child("Stage-Grouped Minimal Refinements (3A / 3B / 3C)"),
                            ),
                    )
                    .child(
                        div()
                            .id("toggle-sheet-height")
                            .cursor_pointer()
                            .px(SpacePx::S12.px())
                            .h(RowHeightPx::H24.px())
                            .flex()
                            .items_center()
                            .rounded(p.radius_f32(&self.glass, 0.8))
                            .bg(self.pill_glass(p.glass_selected, 0.45))
                            .text_size(FontPx::F12.px())
                            .text_color(p.text_primary)
                            .child(self.sheet_mode.label())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sheet_mode = match this.sheet_mode {
                                    SheetHeightMode::Compact => SheetHeightMode::Expanded,
                                    SheetHeightMode::Expanded => SheetHeightMode::Compact,
                                };
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_center()
                    .flex_1()
                    .min_h(px(0.0))
                    .p(SpacePx::S16.px())
                    .pt(SpacePx::S8.px())
                    .gap(SpacePx::S16.px())
                    .child(self.render_3a_hairline(cx))
                    .child(self.render_3b_pure_typo(cx))
                    .child(self.render_3c_quiet_wash(cx)),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.activate(true);
        let bounds = Bounds::centered(None, size(px(1200.0), px(790.0)), cx);
        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_background: WindowBackgroundAppearance::Opaque,
                titlebar: Some(TitlebarOptions {
                    title: Some("Afterlife · Step History Minimal Demo (3A / 3B / 3C)".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(13.0), px(13.0))),
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| StepHistoryDemoApp::new()),
        );
    });
}
