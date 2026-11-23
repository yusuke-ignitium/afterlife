mod data;

use std::sync::Arc;

use afterlife_design::{
    EdgeAccent, FadePx, FontPx, GlassSettings, GlassSurface, Palette, RadiusPx, RowHeightPx,
    ScrollFadeExt, SpacePx, UiStyle, single_line, with_alpha,
};
use data::{DemoPr, PrPattern, PrScenario};
use gpui::{
    AnyElement, App, AppContext, Application, Bounds, Context, Image, ImageFormat, ImageSource,
    InteractiveElement, IntoElement, ObjectFit, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, StyledImage, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, div, img, particle_wave_with, point,
    prelude::FluentBuilder, px, size,
};

const APP_TITLE_LOGO_PNG: &[u8] =
    include_bytes!("../../../../../crates/afterlife-ui/assets/afterlife-logo.png");
const SHEET_W: f32 = 344.0;
const SHEET_H: f32 = 580.0;

struct DetailPrDemoApp {
    glass: GlassSettings,
    palette: Palette,
    logo: Arc<Image>,
    scenario: PrScenario,
    hide_when_empty: bool,
    last_opened: Option<String>,
}

impl DetailPrDemoApp {
    fn new() -> Self {
        let glass = GlassSettings::default();
        let palette = Palette::from_style(UiStyle::LiquidGlass, &glass);
        let logo = Arc::new(Image::from_bytes(
            ImageFormat::Png,
            APP_TITLE_LOGO_PNG.to_vec(),
        ));
        Self {
            glass,
            palette,
            logo,
            scenario: PrScenario::Multiple,
            hide_when_empty: false,
            last_opened: None,
        }
    }

