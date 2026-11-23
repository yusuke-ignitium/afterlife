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
const LEFT_PANE_W: f32 = 340.0;
const MAX_COMPACT_JOBS: usize = 3;
const P3_STAGE_MIN_W: f32 = 188.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum CompactPattern { P1GlassHeaderOverflow, P2GlassBottomOverflow, P3FlatQuietStage }

impl CompactPattern {
    const ALL: [Self; 3] = [Self::P1GlassHeaderOverflow, Self::P2GlassBottomOverflow, Self::P3FlatQuietStage];
    fn id(self) -> &'static str {
        match self { Self::P1GlassHeaderOverflow => "pat-p1", Self::P2GlassBottomOverflow => "pat-p2", Self::P3FlatQuietStage => "pat-p3" }
    }
    fn label(self) -> &'static str {
        match self {
            Self::P1GlassHeaderOverflow => "P1 · Glass + Header +n",
            Self::P2GlassBottomOverflow => "P2 · Glass + Bottom +n",
            Self::P3FlatQuietStage => "P3 · Flat Stage + Header +n",
        }
    }
    fn summary(self) -> &'static str {
        match self {
            Self::P1GlassHeaderOverflow => "Fixed height (116px) · hover-only job bg · max 3 jobs + header '+n' badge",
            Self::P2GlassBottomOverflow => "Fixed height (132px) · hover-only job bg · max 3 jobs + bottom '+n more' line",
            Self::P3FlatQuietStage => "Fixed height (114px) · borderless flat stage + aligned stage width (min 188px) · max 3 jobs + '+n'",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DemoScenario { Realistic, Busy, Quiet }

impl DemoScenario {
    const ALL: [Self; 3] = [Self::Realistic, Self::Busy, Self::Quiet];
    fn id(self) -> &'static str {
        match self { Self::Realistic => "scen-realistic", Self::Busy => "scen-busy", Self::Quiet => "scen-quiet" }
    }
    fn label(self) -> &'static str {
        match self { Self::Realistic => "Realistic (8)", Self::Busy => "Busy (14)", Self::Quiet => "Quiet (0)" }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StageKind { Work, ApprovalGate, Terminal }

#[derive(Clone)]
struct DemoJob { id: &'static str, title: &'static str, sub_stage: Option<&'static str> }

#[derive(Clone)]
struct DemoStage {
    label: &'static str,
    short_label: &'static str,
    runner: &'static str,
    kind: StageKind,
    sub_sm_name: Option<&'static str>,
    sub_stages: &'static [(&'static str, &'static str)],
    jobs: Vec<DemoJob>,
}

impl DemoStage {
    fn work(label: &'static str, short_label: &'static str, sub_sm_name: Option<&'static str>, sub_stages: &'static [(&'static str, &'static str)], jobs: Vec<DemoJob>) -> Self {
        Self { label, short_label, runner: "Agent", kind: StageKind::Work, sub_sm_name, sub_stages, jobs }
    }
    fn gate(jobs: Vec<DemoJob>) -> Self {
        Self { label: "Human Gate", short_label: "Approve", runner: "Human", kind: StageKind::ApprovalGate, sub_sm_name: None, sub_stages: &[], jobs }
    }
    fn done(label: &'static str, short_label: &'static str) -> Self {
        Self { label, short_label, runner: "Done", kind: StageKind::Terminal, sub_sm_name: None, sub_stages: &[], jobs: Vec::new() }
    }
}

#[derive(Clone)]
struct DemoDistrict { id: &'static str, name: &'static str, version: &'static str, stages: Vec<DemoStage> }

impl DemoDistrict {
    fn active_count(&self) -> usize { self.stages.iter().map(|s| s.jobs.len()).sum() }
}

fn job(id: &'static str, title: &'static str, sub_stage: Option<&'static str>) -> DemoJob {
    DemoJob { id, title, sub_stage }
}

fn build_districts(scenario: DemoScenario) -> Vec<DemoDistrict> {
    const SM_GUARD: &[(&str, &str)] = &[("capture_base", "Capture Baseline"), ("lock_guard", "Lock Guard")];
    const SM_BUILD: &[(&str, &str)] = &[("apply_change", "Apply Change"), ("edge_tests", "Harden Tests"), ("verify_loop", "Zero-Warn Verify")];
    const SM_SHIP: &[(&str, &str)] = &[("env_check", "Real-Env Check"), ("compare", "Before/After")];
    const SM_EVIDENCE: &[(&str, &str)] = &[("trace", "Trace Sources"), ("options", "Evaluate Options"), ("criteria", "Lock Criteria")];
    const SM_REPRO: &[(&str, &str)] = &[("red_repro", "Lock Repro Test"), ("blast_radius", "Map Blast Radius")];
    const SM_PERF: &[(&str, &str)] = &[("profile", "Profile Bottleneck"), ("optimize", "Principled Fix"), ("remeasure", "Remeasure")];

    let (mut d1p, mut d1b, mut d1v, mut d1a) = (vec![], vec![], vec![], vec![]);
    let (mut d2s, mut d2a, mut d3r, mut d4p, mut d5p) = (vec![], vec![], vec![], vec![], vec![]);

    match scenario {
        DemoScenario::Realistic => {
            d1b.push(job("JOB-014", "District Create / Archive / Restore", Some("apply_change")));
            d1b.push(job("JOB-017", "Compact Mode Toggle Persistence", Some("edge_tests")));
            d1b.push(job("JOB-018", "Sub-SM Active Job Indicator", Some("verify_loop")));
            d1b.push(job("JOB-019", "Keyboard Shortcuts for Quick Nav", Some("apply_change")));
            d1b.push(job("JOB-020", "Artifact Diff Preview Highlight", Some("edge_tests")));
            d1a.push(job("JOB-012", "Detail Sticky Footer (C1 Pill)", None));
            d2s.push(job("JOB-016", "External CI Blocker Phase 2 Shaping", Some("options")));
            d4p.push(job("JOB-015", "Test Suite Wall-Clock Optimization", Some("profile")));
        }
        DemoScenario::Busy => {
            d1p.push(job("JOB-021", "Command Palette Interaction Spec", Some("lock_guard")));
            d1b.push(job("JOB-014", "District Create / Archive / Restore", Some("apply_change")));
            d1b.push(job("JOB-017", "Compact Mode Toggle Persistence", Some("edge_tests")));
            d1b.push(job("JOB-018", "Sub-SM Active Job Indicator", Some("verify_loop")));
            d1b.push(job("JOB-019", "Keyboard Shortcuts for Quick Nav", Some("apply_change")));
            d1b.push(job("JOB-020", "Artifact Diff Preview Highlight", Some("edge_tests")));
            d1v.push(job("JOB-013", "Fade UI Transparency Slider (0-90%)", Some("compare")));
            d1a.push(job("JOB-012", "Detail Sticky Footer (C1 Pill)", None));
            d1a.push(job("JOB-022", "Approval Batch Quick Confirm", None));
            d1a.push(job("JOB-023", "Workspace Filter Persistence", None));
            d1a.push(job("JOB-024", "Stage Overflow Tooltip", None));
            d2a.push(job("JOB-016", "External CI Blocker Phase 2", None));
            d3r.push(job("JOB-025", "Fix Multi-Display Window Restore", Some("red_repro")));
            d4p.push(job("JOB-015", "Test Suite Wall-Clock Optimization", Some("profile")));
            d5p.push(job("JOB-026", "Split engine/mod.rs Below 650 Lines", Some("apply_change")));
        }
        DemoScenario::Quiet => {}
    }

    vec![
        DemoDistrict {
            id: "DIST-001",
            name: "DISTRICT 01 - Product Delivery",
            version: "Philosophy v1 · Spec v1",
            stages: vec![
                DemoStage::work("Plan & Guard", "Plan", Some("Baseline & Guard"), SM_GUARD, d1p),
                DemoStage::work("Build & Hygiene", "Build", Some("Build & Guard Loop"), SM_BUILD, d1b),
                DemoStage::work("Verify & Evidence", "Verify", Some("Verify & Ship"), SM_SHIP, d1v),
                DemoStage::gate(d1a),
                DemoStage::done("Shipped", "Shipped"),
            ],
        },
        DemoDistrict {
            id: "DIST-002",
            name: "DISTRICT 02 - Discovery & Shaping",
            version: "Philosophy v2 · Spec v2",
            stages: vec![
                DemoStage::work("Recon", "Recon", Some("Evidence Gathering"), SM_EVIDENCE, vec![]),
                DemoStage::work("Options", "Options", None, &[], vec![]),
                DemoStage::work("Shape Spec", "Shape", Some("Evidence Gathering"), SM_EVIDENCE, d2s),
                DemoStage::gate(d2a),
                DemoStage::done("Shaped", "Shaped"),
            ],
        },
        DemoDistrict {
            id: "DIST-003",
            name: "DISTRICT 03 - Root-Cause Fix",
            version: "Philosophy v1 · Spec v1",
            stages: vec![
                DemoStage::work("Red Repro", "Repro", Some("Red Repro Lock"), SM_REPRO, d3r),
                DemoStage::work("Root Fix", "Fix", Some("Build & Guard Loop"), SM_BUILD, vec![]),
                DemoStage::gate(vec![]),
                DemoStage::done("Resolved", "Done"),
            ],
        },
        DemoDistrict {
            id: "DIST-004",
            name: "DISTRICT 04 - Perf & Efficiency",
            version: "Philosophy v1 · Spec v1",
            stages: vec![
                DemoStage::work("Observe & Metric", "Observe", Some("Perf Loop"), SM_PERF, d4p),
                DemoStage::work("Optimize", "Optimize", Some("Verify & Ship"), SM_SHIP, vec![]),
                DemoStage::gate(vec![]),
                DemoStage::done("Verified", "Done"),
            ],
        },
        DemoDistrict {
            id: "DIST-005",
            name: "DISTRICT 05 - Prune & Hardening",
            version: "Philosophy v1 · Spec v1",
            stages: vec![
                DemoStage::work("Prune & Split", "Prune", Some("Build & Guard Loop"), SM_BUILD, d5p),
                DemoStage::gate(vec![]),
                DemoStage::done("Hardened", "Done"),
            ],
        },
    ]
}

struct DistrictCompactDemoApp {
    glass: GlassSettings,
    palette: Palette,
    logo: Arc<Image>,
    compact: bool,
    pattern: CompactPattern,
    scenario: DemoScenario,
    selected_job_id: &'static str,
    stopped_jobs: Vec<&'static str>,
}

impl DistrictCompactDemoApp {
    fn new() -> Self {
        let glass = GlassSettings::default();
        let palette = Palette::from_style(UiStyle::LiquidGlass, &glass);
        let logo = Arc::new(Image::from_bytes(ImageFormat::Png, APP_TITLE_LOGO_PNG.to_vec()));
        Self {
            glass, palette, logo, compact: true,
            pattern: CompactPattern::P1GlassHeaderOverflow,
            scenario: DemoScenario::Realistic,
            selected_job_id: "JOB-014",
            stopped_jobs: vec!["JOB-018", "JOB-025"],
        }
    }

    fn is_stopped(&self, id: &str) -> bool { self.stopped_jobs.contains(&id) }
    fn toggle_stop(&mut self, id: &'static str) {
        if let Some(pos) = self.stopped_jobs.iter().position(|&s| s == id) { self.stopped_jobs.remove(pos); } else { self.stopped_jobs.push(id); }
    }

    fn chip(&self, id: &'static str, label: &'static str, active: bool, on_click: impl Fn(&mut Self) + 'static, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        div()
            .id(id).cursor_pointer().h(RowHeightPx::H24.px()).px(SpacePx::S8.px())
            .flex().items_center().rounded(p.radius_f32(&self.glass, 0.7))
            .bg(if active { p.active_bg } else { with_alpha(p.text_primary, 0.05) })
            .hover(move |s| s.bg(if active { p.active_bg } else { with_alpha(p.text_primary, 0.11) }))
            .text_size(FontPx::F11.px()).text_color(if active { p.accent } else { p.text_secondary })
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| { on_click(this); cx.notify(); }))
            .into_any_element()
    }

    fn render_top_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        div()
            .flex().flex_row().items_center().justify_between()
            .px(SpacePx::S16.px()).pt(px(34.0)).pb(SpacePx::S8.px()).gap(SpacePx::S12.px())
            .child(
                div().flex().flex_row().items_center().gap(SpacePx::S12.px())
                    .child(img(ImageSource::Image(self.logo.clone())).w(px(124.0)).h(px(30.0)).object_fit(ObjectFit::Contain))
                    .child(div().text_size(FontPx::F12.px()).text_color(p.text_tertiary).child("Districts Compact Mode · 3 Fixed-Height Patterns")),
            )
            .child(
                div().flex().flex_row().items_center().gap(SpacePx::S8.px())
                    .children(CompactPattern::ALL.into_iter().map(|pat| {
                        self.chip(pat.id(), pat.label(), self.pattern == pat, move |a| {
                            a.pattern = pat;
                            a.compact = true;
                        }, cx)
                    }))
                    .child(div().w(px(1.0)).h(px(16.0)).bg(with_alpha(p.text_primary, 0.14)))
                    .children(DemoScenario::ALL.into_iter().map(|s| {
                        self.chip(s.id(), s.label(), self.scenario == s, move |a| a.scenario = s, cx)
                    })),
            )
            .into_any_element()
    }

    fn render_left_pane(&self, districts: &[DemoDistrict], cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let all_jobs: Vec<(&DemoDistrict, &DemoStage, &DemoJob)> = districts
            .iter()
            .flat_map(|d| d.stages.iter().flat_map(move |s| s.jobs.iter().map(move |j| (d, s, j))))
            .collect();
        let approvals: Vec<_> = all_jobs.iter().filter(|(_, s, _)| s.kind == StageKind::ApprovalGate).collect();

        GlassSurface::panel(&p, &self.glass)
            .pad(SpacePx::S16).gap(SpacePx::S12).build()
            .w(px(LEFT_PANE_W)).min_w(px(LEFT_PANE_W)).h_full().flex_shrink_0()
            .child(
                div().flex().flex_row().items_center().justify_between()
                    .child(div().text_size(FontPx::F14.px()).text_color(p.text_primary).child("Workspace Queue"))
                    .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child(format!("{} active", all_jobs.len()))),
            )
            .when(!approvals.is_empty(), |d| {
                d.child(
                    div().flex().flex_col().gap(SpacePx::S8.px())
                        .child(div().text_size(FontPx::F11.px()).text_color(p.amber).child(format!("Approvals ({})", approvals.len())))
                        .children(approvals.into_iter().take(2).map(|(_, _, job)| {
                            let jid = job.id;
                            let sel = self.selected_job_id == jid;
                            GlassSurface::card(&p, &self.glass)
                                .pad(SpacePx::S8).gap(SpacePx::S4)
                                .border(Some(if sel { p.accent } else { p.amber }))
                                .bg(Some(p.glass_gate)).accent(EdgeAccent::Gate).build()
                                .id(SharedString::from(format!("left-appr-{jid}"))).cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| { this.selected_job_id = jid; cx.notify(); }))
                                .child(div().text_size(FontPx::F11.px()).text_color(p.amber).child(jid))
                                .child(div().text_size(FontPx::F12.px()).text_color(p.text_primary).child(job.title))
                        })),
                )
            })
            .child(
                div().id("left-mock-jobs").flex().flex_col().flex_1().min_h(px(0.0))
                    .overflow_y_scroll().restrict_scroll_to_axis().overflow_fade_bottom(FadePx::F16).gap(SpacePx::S8.px())
                    .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child("In-District Jobs"))
                    .when(all_jobs.is_empty(), |d| {
                        d.child(div().text_size(FontPx::F12.px()).text_color(p.text_tertiary).child("No active jobs in Quiet scenario"))
                    })
                    .children(all_jobs.into_iter().map(|(dist, stg, job)| {
                        let jid = job.id;
                        let sel = self.selected_job_id == jid;
                        let stopped = self.is_stopped(jid);
                        let b = if stopped { with_alpha(p.coral, 0.5) } else if sel { p.accent } else { p.border_rim };
                        GlassSurface::card(&p, &self.glass)
                            .pad(SpacePx::S8).gap(SpacePx::S4).border(Some(b))
                            .bg(Some(if sel { p.glass_selected } else { p.glass_card }))
                            .accent(if sel { EdgeAccent::Selected } else { EdgeAccent::Neutral }).build()
                            .id(SharedString::from(format!("left-job-{jid}"))).cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| { this.selected_job_id = jid; cx.notify(); }))
                            .child(
                                div().flex().flex_row().items_center().justify_between()
                                    .child(div().text_size(FontPx::F11.px()).text_color(if stopped { p.coral } else if sel { p.accent } else { p.text_secondary }).child(jid))
                                    .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child(format!("{} · {}", dist.id, stg.short_label))),
                            )
                            .child(div().text_size(FontPx::F12.px()).text_color(p.text_primary).child(job.title))
                    })),
            )
            .into_any_element()
    }

    fn render_board_header(&self, total_active: usize, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let mode_pill = GlassSurface::card(&p, &self.glass)
            .pad(SpacePx::S0).gap(SpacePx::S0).radius_factor(0.85)
            .border(Some(p.border_rim)).bg(Some(with_alpha(p.stage_base, 0.72))).build()
            .p(px(2.0)).flex().flex_row().items_center().gap(px(2.0))
            .child(self.chip("mode-std", "Standard", !self.compact, |a| a.compact = false, cx))
            .child(self.chip("mode-cmp", "Compact", self.compact, |a| a.compact = true, cx));
        let desc = if self.compact {
            self.pattern.summary()
        } else {
            "Current production layout · full Sub-SM lists expanded + horizontal stage scroll"
        };

        div().flex().flex_row().items_center().justify_between().gap(SpacePx::S12.px()).px(SpacePx::S4.px())
            .child(
                div().flex().flex_col().gap(px(2.0))
                    .child(
                        div().flex().flex_row().items_center().gap(SpacePx::S8.px())
                            .child(div().text_size(FontPx::F13.px()).text_color(p.text_primary).child("Districts"))
                            .child(div().text_size(FontPx::F11.px()).text_color(p.accent).child(format!("5 districts · {total_active} active"))),
                    )
                    .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child(desc)),
            )
            .child(mode_pill)
            .into_any_element()
    }

    fn stage_color(&self, kind: StageKind) -> Rgba {
        match kind {
            StageKind::ApprovalGate => self.palette.amber,
            StageKind::Terminal => self.palette.green,
            StageKind::Work => self.palette.text_primary,
        }
    }

    fn render_compact_job_row(&self, job: &DemoJob, is_gate: bool, row_h: f32, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let jid = job.id;
        let sel = self.selected_job_id == jid;
        let stopped = self.is_stopped(jid);
        let id_color = if stopped { p.coral } else if is_gate { p.amber } else if sel { p.accent } else { p.text_secondary };
        let hover_bg = if stopped { with_alpha(p.coral_inactive, 0.24) }
            else if is_gate { with_alpha(p.amber_inactive, 0.22) }
            else { with_alpha(p.text_primary, 0.09) };
        let grp = SharedString::from(format!("cmp-jgrp-{jid}"));

        div()
            .id(SharedString::from(format!("cmp-job-{jid}"))).group(grp.clone()).cursor_pointer()
            .h(px(row_h)).px(SpacePx::S4.px()).rounded(p.radius_f32(&self.glass, 0.55))
            .hover(move |s| s.bg(hover_bg))
            .flex().flex_row().items_center().justify_between().gap(SpacePx::S4.px())
            .on_click(cx.listener(move |this, _, _, cx| { this.selected_job_id = jid; cx.notify(); }))
            .child(
                div().w(px(2.0)).h(px(11.0)).rounded(px(1.0)).flex_shrink_0()
                    .bg(if sel { p.accent } else if stopped { with_alpha(p.coral, 0.7) } else { with_alpha(p.text_primary, 0.0) }),
            )
            .child(div().text_size(FontPx::F11.px()).text_color(id_color).flex_shrink_0().child(jid))
            .child(
                div().flex_1().min_w(px(0.0)).overflow_hidden().whitespace_nowrap().text_ellipsis()
                    .text_size(FontPx::F11.px())
                    .text_color(if stopped { p.text_secondary } else if sel { p.text_primary } else { with_alpha(p.text_primary, 0.86) })
                    .child(job.title),
            )
            .child(
                div().id(SharedString::from(format!("cmp-stop-{jid}"))).cursor_pointer().px(px(4.0)).rounded(px(3.0))
                    .flex_shrink_0()
                    .when(!stopped, |d| d.opacity(0.0).group_hover(grp, |s| s.opacity(1.0)))
                    .hover(move |s| s.bg(with_alpha(if stopped { p.accent_inactive } else { p.coral_inactive }, 0.30)))
                    .text_size(FontPx::F11.px()).text_color(if stopped { p.coral } else { p.text_tertiary })
                    .child(if stopped { "Resume" } else { "Stop" })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_stop(jid);
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    fn render_standard_job_card(&self, job: &DemoJob, is_gate: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let jid = job.id;
        let sel = self.selected_job_id == jid;
        let stopped = self.is_stopped(jid);
        let bg = if stopped { with_alpha(p.coral_inactive, if sel { 0.22 } else { 0.13 }) }
            else if is_gate { p.glass_gate } else if sel { p.glass_selected } else { p.glass_card };
        let border = if stopped { with_alpha(p.coral, if sel { 0.72 } else { 0.30 }) }
            else if sel { p.accent } else if is_gate { p.amber } else { p.border_rim };
        let id_color = if stopped { p.coral } else if is_gate { p.amber } else if sel { p.accent } else { p.text_secondary };
        let sub_tag = job.sub_stage.map(|s| format!(" ({s})")).unwrap_or_default();
        let grp = SharedString::from(format!("std-jgrp-{jid}"));

        GlassSurface::card(&p, &self.glass)
            .pad(SpacePx::S8).gap(SpacePx::S4).border(Some(border)).bg(Some(bg))
            .accent(if stopped || is_gate { EdgeAccent::Gate } else { EdgeAccent::Selected })
            .build().id(SharedString::from(format!("std-job-{jid}"))).group(grp.clone()).cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| { this.selected_job_id = jid; cx.notify(); }))
            .child(
                div().flex().flex_row().items_center().justify_between().gap(SpacePx::S4.px())
                    .child(div().text_size(FontPx::F11.px()).text_color(id_color).child(format!("{jid}{sub_tag}")))
                    .child(
                        div().id(SharedString::from(format!("std-stop-{jid}"))).cursor_pointer().px(SpacePx::S4.px()).rounded(px(4.0))
                            .when(!stopped, |d| d.opacity(0.0).group_hover(grp, |s| s.opacity(1.0)))
                            .hover(move |s| s.bg(with_alpha(if stopped { p.accent_inactive } else { p.coral_inactive }, 0.30)))
                            .text_size(FontPx::F11.px()).text_color(if stopped { p.coral } else { p.text_secondary })
                            .child(if stopped { "Resume" } else { "Stop" })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.toggle_stop(jid);
                                cx.notify();
                            })),
                    ),
            )
            .child(div().text_size(FontPx::F12.px()).text_color(if stopped { p.text_secondary } else { p.text_primary }).child(job.title))
            .into_any_element()
    }

    fn render_district_standard(&self, dist: &DemoDistrict, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        GlassSurface::panel(&p, &self.glass)
            .pad(SpacePx::S16).gap(SpacePx::S12).bg(Some(p.panel_base)).build().flex_shrink_0()
            .child(
                div().flex().flex_row().items_center().justify_between()
                    .child(
                        div().flex().flex_row().items_center().gap(SpacePx::S12.px())
                            .child(single_line(RowHeightPx::H28, SpacePx::S0, SpacePx::S4, FontPx::F14, p.radius(RadiusPx::R4), None, None, p.text_primary, false, p.centerline, [SharedString::from(dist.name).into_any_element()]))
                            .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child(dist.version)),
                    )
                    .child(div().text_size(FontPx::F11.px()).text_color(p.accent).child(format!("{} active", dist.active_count()))),
            )
            .child(
                div().id(SharedString::from(format!("std-scroll-{}", dist.id)))
                    .flex().flex_row().items_start().gap(SpacePx::S12.px()).w_full().min_w(px(0.0))
                    .overflow_x_scroll().restrict_scroll_to_axis()
                    .children(dist.stages.iter().enumerate().map(|(i, stg)| {
                        let is_gate = stg.kind == StageKind::ApprovalGate;
                        GlassSurface::card(&p, &self.glass)
                            .pad(SpacePx::S12).gap(SpacePx::S12).radius_factor(1.25)
                            .border(Some(if is_gate { p.amber } else { p.border_rim }))
                            .bg(Some(if is_gate { p.glass_gate } else { p.stage_base }))
                            .accent(if is_gate { EdgeAccent::Gate } else { EdgeAccent::Neutral }).build()
                            .w(px(268.0)).min_w(px(268.0)).min_h(px(148.0)).flex_shrink_0()
                            .child(
                                div().flex().flex_col().gap(SpacePx::S4.px())
                                    .child(
                                        div().flex().flex_row().items_center().justify_between()
                                            .child(div().text_size(FontPx::F13.px()).text_color(self.stage_color(stg.kind)).child(format!("{}. {}", i + 1, stg.label)))
                                            .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child(stg.runner)),
                                    )
                                    .when_some(stg.sub_sm_name, |d, sm| {
                                        d.child(div().text_size(FontPx::F11.px()).text_color(p.purple).child(format!("• Sub-SM: {sm}")))
                                    }),
                            )
                            .when(!stg.sub_stages.is_empty(), |d| {
                                d.child(
                                    div().flex().flex_col().pl(SpacePx::S8.px()).border_l(px(1.0))
                                        .border_color(with_alpha(p.purple_inactive, 0.55)).gap(SpacePx::S4.px())
                                        .children(stg.sub_stages.iter().map(|(sid, lbl)| {
                                            let on = stg.jobs.iter().any(|j| j.sub_stage == Some(*sid));
                                            div().flex().flex_row().items_center().gap(SpacePx::S4.px())
                                                .child(div().w(px(5.0)).h(px(5.0)).rounded(px(2.5)).bg(if on { p.purple } else { p.text_tertiary }))
                                                .child(div().text_size(FontPx::F11.px()).text_color(if on { p.purple } else { p.text_secondary }).child(*lbl))
                                        })),
                                )
                            })
                            .children(stg.jobs.iter().map(|j| self.render_standard_job_card(j, is_gate, cx)))
                    })),
            )
            .into_any_element()
    }

    fn p3_stage_width(window_width: f32, max_stages: usize) -> f32 {
        let cols = max_stages.max(1) as f32;
        let right_pane_w = (window_width - LEFT_PANE_W - SpacePx::S16.val() * 3.0).max(0.0);
        let district_inner_w = (right_pane_w - SpacePx::S8.val() * 2.0).max(0.0);
        let total_gaps = SpacePx::S8.val() * (cols - 1.0);
        ((district_inner_w - total_gaps) / cols).floor().max(P3_STAGE_MIN_W)
    }

    fn render_compact_stage(&self, idx: usize, stg: &DemoStage, p3_stage_w: f32, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let is_gate = stg.kind == StageKind::ApprovalGate;
        let overflow = stg.jobs.len().saturating_sub(MAX_COMPACT_JOBS);
        let pat = self.pattern;
        let show_header_overflow = overflow > 0 && pat != CompactPattern::P2GlassBottomOverflow;
        let show_bottom_overflow = overflow > 0 && pat == CompactPattern::P2GlassBottomOverflow;
        let stage_h = match pat {
            CompactPattern::P1GlassHeaderOverflow => 116.0,
            CompactPattern::P2GlassBottomOverflow => 132.0,
            CompactPattern::P3FlatQuietStage => 114.0,
        };
        let row_h = if pat == CompactPattern::P2GlassBottomOverflow { 22.0 } else { 24.0 };

        let header = div().flex().flex_row().items_center().justify_between().gap(SpacePx::S4.px()).h(px(18.0)).flex_shrink_0()
            .child(
                div().flex_1().min_w(px(0.0)).overflow_hidden().whitespace_nowrap().text_ellipsis()
                    .text_size(FontPx::F11.px()).text_color(self.stage_color(stg.kind))
                    .child(format!("{}. {}", idx + 1, stg.short_label)),
            )
            .child(
                div().flex().flex_row().items_center().gap(SpacePx::S4.px()).flex_shrink_0()
                    .when(!stg.sub_stages.is_empty(), |d| {
                        d.child(
                            div().flex().flex_row().items_center().gap(px(3.0))
                                .children(stg.sub_stages.iter().map(|(sid, _)| {
                                    let on = stg.jobs.iter().any(|j| j.sub_stage == Some(*sid));
                                    div().w(px(5.0)).h(px(5.0)).rounded(px(2.5)).bg(if on { p.purple } else { with_alpha(p.text_tertiary, 0.38) })
                                })),
                        )
                    })
                    .when(show_header_overflow, |d| {
                        d.child(
                            div().px(px(5.0)).h(px(16.0)).rounded(px(8.0))
                                .bg(with_alpha(if is_gate { p.amber } else { p.accent }, 0.16))
                                .flex().items_center()
                                .text_size(FontPx::F11.px()).text_color(if is_gate { p.amber } else { p.accent })
                                .child(format!("+{overflow}")),
                        )
                    }),
            );

        let jobs_box = div().flex().flex_col().flex_1().min_h(px(0.0)).gap(px(2.0)).overflow_hidden()
            .children(stg.jobs.iter().take(MAX_COMPACT_JOBS).map(|j| self.render_compact_job_row(j, is_gate, row_h, cx)))
            .when(show_bottom_overflow, |d| {
                d.child(
                    div().h(px(18.0)).px(SpacePx::S8.px()).flex().items_center()
                        .text_size(FontPx::F11.px()).text_color(if is_gate { p.amber } else { p.text_tertiary })
                        .child(format!("+{overflow} more")),
                )
            });

        if pat == CompactPattern::P3FlatQuietStage {
            let flat_bg = if is_gate { with_alpha(p.amber_inactive, if stg.jobs.is_empty() { 0.06 } else { 0.13 }) } else { with_alpha(p.text_primary, 0.032) };
            return div()
                .flex().flex_col()
                .w(px(p3_stage_w)).min_w(px(p3_stage_w)).max_w(px(p3_stage_w)).flex_shrink_0()
                .h(px(stage_h)).overflow_hidden()
                .p(SpacePx::S8.px()).gap(SpacePx::S4.px()).rounded(p.radius_f32(&self.glass, 0.85)).bg(flat_bg)
                .child(header).child(jobs_box).into_any_element();
        }

        let border = if is_gate && !stg.jobs.is_empty() { p.amber } else if is_gate { with_alpha(p.amber, 0.45) } else { p.border_rim };
        GlassSurface::card(&p, &self.glass)
            .pad(SpacePx::S8).gap(SpacePx::S4).radius_factor(1.0)
            .border(Some(border)).bg(Some(if is_gate { p.glass_gate } else { p.stage_base }))
            .accent(if is_gate { EdgeAccent::Gate } else { EdgeAccent::Neutral }).build()
            .flex_1().min_w(px(0.0)).h(px(stage_h)).overflow_hidden()
            .child(header)
            .child(jobs_box)
            .into_any_element()
    }

    fn render_district_compact(&self, dist: &DemoDistrict, p3_stage_w: f32, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let active = dist.active_count();
        let stages_row = if self.pattern == CompactPattern::P3FlatQuietStage {
            div()
                .id(SharedString::from(format!("cmp-scroll-{}", dist.id)))
                .flex().flex_row().items_start().gap(SpacePx::S8.px()).w_full().min_w(px(0.0))
                .overflow_x_scroll().restrict_scroll_to_axis()
                .children(dist.stages.iter().enumerate().map(|(i, stg)| self.render_compact_stage(i, stg, p3_stage_w, cx)))
                .into_any_element()
        } else {
            div()
                .flex().flex_row().items_start().gap(SpacePx::S8.px()).w_full()
                .children(dist.stages.iter().enumerate().map(|(i, stg)| self.render_compact_stage(i, stg, p3_stage_w, cx)))
                .into_any_element()
        };

        GlassSurface::panel(&p, &self.glass)
            .pad(SpacePx::S8).gap(SpacePx::S8).bg(Some(p.panel_base)).build().w_full().min_w(px(0.0)).flex_shrink_0()
            .child(
                div().flex().flex_row().items_center().justify_between().px(SpacePx::S4.px())
                    .child(div().text_size(FontPx::F12.px()).text_color(p.text_primary).child(dist.name))
                    .child(div().text_size(FontPx::F11.px()).text_color(if active > 0 { p.accent } else { p.text_tertiary }).child(format!("{active} active"))),
            )
            .child(stages_row)
            .into_any_element()
    }
}

