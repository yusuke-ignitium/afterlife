use std::sync::Arc;

use afterlife_design::{
    EdgeAccent, FontPx, GlassSettings, GlassSurface, Palette, RadiusPx, RowHeightPx, SpacePx,
    UiStyle, single_line, with_alpha,
};
use gpui::{
    AnyElement, App, AppContext, Application, Bounds, Context, Image, ImageFormat, ImageSource,
    InteractiveElement, IntoElement, ObjectFit, ParentElement, Render, Rgba, SharedString,
    StatefulInteractiveElement, Styled, StyledImage, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, div, img, liquid_glass_styled,
    particle_wave_with, point, prelude::FluentBuilder, px, size,
};

const APP_TITLE_LOGO_PNG: &[u8] =
    include_bytes!("../../../../crates/afterlife-ui/assets/afterlife-logo.png");
const CARD_TOGGLE_WIDTH_PX: f32 = 66.0;
const DETAIL_TOGGLE_WIDTH_PX: f32 = 76.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Variant {
    C3aGhostAtRest,
    C3bSoftResumePill,
}

impl Variant {
    fn prefix(self) -> &'static str {
        match self {
            Self::C3aGhostAtRest => "c3a",
            Self::C3bSoftResumePill => "c3b",
        }
    }

    fn resume_resting_bg(self) -> bool {
        matches!(self, Self::C3bSoftResumePill)
    }
}

#[derive(Clone)]
struct DemoJob {
    id: &'static str,
    title: &'static str,
    sub_stage: &'static str,
    runner: &'static str,
    summary: &'static str,
    stopped: bool,
}

struct ColumnState {
    jobs: [DemoJob; 2],
    selected: usize,
}

impl ColumnState {
    fn initial() -> Self {
        Self {
            jobs: [
                DemoJob {
                    id: "JOB-003",
                    title: "Prevent fabricated artifact URIs & wire blob storage",
                    sub_stage: "apply_change",
                    runner: "Claude Code",
                    summary: "Enforce fail-closed artifact URI whitelist and persist evidence via ArtifactBlobStore.",
                    stopped: false,
                },
                DemoJob {
                    id: "JOB-004",
                    title: "Unify default districts & step labels to English",
                    sub_stage: "edge_guard_tests",
                    runner: "Codex",
                    summary: "Align built-in seed copy to English while preserving user-defined multilingual specs.",
                    stopped: true,
                },
            ],
            selected: 0,
        }
    }
}

fn plain_line(
    p: &Palette,
    height: RowHeightPx,
    font: FontPx,
    radius: RadiusPx,
    color: Rgba,
    text: impl Into<SharedString>,
) -> AnyElement {
    single_line(
        height,
        SpacePx::S0,
        SpacePx::S4,
        font,
        radius,
        None,
        None,
        color,
        false,
        p.centerline,
        [text.into().into_any_element()],
    )
    .into_any_element()
}

fn dot_line(p: &Palette, dot_color: Rgba, text_color: Rgba, text: &'static str) -> AnyElement {
    single_line(
        RowHeightPx::H22,
        SpacePx::S0,
        SpacePx::S4,
        FontPx::F11,
        RadiusPx::R0,
        None,
        None,
        text_color,
        false,
        p.centerline,
        [
            div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(dot_color).into_any_element(),
            SharedString::from(text).into_any_element(),
        ],
    )
    .into_any_element()
}

struct JobStopperDemoApp {
    logo: Arc<Image>,
    glass: GlassSettings,
    palette: Palette,
    c3a: ColumnState,
    c3b: ColumnState,
}

impl JobStopperDemoApp {
    fn new() -> Self {
        let glass = GlassSettings::default();
        let palette = Palette::from_style(UiStyle::LiquidGlass, &glass);
        let logo = Arc::new(Image::from_bytes(ImageFormat::Png, APP_TITLE_LOGO_PNG.to_vec()));
        Self {
            logo,
            glass,
            palette,
            c3a: ColumnState::initial(),
            c3b: ColumnState::initial(),
        }
    }