    fn chip(
        &self,
        id: &'static str,
        label: &'static str,
        active: bool,
        on_click: impl Fn(&mut Self) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        div()
            .id(id)
            .cursor_pointer()
            .h(RowHeightPx::H24.px())
            .px(SpacePx::S8.px())
            .flex()
            .items_center()
            .rounded(p.radius_f32(&self.glass, 0.7))
            .bg(if active {
                p.active_bg
            } else {
                with_alpha(p.text_primary, 0.05)
            })
            .hover(move |s| {
                s.bg(if active {
                    p.active_bg
                } else {
                    with_alpha(p.text_primary, 0.11)
                })
            })
            .text_size(FontPx::F11.px())
            .text_color(if active { p.accent } else { p.text_secondary })
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                on_click(this);
                cx.notify();
            }))
            .into_any_element()
    }

    fn meta_row(
        &self,
        label: &'static str,
        value: &'static str,
        value_color: gpui::Rgba,
    ) -> AnyElement {
        let p = self.palette;
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
                [SharedString::from(label).into_any_element()],
            ))
            .child(single_line(
                RowHeightPx::H22,
                SpacePx::S0,
                SpacePx::S4,
                FontPx::F11,
                RadiusPx::R0,
                None,
                None,
                value_color,
                false,
                p.centerline,
                [SharedString::from(value).into_any_element()],
            ))
            .into_any_element()
    }

    fn render_pr_item(
        &self,
        pat: PrPattern,
        idx: usize,
        pr: DemoPr,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let text = pr.display_label();
        let uri = pr.uri.to_string();
        let id = SharedString::from(format!("{}-pr-{idx}", pat.prefix()));
        let grp = SharedString::from(format!("{}-pr-grp-{idx}", pat.prefix()));

        match pat {
            PrPattern::AQuietRightStack => div()
                .id(id)
                .cursor_pointer()
                .h(RowHeightPx::H22.px())
                .max_w_full()
                .flex()
                .flex_row()
                .items_center()
                .justify_end()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.last_opened = Some(uri.clone());
                    cx.notify();
                }))
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(FontPx::F11.px())
                        .text_color(p.accent)
                        .hover(|s| s.underline())
                        .child(text),
                )
                .into_any_element(),
            PrPattern::BHoverPillWithArrow => div()
                .id(id)
                .group(grp.clone())
                .cursor_pointer()
                .h(RowHeightPx::H22.px())
                .max_w_full()
                .px(SpacePx::S4.px())
                .mr(px(-4.0))
                .rounded(p.radius_f32(&self.glass, 0.55))
                .hover(move |s| s.bg(with_alpha(p.accent_inactive, 0.24)))
                .flex()
                .flex_row()
                .items_center()
                .justify_end()
                .gap(px(4.0))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.last_opened = Some(uri.clone());
                    cx.notify();
                }))
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(FontPx::F11.px())
                        .text_color(p.accent)
                        .child(text),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(FontPx::F11.px())
                        .text_color(with_alpha(p.accent, 0.55))
                        .group_hover(grp, move |s| s.text_color(p.accent))
                        .child("↗"),
                )
                .into_any_element(),
            PrPattern::CDotInlineList => div()
                .id(id)
                .cursor_pointer()
                .h(RowHeightPx::H22.px())
                .max_w_full()
                .flex()
                .flex_row()
                .items_center()
                .justify_end()
                .gap(px(6.0))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.last_opened = Some(uri.clone());
                    cx.notify();
                }))
                .child(
                    div()
                        .w(px(5.0))
                        .h(px(5.0))
                        .rounded(px(2.5))
                        .flex_shrink_0()
                        .bg(p.accent),
                )
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(FontPx::F11.px())
                        .text_color(p.accent)
                        .hover(|s| s.underline())
                        .child(text),
                )
                .into_any_element(),
        }
    }

    fn render_pr_row(
        &self,
        pat: PrPattern,
        prs: &[DemoPr],
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let p = self.palette;
        if prs.is_empty() {
            return (!self.hide_when_empty).then(|| self.meta_row("PR", "None", p.text_tertiary));
        }
        let label = div().flex_shrink_0().child(single_line(
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
            [SharedString::from("PR").into_any_element()],
        ));
        Some(
            div()
                .flex()
                .flex_row()
                .items_start()
                .justify_between()
                .gap(SpacePx::S12.px())
                .child(label)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_end()
                        .flex_1()
                        .min_w(px(0.0))
                        .gap(px(2.0))
                        .children(
                            prs.iter()
                                .enumerate()
                                .map(|(i, pr)| self.render_pr_item(pat, i, *pr, cx)),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_sheet_column(&self, pat: PrPattern, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let sc = self.scenario;
        let prs = sc.prs();
        let is_approve = sc.state_label() == "Approve";

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .flex_shrink_0()
            .px(SpacePx::S12.px())
            .pt(SpacePx::S12.px())
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
                        [SharedString::from(sc.job_id()).into_any_element()],
                    ))
                    .child(single_line(
                        RowHeightPx::H22,
                        SpacePx::S4,
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
            );

        let meta_block = div()
            .flex()
            .flex_col()
            .gap(SpacePx::S4.px())
            .child(self.meta_row("State", sc.state_label(), p.text_secondary))
            .child(self.meta_row("Runner", sc.runner_label(), p.text_secondary))
            .when_some(self.render_pr_row(pat, prs, cx), |d, row| d.child(row));

        let footer = div()
            .absolute()
            .left(SpacePx::S8.px())
            .right(SpacePx::S8.px())
            .bottom(SpacePx::S8.px())
            .child(
                GlassSurface::card(&p, &self.glass)
                    .pad(SpacePx::S8)
                    .gap(SpacePx::S8)
                    .border(Some(p.border_rim))
                    .bg(Some(with_alpha(p.stage_base, 0.72)))
                    .accent(EdgeAccent::Neutral)
                    .build()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(SpacePx::S8.px())
                            .when(is_approve, |d| {
                                d.child(
                                    div()
                                        .h(RowHeightPx::H28.px())
                                        .px(SpacePx::S12.px())
                                        .rounded(p.radius_f32(&self.glass, 0.75))
                                        .bg(p.active_bg)
                                        .flex()
                                        .items_center()
                                        .text_size(FontPx::F12.px())
                                        .text_color(p.accent)
                                        .child("Approve"),
                                )
                                .child(
                                    div()
                                        .h(RowHeightPx::H28.px())
                                        .px(SpacePx::S12.px())
                                        .rounded(p.radius_f32(&self.glass, 0.75))
                                        .bg(p.btn_bg)
                                        .flex()
                                        .items_center()
                                        .text_size(FontPx::F12.px())
                                        .text_color(p.text_secondary)
                                        .child("Rework"),
                                )
                            }),
                    )
                    .child(
                        div()
                            .h(RowHeightPx::H28.px())
                            .px(SpacePx::S8.px())
                            .flex()
                            .items_center()
                            .text_size(FontPx::F11.px())
                            .text_color(p.text_tertiary)
                            .child("Archive"),
                    ),
            );

        let sheet = GlassSurface::panel(&p, &self.glass)
            .pad(SpacePx::S0)
            .gap(SpacePx::S0)
            .bg(Some(p.panel_base))
            .build()
            .relative()
            .w(px(SHEET_W))
            .h(px(SHEET_H))
            .overflow_hidden()
            .child(
                div().w_full().pt(px(6.0)).flex().justify_center().child(
                    div()
                        .w(px(34.0))
                        .h(px(3.0))
                        .rounded(px(1.5))
                        .bg(with_alpha(p.text_primary, 0.16)),
                ),
            )
            .child(header)
            .child(
                div()
                    .id(SharedString::from(format!("{}-scroll", pat.prefix())))
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .px(SpacePx::S12.px())
                    .pt(SpacePx::S8.px())
                    .pb(px(64.0))
                    .gap(SpacePx::S12.px())
                    .overflow_y_scroll()
                    .restrict_scroll_to_axis()
                    .overflow_fade_bottom(FadePx::F24)
                    .child(
                        div()
                            .text_size(FontPx::F13.px())
                            .text_color(p.text_primary)
                            .child(sc.title()),
                    )
                    .child(meta_block)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(SpacePx::S8.px())
                            .text_size(FontPx::F12.px())
                            .text_color(p.text_secondary)
                            .child("## 目的")
                            .child(
                                "State / Runner の直下に PR 行を配置し、複数 PR がある場合は {repo name} #{n} 形式で縦に整列表示する。クリックで該当 PR をブラウザで開ける。",
                            )
                            .child("## 確認ポイント")
                            .child(
                                "1. 単一 PR・複数 PR・長いリポジトリ名での省略バランス\n2. PR が 0 件のときの扱い（非表示 vs None）\n3. 下部 Artifacts セクションとの役割分担",
                            ),
                    ),
            )
            .child(footer);

        div()
            .flex()
            .flex_col()
            .gap(SpacePx::S8.px())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .px(SpacePx::S4.px())
                    .child(div().text_size(FontPx::F13.px()).text_color(p.text_primary).child(pat.title()))
                    .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child(pat.subtitle())),
            )
            .child(sheet)
            .into_any_element()
    }
}