impl Render for DistrictCompactDemoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let districts = build_districts(self.scenario);
        let total_active: usize = districts.iter().map(DemoDistrict::active_count).sum();
        let max_stages = districts.iter().map(|d| d.stages.len()).max().unwrap_or(1);
        let window_width = f32::from(window.viewport_size().width);
        let p3_stage_w = Self::p3_stage_width(window_width, max_stages);

        div()
            .relative().flex().flex_col().size_full()
            .bg(particle_wave_with(p.window_backdrop, p.wave_a, p.wave_b, self.glass.bg_brightness))
            .text_color(p.text_primary)
            .child(self.render_top_bar(cx))
            .child(
                div().flex().flex_row().flex_1().min_h(px(0.0)).px(SpacePx::S16.px()).pb(SpacePx::S16.px()).gap(SpacePx::S16.px())
                    .child(self.render_left_pane(&districts, cx))
                    .child(
                        div().flex().flex_col().flex_1().min_w(px(0.0)).h_full().min_h(px(0.0)).gap(SpacePx::S8.px())
                            .child(self.render_board_header(total_active, cx))
                            .child(
                                div().id("districts-demo-scroll").flex().flex_col().flex_1().min_h(px(0.0))
                                    .overflow_y_scroll().restrict_scroll_to_axis().overflow_fade_bottom(FadePx::F24)
                                    .gap(if self.compact { SpacePx::S12.px() } else { SpacePx::S24.px() })
                                    .children(districts.iter().map(|d| {
                                        if self.compact {
                                            self.render_district_compact(d, p3_stage_w, cx)
                                        } else {
                                            self.render_district_standard(d, cx)
                                        }
                                    })),
                            ),
                    ),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.activate(true);
        let bounds = Bounds::centered(None, size(px(1380.0), px(860.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_background: WindowBackgroundAppearance::Opaque,
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Afterlife · Districts Compact Mode Demo")),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(14.0), px(14.0))),
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| DistrictCompactDemoApp::new()),
        )
        .expect("open demo window");
    });
}