    fn col_mut(&mut self, variant: Variant) -> &mut ColumnState {
        match variant {
            Variant::C3aGhostAtRest => &mut self.c3a,
            Variant::C3bSoftResumePill => &mut self.c3b,
        }
    }

    fn col(&self, variant: Variant) -> &ColumnState {
        match variant {
            Variant::C3aGhostAtRest => &self.c3a,
            Variant::C3bSoftResumePill => &self.c3b,
        }
    }

    fn all_stopped(&self) -> bool {
        [&self.c3a, &self.c3b].iter().all(|c| c.jobs.iter().all(|j| j.stopped))
    }

    fn pause_glyph(&self, color: Rgba) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.0))
            .child(div().w(px(2.0)).h(px(7.0)).rounded(px(1.0)).bg(color))
            .child(div().w(px(2.0)).h(px(7.0)).rounded(px(1.0)).bg(color))
            .into_any_element()
    }

    fn chip_glass(&self, tint: Rgba, accent: f32) -> gpui::Background {
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

    #[allow(clippy::too_many_arguments)]
    fn action_chip(
        &self,
        id: String,
        label: &'static str,
        tone_bg: Rgba,
        fg: Rgba,
        fixed_width: Option<f32>,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(SharedString::from(id))
            .cursor_pointer()
            .h(RowHeightPx::H28.px())
            .when_some(fixed_width, |d, w| d.w(px(w)).justify_center())
            .when(fixed_width.is_none(), |d| d.px(SpacePx::S12.px()))
            .flex()
            .items_center()
            .rounded(self.palette.radius_f32(&self.glass, 0.75))
            .bg(self.chip_glass(tone_bg, 0.35))
            .hover(|s| s.opacity(0.88))
            .text_size(FontPx::F12.px())
            .text_color(fg)
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                on_click(this, cx);
                cx.notify();
            }))
            .into_any_element()
    }

    fn render_stage_header(&self, active_running: bool) -> AnyElement {
        let p = self.palette;
        let sub_stages = [
            ("Apply Change", "AI Agent", active_running),
            ("Edge & Guard Tests", "AI Agent", false),
            ("Hygiene Gate", "Build & Test", false),
        ];

        div()
            .flex()
            .flex_col()
            .gap(SpacePx::S8.px())
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
                            .gap(SpacePx::S8.px())
                            .child(plain_line(&p, RowHeightPx::H24, FontPx::F13, p.radius(RadiusPx::R2), p.text_primary, "4. Build Core"))
                            .child(plain_line(&p, RowHeightPx::H22, FontPx::F11, p.radius(RadiusPx::R2), p.text_tertiary, "AI Agent")),
                    )
                    .child(dot_line(&p, p.purple, p.purple, "Sub-SM: Build & Hygiene")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .pl(SpacePx::S8.px())
                    .border_l(px(1.0))
                    .border_color(with_alpha(p.purple_inactive, 0.55))
                    .gap(SpacePx::S4.px())
                    .children(sub_stages.into_iter().map(|(label, runner, is_active)| {
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap(SpacePx::S8.px())
                            .child(dot_line(
                                &p,
                                if is_active { p.purple } else { p.text_tertiary },
                                if is_active { p.purple } else { p.text_secondary },
                                label,
                            ))
                            .child(plain_line(&p, RowHeightPx::H22, FontPx::F11, RadiusPx::R0, p.text_tertiary, runner))
                    })),
            )
            .into_any_element()
    }

    fn render_job_card(
        &self,
        variant: Variant,
        idx: usize,
        job: &DemoJob,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let tone_fg = p.coral;
        let tone_inactive = p.coral_inactive;
        let prefix = variant.prefix();
        let grp = SharedString::from(format!("{prefix}-card-grp-{idx}"));
        let is_stopped = job.stopped;

        let base_bg = if is_stopped {
            with_alpha(tone_inactive, if is_selected { 0.20 } else { 0.12 })
        } else if is_selected {
            p.glass_selected
        } else {
            p.glass_card
        };
        let border_color = match (is_selected, is_stopped) {
            (true, true) => with_alpha(tone_fg, 0.72),
            (true, false) => p.accent,
            (false, true) => with_alpha(tone_fg, 0.28),
            (false, false) => p.border_rim,
        };
        let id_color = if is_stopped {
            tone_fg
        } else if is_selected {
            p.accent
        } else {
            p.text_secondary
        };
        let title_color = if is_stopped { p.text_secondary } else { p.text_primary };

        let toggle_btn = div()
            .id(SharedString::from(format!("{prefix}-inline-toggle-{idx}")))
            .cursor_pointer()
            .w(px(CARD_TOGGLE_WIDTH_PX))
            .h(RowHeightPx::H22.px())
            .rounded(p.radius_f32(&self.glass, 0.5))
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .gap(SpacePx::S4.px())
            .flex_shrink_0()
            .when(!is_stopped, |d| {
                d.opacity(0.0)
                    .group_hover(grp.clone(), |s| s.opacity(1.0))
                    .hover(|s| s.bg(with_alpha(tone_inactive, 0.28)))
                    .child(self.pause_glyph(with_alpha(p.text_secondary, 0.85)))
                    .child(div().text_size(FontPx::F11.px()).text_color(p.text_secondary).child("Stop"))
            })
            .when(is_stopped, |d| {
                d.when(variant.resume_resting_bg(), |b| b.bg(with_alpha(tone_inactive, 0.18)))
                    .hover(|s| s.bg(with_alpha(p.accent_inactive, 0.28)))
                    .child(div().w(px(5.0)).h(px(5.0)).rounded(px(2.5)).bg(tone_fg))
                    .child(div().text_size(FontPx::F11.px()).text_color(tone_fg).child("Resume"))
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                let col = this.col_mut(variant);
                col.jobs[idx].stopped = !col.jobs[idx].stopped;
                col.selected = idx;
                cx.notify();
            }));

        GlassSurface::card(&p, &self.glass)
            .pad(SpacePx::S8)
            .gap(SpacePx::S4)
            .border(Some(border_color))
            .bg(Some(base_bg))
            .accent(if is_stopped { EdgeAccent::Gate } else { EdgeAccent::Selected })
            .build()
            .overflow_hidden()
            .id(SharedString::from(format!("{prefix}-card-{idx}")))
            .group(grp)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.col_mut(variant).selected = idx;
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(SpacePx::S8.px())
                    .child(plain_line(
                        &p,
                        RowHeightPx::H22,
                        FontPx::F11,
                        p.radius(RadiusPx::R2),
                        id_color,
                        format!("{} ({})", job.id, job.sub_stage),
                    ))
                    .child(toggle_btn),
            )
            .child(div().text_size(FontPx::F13.px()).text_color(title_color).child(job.title))
            .into_any_element()
    }

    fn render_detail_sheet(&self, variant: Variant, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let tone_fg = p.coral;
        let tone_inactive = p.coral_inactive;
        let col = self.col(variant);
        let idx = col.selected;
        let job = &col.jobs[idx];
        let prefix = variant.prefix();
        let is_stopped = job.stopped;
        let status_label = if is_stopped {
            format!("Build Core · {} · Stopped", job.sub_stage)
        } else {
            format!("Build Core · {} · {}", job.sub_stage, job.runner)
        };

        GlassSurface::card(&p, &self.glass)
            .pad(SpacePx::S12)
            .gap(SpacePx::S8)
            .radius_factor(1.1)
            .bg(Some(p.glass_sheet))
            .accent(EdgeAccent::Neutral)
            .build()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(SpacePx::S8.px())
                    .child(div().text_size(FontPx::F11.px()).text_color(if is_stopped { tone_fg } else { p.accent }).child(job.id))
                    .child(
                        div()
                            .text_size(FontPx::F11.px())
                            .text_color(if is_stopped { with_alpha(tone_fg, 0.85) } else { p.text_tertiary })
                            .child(status_label),
                    ),
            )
            .child(div().text_size(FontPx::F13.px()).text_color(p.text_primary).child(job.title))
            .child(div().text_size(FontPx::F12.px()).text_color(p.text_secondary).child(job.summary))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(SpacePx::S8.px())
                    .pt(SpacePx::S4.px())
                    .child(self.action_chip(
                        format!("{prefix}-detail-stop-resume"),
                        if is_stopped { "Resume" } else { "Stop" },
                        if is_stopped { p.active_bg } else { with_alpha(tone_inactive, 0.30) },
                        if is_stopped { p.accent } else { tone_fg },
                        Some(DETAIL_TOGGLE_WIDTH_PX),
                        move |this, _| {
                            let c = this.col_mut(variant);
                            c.jobs[idx].stopped = !c.jobs[idx].stopped;
                        },
                        cx,
                    ))
                    .child(self.action_chip(
                        format!("{prefix}-detail-rework"),
                        "Rework",
                        p.btn_bg,
                        p.text_secondary,
                        None,
                        |_, _| {},
                        cx,
                    ))
                    .child(self.action_chip(
                        format!("{prefix}-detail-archive"),
                        "Archive",
                        p.btn_bg,
                        p.text_secondary,
                        None,
                        |_, _| {},
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_column(
        &self,
        variant: Variant,
        title: &'static str,
        subtitle: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let col = self.col(variant);
        let any_running = col.jobs.iter().any(|j| !j.stopped);

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(340.0))
            .max_w(px(400.0))
            .gap(SpacePx::S12.px())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .px(SpacePx::S4.px())
                    .child(div().text_size(FontPx::F14.px()).text_color(p.text_primary).child(title))
                    .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child(subtitle)),
            )
            .child(
                GlassSurface::card(&p, &self.glass)
                    .pad(SpacePx::S12)
                    .gap(SpacePx::S12)
                    .radius_factor(1.25)
                    .border(Some(p.border_rim))
                    .bg(Some(p.stage_base))
                    .accent(EdgeAccent::Neutral)
                    .build()
                    .child(self.render_stage_header(any_running))
                    .children(col.jobs.iter().enumerate().map(|(idx, job)| {
                        self.render_job_card(variant, idx, job, col.selected == idx, cx)
                    })),
            )
            .child(div().px(SpacePx::S4.px()).text_size(FontPx::F11.px()).text_color(p.text_tertiary).child("Job Detail Sheet"))
            .child(self.render_detail_sheet(variant, cx))
            .into_any_element()
    }
}

