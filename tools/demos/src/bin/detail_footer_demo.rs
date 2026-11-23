use std::sync::Arc;

use afterlife_design::{
    EdgeAccent, FadePx, FontPx, GlassSettings, GlassSurface, Palette, RadiusPx, RowHeightPx,
    ScrollFadeExt, SpacePx, UiStyle, single_line, with_alpha,
};
use gpui::{
    AnyElement, App, AppContext, Application, Bounds, Context, Image, ImageFormat, ImageSource,
    InteractiveElement, IntoElement, ObjectFit, ParentElement, Render, Rgba, SharedString,
    StatefulInteractiveElement, Styled, StyledImage, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, div, img, particle_wave_with, point,
    prelude::FluentBuilder, px, size,
};

const APP_TITLE_LOGO_PNG: &[u8] =
    include_bytes!("../../../../crates/afterlife-ui/assets/afterlife-logo.png");
const DETAIL_TOGGLE_WIDTH_PX: f32 = 72.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum FooterVariant {
    C1LiquidPopover,
    C2DeepSmoke,
    C3AiryCapsule,
}

impl FooterVariant {
    fn index(self) -> usize {
        match self {
            Self::C1LiquidPopover => 0,
            Self::C2DeepSmoke => 1,
            Self::C3AiryCapsule => 2,
        }
    }

    fn id_prefix(self) -> &'static str {
        match self {
            Self::C1LiquidPopover => "var-c1",
            Self::C2DeepSmoke => "var-c2",
            Self::C3AiryCapsule => "var-c3",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::C1LiquidPopover => "C1 · Liquid Glass Pill",
            Self::C2DeepSmoke => "C2 · Deep Smoke Glass",
            Self::C3AiryCapsule => "C3 · Airy Frosted Capsule",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Self::C1LiquidPopover => "No solid border · popover blur + natural optical glass rim",
            Self::C2DeepSmoke => "No solid border · deeper dark smoke + high button contrast",
            Self::C3AiryCapsule => "No solid border · lighter translucency + softer capsule radius",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DemoContextKind {
    ApprovalGate,
    WorkStage,
    Queued,
    Proposal,
}

impl DemoContextKind {
    fn label(self) -> &'static str {
        match self {
            Self::ApprovalGate => "ApprovalGate (4)",
            Self::WorkStage => "Work Stage (3)",
            Self::Queued => "Queued (1)",
            Self::Proposal => "Proposal (1)",
        }
    }

    fn header_title(self) -> &'static str {
        if self == Self::Proposal { "Proposal Detail" } else { "Job Detail" }
    }

    fn item_id(self) -> &'static str {
        match self {
            Self::ApprovalGate => "JOB-006",
            Self::WorkStage => "JOB-011",
            Self::Queued => "JOB-012",
            Self::Proposal => "PROP-SPEC-0002",
        }
    }

    fn title_text(self) -> &'static str {
        match self {
            Self::ApprovalGate => {
                "District内ジョブのStop / Resume（一時停止・再開）ガバナンス機能の本実装（C3a仕様）"
            }
            Self::WorkStage => {
                "イベント駆動待機プリミティブの実装（afterlife wait / afterlife watch --ndjson）"
            }
            Self::Queued => "Detail 下部固定アクションバーの本実装",
            Self::Proposal => "Evolve DIST-002 into 5-stage Discovery & Shaping workflow",
        }
    }

    fn stage_label(self, stopped: bool) -> String {
        let base = match self {
            Self::ApprovalGate => "Human Gate",
            Self::WorkStage => "Build & Hygiene (apply_change)",
            Self::Queued => "Jobs (Queued)",
            Self::Proposal => "DIST-002 · ProcedureSpec · S v1 → v2",
        };
        if stopped && matches!(self, Self::ApprovalGate | Self::WorkStage) {
            format!("{base} · Stopped")
        } else {
            base.to_string()
        }
    }
}

struct StepEntry {
    stage: &'static str,
    sub: &'static str,
    outcome: &'static str,
    field_label: &'static str,
    body: &'static str,
    age: &'static str,
}