impl Render for DetailPrDemoApp {
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
                    .px(SpacePx::S16.px())
                    .pt(px(34.0))
                    .pb(SpacePx::S12.px())
                    .gap(SpacePx::S12.px())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(SpacePx::S12.px())
                            .child(
                                img(ImageSource::Image(self.logo.clone()))
                                    .w(px(124.0))
                                    .h(px(30.0))
                                    .object_fit(ObjectFit::Contain),
                            )
                            .child(
                                div()
                                    .text_size(FontPx::F12.px())
                                    .text_color(p.text_tertiary)
                                    .child("Job Detail · PR Metadata Row Demo"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(SpacePx::S8.px())
                            .children(PrScenario::ALL.into_iter().map(|s| {
                                self.chip(
                                    s.id(),
                                    s.label(),
                                    self.scenario == s,
                                    move |a| a.scenario = s,
                                    cx,
                                )
                            }))
                            .child(
                                div()
                                    .w(px(1.0))
                                    .h(px(16.0))
                                    .bg(with_alpha(p.text_primary, 0.14)),
                            )
                            .child(self.chip(
                                "toggle-hide-empty",
                                if self.hide_when_empty {
                                    "0 PR: Hidden"
                                } else {
                                    "0 PR: Show None"
                                },
                                self.hide_when_empty,
                                |a| a.hide_when_empty = !a.hide_when_empty,
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .items_start()
                    .justify_center()
                    .gap(SpacePx::S24.px())
                    .px(SpacePx::S16.px())
                    .pb(SpacePx::S16.px())
                    .children(
                        PrPattern::ALL
                            .into_iter()
                            .map(|pat| self.render_sheet_column(pat, cx)),
                    ),
            )
            .when_some(self.last_opened.as_ref(), |d, uri| {
                d.child(
                    div()
                        .px(SpacePx::S16.px())
                        .pb(SpacePx::S12.px())
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .px(SpacePx::S12.px())
                                .py(SpacePx::S4.px())
                                .rounded(p.radius_f32(&self.glass, 0.7))
                                .bg(with_alpha(p.accent_inactive, 0.28))
                                .text_size(FontPx::F11.px())
                                .text_color(p.accent)
                                .child(format!("Clicked -> {uri}")),
                        ),
                )
            })
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.activate(true);
        let bounds = Bounds::centered(None, size(px(1140.0), px(740.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_background: WindowBackgroundAppearance::Opaque,
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Afterlife · Job Detail PR Row Demo")),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(14.0), px(14.0))),
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| DetailPrDemoApp::new()),
        )
        .expect("open demo window");
    });
}