impl Render for JobStopperDemoApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(particle_wave_with(p.window_backdrop, p.wave_a, p.wave_b, self.glass.bg_brightness))
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
                            .child(img(ImageSource::Image(self.logo.clone())).h(px(22.0)).w(px(86.0)).object_fit(ObjectFit::Contain))
                            .child(div().text_size(FontPx::F12.px()).text_color(p.text_secondary).child("C3 Final · Hover-Only Stop + Fixed Width")),
                    )
                    .child(
                        div()
                            .id("toggle-all-state")
                            .cursor_pointer()
                            .px(SpacePx::S12.px())
                            .h(RowHeightPx::H24.px())
                            .flex()
                            .items_center()
                            .rounded(p.radius_f32(&self.glass, 0.8))
                            .bg(self.chip_glass(p.btn_bg, 0.25))
                            .text_size(FontPx::F12.px())
                            .text_color(p.text_secondary)
                            .child(if self.all_stopped() { "Resume All" } else { "Stop All" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                let next = !this.all_stopped();
                                for col in [&mut this.c3a, &mut this.c3b] {
                                    for job in &mut col.jobs {
                                        job.stopped = next;
                                    }
                                }
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
                    .child(self.render_column(
                        Variant::C3aGhostAtRest,
                        "C3a. Pure Ghost Button",
                        "No resting button bg · button bg appears only on button hover",
                        cx,
                    ))
                    .child(self.render_column(
                        Variant::C3bSoftResumePill,
                        "C3b. Soft Resume Pill",
                        "Stop is ghost on card hover · Resume keeps subtle coral bg",
                        cx,
                    )),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.activate(true);
        let bounds = Bounds::centered(None, size(px(880.0), px(680.0)), cx);
        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_background: WindowBackgroundAppearance::Opaque,
                titlebar: Some(TitlebarOptions {
                    title: Some("Afterlife · Job Stopper C3 Final (C3a / C3b)".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(13.0), px(13.0))),
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| JobStopperDemoApp::new()),
        );
    });
}