const DEMO_STEPS: &[StepEntry] = &[
    StepEntry {
        stage: "Spec & Recon",
        sub: "trace_sources",
        outcome: "advanced",
        field_label: "Confirmed Facts & Constraints",
        body: "Traced job_detail.rs and proposal_detail.rs. Action buttons currently sit at the bottom of left-detail-scroll behind 10+ step records.",
        age: "14m",
    },
    StepEntry {
        stage: "Spec & Recon",
        sub: "lock_criteria",
        outcome: "advanced",
        field_label: "Acceptance Criteria",
        body: "Controls must stay permanently pinned at the bottom without obscuring scrolled content or introducing idle redraws.",
        age: "11m",
    },
    StepEntry {
        stage: "Architecture & Plan",
        sub: "design",
        outcome: "advanced",
        field_label: "Chosen Design & Module Boundaries",
        body: "Separate header, flex-1 scroll body with FadePx bottom mask, and pinned action footer shared by Job Detail and Proposal Detail.",
        age: "8m",
    },
    StepEntry {
        stage: "Build & Hygiene",
        sub: "apply_change",
        outcome: "advanced",
        field_label: "Changes Applied",
        body: "Implemented Stop/Resume governance with Human-only fail-closed checks, lease revocation on stop, and C3a ghost button on district cards.",
        age: "5m",
    },
    StepEntry {
        stage: "Build & Hygiene",
        sub: "hygiene_gate",
        outcome: "advanced",
        field_label: "Zero-Warning Green Confirmation",
        body: "All workspace tests passed including no_comments_guard, module_size_guard, fade_px_guard, and SQLite backward-compatibility suites.",
        age: "2m",
    },
    StepEntry {
        stage: "Verify & Ship",
        sub: "bundle_evidence",
        outcome: "advanced",
        field_label: "Evidence Summary",
        body: "Attached verification report and live test output blobs. Ready for Human ApprovalGate sign-off.",
        age: "1m",
    },
];

const DEMO_ARTIFACTS: &[(&str, &str)] = &[
    ("art-00021", "verification-report-job006.md"),
    ("art-00022", "stop-resume-compat-test.log"),
    ("art-00023", "spec-c3a-ghost-button.md"),
];

struct DetailFooterDemoApp {
    logo: Arc<Image>,
    glass: GlassSettings,
    palette: Palette,
    ctx_kind: DemoContextKind,
    sticky_header: bool,
    stopped: [bool; 3],
    last_action: [Option<String>; 3],
}

impl DetailFooterDemoApp {
    fn new() -> Self {
        let glass = GlassSettings::default();
        let palette = Palette::from_style(UiStyle::LiquidGlass, &glass);
        let logo = Arc::new(Image::from_bytes(ImageFormat::Png, APP_TITLE_LOGO_PNG.to_vec()));
        Self {
            logo,
            glass,
            palette,
            ctx_kind: DemoContextKind::ApprovalGate,
            sticky_header: true,
            stopped: [false, false, false],
            last_action: [None, None, None],
        }
    }

    fn render_top_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let kinds = [
            DemoContextKind::ApprovalGate,
            DemoContextKind::WorkStage,
            DemoContextKind::Queued,
            DemoContextKind::Proposal,
        ];

        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .h(px(46.0))
            .pl(px(86.0))
            .pr(SpacePx::S24.px())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(SpacePx::S12.px())
                    .child(img(ImageSource::Image(self.logo.clone())).h(px(20.0)).w(px(78.0)).object_fit(ObjectFit::Contain))
                    .child(div().text_size(FontPx::F13.px()).text_color(p.text_primary).child("Detail Sticky Footer Comparison")),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(SpacePx::S8.px())
                    .children(kinds.into_iter().map(|kind| {
                        let active = self.ctx_kind == kind;
                        div()
                            .id(SharedString::from(format!("mode-{kind:?}")))
                            .cursor_pointer()
                            .h(RowHeightPx::H28.px())
                            .px(SpacePx::S12.px())
                            .flex()
                            .items_center()
                            .rounded(p.radius_f32(&self.glass, 0.75))
                            .bg(if active { p.active_bg } else { p.btn_bg })
                            .border_1()
                            .border_color(if active { with_alpha(p.accent, 0.45) } else { p.border_subtle })
                            .text_size(FontPx::F11.px())
                            .text_color(if active { p.accent } else { p.text_secondary })
                            .hover(|s| s.opacity(0.9))
                            .child(kind.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.ctx_kind = kind;
                                this.last_action = [None, None, None];
                                cx.notify();
                            }))
                    }))
                    .child(div().w(px(1.0)).h(px(18.0)).bg(p.border_subtle).mx(SpacePx::S4.px()))
                    .child(
                        div()
                            .id("toggle-sticky-header")
                            .cursor_pointer()
                            .h(RowHeightPx::H28.px())
                            .px(SpacePx::S12.px())
                            .flex()
                            .items_center()
                            .rounded(p.radius_f32(&self.glass, 0.75))
                            .bg(if self.sticky_header { with_alpha(p.amber, 0.16) } else { p.btn_bg })
                            .border_1()
                            .border_color(if self.sticky_header { with_alpha(p.amber, 0.45) } else { p.border_subtle })
                            .text_size(FontPx::F11.px())
                            .text_color(if self.sticky_header { p.amber } else { p.text_secondary })
                            .child(if self.sticky_header { "Sticky Header: ON" } else { "Sticky Header: OFF" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sticky_header = !this.sticky_header;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    fn render_header_block(&self, is_stopped: bool) -> AnyElement {
        let p = self.palette;
        let id_color = if is_stopped { p.coral } else { p.accent };
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
                [SharedString::from(self.ctx_kind.header_title()).into_any_element()],
            ))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(SpacePx::S12.px())
                    .child(div().text_size(FontPx::F11.px()).text_color(id_color).child(self.ctx_kind.item_id()))
                    .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child("Close")),
            )
            .into_any_element()
    }

    fn render_scroll_content(&self, is_stopped: bool) -> Vec<AnyElement> {
        let p = self.palette;
        let mut nodes = Vec::new();
        if !self.sticky_header {
            nodes.push(self.render_header_block(is_stopped));
        }
        nodes.push(
            div()
                .flex()
                .flex_col()
                .gap(SpacePx::S4.px())
                .child(div().text_size(FontPx::F13.px()).text_color(p.text_primary).child(self.ctx_kind.title_text()))
                .child(
                    div()
                        .text_size(FontPx::F11.px())
                        .text_color(if is_stopped { with_alpha(p.coral, 0.85) } else { p.text_secondary })
                        .child(self.ctx_kind.stage_label(is_stopped)),
                )
                .into_any_element(),
        );
        nodes.push(
            div()
                .text_size(FontPx::F12.px())
                .text_color(p.text_secondary)
                .child("Scroll through Step History and Artifacts while keeping governance actions pinned at the bottom.")
                .into_any_element(),
        );
        nodes.push(
            div()
                .flex()
                .flex_col()
                .gap(SpacePx::S8.px())
                .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child("Step History (6)"))
                .children(DEMO_STEPS.iter().map(|step| {
                    div()
                        .px(SpacePx::S8.px())
                        .py(SpacePx::S8.px())
                        .rounded(p.radius_f32(&self.glass, 0.65))
                        .bg(with_alpha(p.text_primary, 0.03))
                        .border_1()
                        .border_color(p.border_subtle)
                        .flex()
                        .flex_col()
                        .gap(SpacePx::S4.px())
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .child(div().text_size(FontPx::F11.px()).text_color(p.text_primary).child(format!("{} · {}", step.stage, step.sub)))
                                .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child(format!("{} · {}", step.outcome, step.age))),
                        )
                        .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child(step.field_label))
                        .child(div().text_size(FontPx::F11.px()).text_color(p.text_secondary).child(step.body))
                }))
                .into_any_element(),
        );
        if self.ctx_kind != DemoContextKind::Proposal {
            nodes.push(
                div()
                    .flex()
                    .flex_col()
                    .gap(SpacePx::S4.px())
                    .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child("Artifacts"))
                    .children(DEMO_ARTIFACTS.iter().map(|(id, label)| {
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(SpacePx::S8.px())
                            .child(div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(p.text_tertiary))
                            .child(div().text_size(FontPx::F11.px()).text_color(p.accent).child(format!("{label} ({id})")))
                    }))
                    .into_any_element(),
            );
        }
        nodes
    }

    fn render_action_buttons(&self, variant: FooterVariant, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let idx = variant.index();
        let prefix = variant.id_prefix();
        let is_stopped = self.stopped[idx];
        let in_district = matches!(self.ctx_kind, DemoContextKind::ApprovalGate | DemoContextKind::WorkStage);
        let at_approval = self.ctx_kind == DemoContextKind::ApprovalGate;
        let is_proposal = self.ctx_kind == DemoContextKind::Proposal;

        let chip = |id: String, label: &'static str, bg: Rgba, fg: Rgba, width: Option<f32>| {
            div()
                .id(SharedString::from(id))
                .cursor_pointer()
                .h(RowHeightPx::H28.px())
                .when_some(width, |d, w| d.w(px(w)))
                .when(width.is_none(), |d| d.px(SpacePx::S12.px()))
                .flex()
                .items_center()
                .justify_center()
                .rounded(p.radius_f32(&self.glass, 0.75))
                .bg(bg)
                .hover(|s| s.opacity(0.88))
                .text_size(FontPx::F12.px())
                .text_color(fg)
                .child(label)
        };

        let left_group = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(SpacePx::S8.px())
            .when(at_approval || is_proposal, |d| {
                d.child(chip(format!("{prefix}-approve"), "Approve", p.active_bg, p.accent, None).on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.last_action[idx] = Some(format!("Approved {}", this.ctx_kind.item_id()));
                        cx.notify();
                    }),
                ))
            })
            .when(in_district, |d| {
                let stop_bg = if is_stopped { p.active_bg } else { with_alpha(p.coral_inactive, 0.30) };
                let stop_fg = if is_stopped { p.accent } else { p.coral };
                d.child(
                    chip(
                        format!("{prefix}-stop"),
                        if is_stopped { "Resume" } else { "Stop" },
                        stop_bg,
                        stop_fg,
                        Some(DETAIL_TOGGLE_WIDTH_PX),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.stopped[idx] = !this.stopped[idx];
                        let verb = if this.stopped[idx] { "Stopped" } else { "Resumed" };
                        this.last_action[idx] = Some(format!("{verb} {}", this.ctx_kind.item_id()));
                        cx.notify();
                    })),
                )
                .child(chip(format!("{prefix}-rework"), "Rework", p.btn_bg, p.text_secondary, None).on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.last_action[idx] = Some(format!("Reworked {}", this.ctx_kind.item_id()));
                        cx.notify();
                    }),
                ))
            });

        let right_archive = (!is_proposal).then(|| {
            div()
                .id(SharedString::from(format!("{prefix}-archive")))
                .cursor_pointer()
                .h(RowHeightPx::H28.px())
                .px(SpacePx::S8.px())
                .flex()
                .items_center()
                .justify_center()
                .rounded(p.radius_f32(&self.glass, 0.75))
                .hover(move |s| s.bg(with_alpha(p.text_primary, 0.08)).text_color(p.text_primary))
                .text_size(FontPx::F11.px())
                .text_color(p.text_tertiary)
                .child("Archive")
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.last_action[idx] = Some(format!("Archived {}", this.ctx_kind.item_id()));
                    cx.notify();
                }))
        });

        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w_full()
            .gap(SpacePx::S8.px())
            .child(left_group)
            .when_some(right_archive, |d, btn| d.child(btn))
            .into_any_element()
    }

    fn render_column(&self, variant: FooterVariant, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let idx = variant.index();
        let prefix = variant.id_prefix();
        let is_stopped = self.stopped[idx];
        let body_nodes = self.render_scroll_content(is_stopped);
        let actions = self.render_action_buttons(variant, cx);

        let pill = match variant {
            FooterVariant::C1LiquidPopover => GlassSurface::popover(&p, &self.glass)
                .pad(SpacePx::S8)
                .gap(SpacePx::S0)
                .radius_factor(1.0)
                .border(None)
                .bg(Some(p.glass_popover))
                .accent(EdgeAccent::Neutral)
                .build()
                .child(actions),
            FooterVariant::C2DeepSmoke => GlassSurface::sheet(&p, &self.glass)
                .pad(SpacePx::S8)
                .gap(SpacePx::S0)
                .radius_factor(1.0)
                .border(None)
                .bg(Some(with_alpha(p.panel_base, 0.84)))
                .accent(EdgeAccent::Neutral)
                .build()
                .child(actions),
            FooterVariant::C3AiryCapsule => GlassSurface::popover(&p, &self.glass)
                .pad(SpacePx::S8)
                .gap(SpacePx::S0)
                .radius_factor(1.35)
                .border(None)
                .bg(Some(with_alpha(p.glass_sheet, 0.55)))
                .accent(EdgeAccent::Neutral)
                .build()
                .child(actions),
        };

        let sheet = GlassSurface::sheet(&p, &self.glass)
            .pad(SpacePx::S0)
            .gap(SpacePx::S0)
            .build()
            .relative()
            .w(px(368.0))
            .h(px(540.0))
            .overflow_hidden()
            .when(self.sticky_header, |d| {
                d.child(
                    div()
                        .px(SpacePx::S12.px())
                        .pt(SpacePx::S12.px())
                        .pb(SpacePx::S4.px())
                        .child(self.render_header_block(is_stopped)),
                )
            })
            .child(
                div()
                    .id(SharedString::from(format!("{prefix}-scroll")))
                    .flex().flex_col().flex_1().min_h(px(0.0))
                    .overflow_y_scroll().restrict_scroll_to_axis()
                    .overflow_fade_bottom(FadePx::F24)
                    .px(SpacePx::S12.px()).pt(SpacePx::S8.px()).pb(px(64.0))
                    .gap(SpacePx::S12.px())
                    .children(body_nodes),
            )
            .child(
                div()
                    .absolute()
                    .left(SpacePx::S8.px()).right(SpacePx::S8.px()).bottom(SpacePx::S8.px())
                    .child(pill),
            );

        div()
            .flex().flex_col().gap(SpacePx::S8.px()).w(px(368.0))
            .child(
                div()
                    .flex().flex_col().gap(px(2.0)).px(SpacePx::S4.px())
                    .child(
                        div()
                            .flex().flex_row().items_center().justify_between()
                            .child(div().text_size(FontPx::F13.px()).text_color(p.text_primary).child(variant.title()))
                            .when_some(self.last_action[idx].clone(), |d, msg| {
                                d.child(div().text_size(FontPx::F11.px()).text_color(p.accent).child(msg))
                            }),
                    )
                    .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child(variant.subtitle())),
            )
            .child(sheet)
            .into_any_element()
    }
}

impl Render for DetailFooterDemoApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        div()
            .relative().flex().flex_col().size_full()
            .bg(particle_wave_with(p.window_backdrop, p.wave_a, p.wave_b, self.glass.bg_brightness))
            .text_color(p.text_primary)
            .child(self.render_top_toolbar(cx))
            .child(
                div()
                    .flex().flex_row().items_start().justify_center().flex_1()
                    .px(SpacePx::S24.px()).pt(SpacePx::S12.px()).pb(SpacePx::S24.px())
                    .gap(SpacePx::S24.px())
                    .child(self.render_column(FooterVariant::C1LiquidPopover, cx))
                    .child(self.render_column(FooterVariant::C2DeepSmoke, cx))
                    .child(self.render_column(FooterVariant::C3AiryCapsule, cx)),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.activate(true);
        let bounds = Bounds::centered(None, size(px(1220.0), px(700.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_background: WindowBackgroundAppearance::Opaque,
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Afterlife · Detail Sticky Footer Demo")),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(14.0), px(14.0))),
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| DetailFooterDemoApp::new()),
        )
        .expect("open demo window");
    });
}
