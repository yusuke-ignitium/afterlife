mod policy;

use std::time::{Duration, Instant};

use afterlife_design::{
    EdgeAccent, FontPx, GlassSettings, GlassSurface, Palette, RadiusPx, RowHeightPx, SpacePx,
    UiStyle, single_line, with_alpha,
};
use gpui::{
    AnyElement, App, AppContext, Application, Bounds, Context, InteractiveElement, IntoElement,
    ParentElement, Render, ScrollHandle, ScrollWheelEvent, SharedString,
    StatefulInteractiveElement, Styled, TitlebarOptions, Window, WindowBackgroundAppearance,
    WindowBounds, WindowOptions, div, particle_wave_with, point, prelude::FluentBuilder, px, size,
};

use crate::policy::{ScrollAxis, ScrollAxisRouter, ScrollMode, phase_label};

struct DemoDistrict {
    name: &'static str,
    spec: &'static str,
    stages: &'static [(&'static str, &'static [&'static str], &'static [&'static str])],
}

const DISTRICTS: &[DemoDistrict] = &[
    DemoDistrict {
        name: "Product Delivery",
        spec: "Philosophy v4 · Spec v7",
        stages: &[
            ("1. Discover & Probe", &["Trace Sources", "Isolate Probe", "Lock Criteria"], &["JOB-051 Auth refresh"]),
            ("2. Design Whole", &[], &["JOB-052 Billing flow"]),
            ("3. Build & Verify", &["Implement", "Run Guards", "Smoke Test"], &[]),
            ("4. Human Gate", &[], &["JOB-049 PR #1079"]),
            ("5. Release Prep", &["Sign Bundle", "Verify Notes"], &[]),
            ("6. Rollout", &[], &[]),
            ("7. Shipped", &[], &["JOB-044 Scroll fix"]),
        ],
    },
    DemoDistrict {
        name: "Platform Core",
        spec: "Philosophy v2 · Spec v5",
        stages: &[
            ("1. Intake & Triage", &["Repro", "Scope"], &["JOB-061 Metal pass"]),
            ("2. Architecture", &[], &[]),
            ("3. Execute", &["Code", "Bench", "Guard"], &["JOB-062 Blit trim"]),
            ("4. Security Review", &[], &[]),
            ("5. Approval Gate", &[], &["JOB-058 Follow-up"]),
            ("6. Canary", &[], &[]),
            ("7. Shipped", &[], &[]),
        ],
    },
    DemoDistrict {
        name: "Design Systems",
        spec: "Philosophy v3 · Spec v4",
        stages: &[
            ("1. Token Audit", &["Spacing", "Glass", "Motion"], &[]),
            ("2. Prototype", &[], &["JOB-071 Axis demo"]),
            ("3. Componentize", &["Primitives", "Tests"], &[]),
            ("4. Visual Sign-off", &[], &[]),
            ("5. Publish", &[], &[]),
            ("6. Adopt", &[], &[]),
            ("7. Shipped", &[], &[]),
        ],
    },
    DemoDistrict {
        name: "Incident Response",
        spec: "Philosophy v1 · Spec v3",
        stages: &[
            ("1. Detect", &["Alert", "Correlate"], &[]),
            ("2. Mitigate", &[], &[]),
            ("3. Root Cause", &["Timeline", "Fix", "Guard"], &[]),
            ("4. Postmortem Gate", &[], &[]),
            ("5. Hardening", &[], &[]),
            ("6. Verify", &[], &[]),
            ("7. Closed", &[], &[]),
        ],
    },
    DemoDistrict {
        name: "Growth Experiments",
        spec: "Philosophy v2 · Spec v2",
        stages: &[
            ("1. Hypothesis", &["Baseline", "Metric"], &[]),
            ("2. Build Variant", &[], &[]),
            ("3. Run Cohort", &[], &[]),
            ("4. Readout Gate", &[], &[]),
            ("5. Graduate", &[], &[]),
            ("6. Cleanup", &[], &[]),
            ("7. Done", &[], &[]),
        ],
    },
    DemoDistrict {
        name: "Release Engineering",
        spec: "Philosophy v3 · Spec v6",
        stages: &[
            ("1. Cut Branch", &["Changelog", "Tag"], &[]),
            ("2. Build Matrix", &[], &[]),
            ("3. Codesign", &["Notarize", "Staple"], &[]),
            ("4. Ship Gate", &[], &[]),
            ("5. Distribute", &[], &[]),
            ("6. Monitor", &[], &[]),
            ("7. Archived", &[], &[]),
        ],
    },
];

struct ScrollAxisDemoApp {
    glass: GlassSettings,
    palette: Palette,
    mode: ScrollMode,
    router: ScrollAxisRouter,
    vert_handle: ScrollHandle,
    row_handles: Vec<ScrollHandle>,
    last_row_idx: Option<usize>,
}

impl ScrollAxisDemoApp {
    fn new() -> Self {
        let glass = GlassSettings::default();
        let palette = Palette::from_style(UiStyle::LiquidGlass, &glass);
        let row_handles = (0..DISTRICTS.len()).map(|_| ScrollHandle::new()).collect();
        Self {
            glass,
            palette,
            mode: ScrollMode::OsGestureLock,
            router: ScrollAxisRouter::new(),
            vert_handle: ScrollHandle::new(),
            row_handles,
            last_row_idx: None,
        }
    }

    fn apply_scroll_event(
        &mut self,
        row_idx: Option<usize>,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(window.line_height());
        let decision = self.router.route(
            self.mode,
            event.gesture_phase,
            f32::from(delta.x),
            f32::from(delta.y),
            Instant::now(),
        );
        self.last_row_idx = row_idx;

        if decision.applied_x != 0.0
            && let Some(idx) = row_idx
            && let Some(handle) = self.row_handles.get(idx)
        {
            let cur = handle.offset();
            let max_x = f32::from(handle.max_offset().width).max(0.0);
            let next_x = (f32::from(cur.x) + decision.applied_x).clamp(-max_x, 0.0);
            handle.set_offset(point(px(next_x), cur.y));
        }

        if decision.applied_y != 0.0 {
            let cur = self.vert_handle.offset();
            let max_y = f32::from(self.vert_handle.max_offset().height).max(0.0);
            let next_y = (f32::from(cur.y) + decision.applied_y).clamp(-max_y, 0.0);
            self.vert_handle.set_offset(point(cur.x, px(next_y)));
        }

        cx.notify();
    }

    fn reset_all_scroll(&mut self, cx: &mut Context<Self>) {
        self.vert_handle.set_offset(point(px(0.0), px(0.0)));
        for h in &self.row_handles {
            h.set_offset(point(px(0.0), px(0.0)));
        }
        self.router.reset();
        cx.notify();
    }

    fn render_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let lock_text = match self.router.locked() {
            Some(axis) => format!("LOCK: {}", axis.label()),
            None => "LOCK: FREE".to_string(),
        };
        let lock_color = match self.router.locked() {
            Some(ScrollAxis::Vertical) => p.accent,
            Some(ScrollAxis::Horizontal) => p.amber,
            None => p.text_tertiary,
        };
        let phase_text = format!("OS: {}", phase_label(self.router.last_phase));
        let delta_text = format!(
            "raw ({:+.1}, {:+.1})  →  applied ({:+.1}, {:+.1})",
            self.router.last_raw.0,
            self.router.last_raw.1,
            self.router.last_applied.0,
            self.router.last_applied.1
        );
        let switch_text = format!("mid-gesture switches: {}", self.router.axis_switches);

        GlassSurface::panel(&p, &self.glass)
            .pad(SpacePx::S12)
            .gap(SpacePx::S8)
            .build()
            .w_full()
            .flex_shrink_0()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(SpacePx::S12.px())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(SpacePx::S8.px())
                            .children(ScrollMode::ALL.into_iter().map(|m| {
                                let selected = m == self.mode;
                                div()
                                    .id(SharedString::from(format!("mode-{:?}", m)))
                                    .cursor_pointer()
                                    .px(SpacePx::S12.px())
                                    .py(SpacePx::S4.px())
                                    .rounded(p.radius_f32(&self.glass, 0.7))
                                    .bg(if selected {
                                        with_alpha(p.accent, 0.24)
                                    } else {
                                        with_alpha(p.text_primary, 0.05)
                                    })
                                    .border_1()
                                    .border_color(if selected {
                                        p.accent
                                    } else {
                                        with_alpha(p.text_primary, 0.10)
                                    })
                                    .text_size(FontPx::F12.px())
                                    .text_color(if selected { p.text_primary } else { p.text_secondary })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.mode = m;
                                        this.router.reset();
                                        cx.notify();
                                    }))
                                    .child(m.short_label())
                            })),
                    )
                    .child(
                        div()
                            .id("reset-scroll-btn")
                            .cursor_pointer()
                            .px(SpacePx::S12.px())
                            .py(SpacePx::S4.px())
                            .rounded(p.radius_f32(&self.glass, 0.7))
                            .bg(with_alpha(p.text_primary, 0.07))
                            .text_size(FontPx::F11.px())
                            .text_color(p.text_secondary)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reset_all_scroll(cx);
                            }))
                            .child("Reset Scroll"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(SpacePx::S12.px())
                    .child(
                        div()
                            .text_size(FontPx::F12.px())
                            .text_color(p.text_secondary)
                            .child(self.mode.summary()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(SpacePx::S12.px())
                            .flex_shrink_0()
                            .child(
                                div()
                                    .px(SpacePx::S8.px())
                                    .py(px(2.0))
                                    .rounded(p.radius(RadiusPx::R4).px())
                                    .bg(with_alpha(lock_color, 0.16))
                                    .text_size(FontPx::F11.px())
                                    .text_color(lock_color)
                                    .child(lock_text),
                            )
                            .child(
                                div()
                                    .text_size(FontPx::F11.px())
                                    .text_color(p.purple)
                                    .child(phase_text),
                            )
                            .child(
                                div()
                                    .text_size(FontPx::F11.px())
                                    .text_color(p.text_tertiary)
                                    .child(delta_text),
                            )
                            .child(
                                div()
                                    .text_size(FontPx::F11.px())
                                    .text_color(if self.router.axis_switches > 0 {
                                        p.amber
                                    } else {
                                        p.green
                                    })
                                    .child(switch_text),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_district(&self, idx: usize, d: &DemoDistrict, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let handle = self.row_handles[idx].clone();
        let x_offset = -f32::from(handle.offset().x);
        let stage_min_h = 160.0_f32;

        GlassSurface::panel(&p, &self.glass)
            .pad(SpacePx::S16)
            .gap(SpacePx::S12)
            .build()
            .w_full()
            .flex_shrink_0()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(SpacePx::S12.px())
                            .child(single_line(
                                RowHeightPx::H24,
                                SpacePx::S0,
                                SpacePx::S4,
                                FontPx::F14,
                                RadiusPx::R4,
                                None,
                                None,
                                p.text_primary,
                                false,
                                p.centerline,
                                [SharedString::from(d.name).into_any_element()],
                            ))
                            .child(
                                div()
                                    .text_size(FontPx::F11.px())
                                    .text_color(p.text_tertiary)
                                    .child(d.spec),
                            ),
                    )
                    .child(
                        div()
                            .text_size(FontPx::F11.px())
                            .text_color(if x_offset > 0.5 { p.amber } else { p.text_tertiary })
                            .child(format!("x = {:.0}px", x_offset)),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from(format!("row-scroll-{idx}")))
                    .flex()
                    .flex_row()
                    .w_full()
                    .min_w(px(0.0))
                    .overflow_x_scroll()
                    .track_scroll(&handle)
                    .child(
                        div()
                            .id(SharedString::from(format!("row-inner-{idx}")))
                            .flex()
                            .flex_row()
                            .flex_shrink_0()
                            .items_start()
                            .gap(SpacePx::S12.px())
                            .on_scroll_wheel(cx.listener(move |this, ev: &ScrollWheelEvent, window, cx| {
                                cx.stop_propagation();
                                this.apply_scroll_event(Some(idx), ev, window, cx);
                            }))
                            .children(d.stages.iter().map(|(label, subs, jobs)| {
                                let is_gate = label.contains("Gate");
                                GlassSurface::card(&p, &self.glass)
                                    .pad(SpacePx::S12)
                                    .gap(SpacePx::S12)
                                    .radius_factor(1.25)
                                    .border(Some(if is_gate { p.amber } else { p.border_rim }))
                                    .bg(Some(if is_gate { p.glass_gate } else { p.stage_base }))
                                    .accent(if is_gate {
                                        EdgeAccent::Gate
                                    } else {
                                        EdgeAccent::Neutral
                                    })
                                    .build()
                                    .w(px(330.0))
                                    .min_w(px(330.0))
                                    .max_w(px(330.0))
                                    .min_h(px(stage_min_h))
                                    .flex_shrink_0()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(SpacePx::S4.px())
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .justify_between()
                                                    .child(
                                                        div()
                                                            .text_size(FontPx::F13.px())
                                                            .text_color(if is_gate {
                                                                p.amber
                                                            } else {
                                                                p.text_primary
                                                            })
                                                            .child(*label),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(FontPx::F11.px())
                                                            .text_color(p.text_tertiary)
                                                            .child(if is_gate { "Human" } else { "AI Agent" }),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .text_size(FontPx::F11.px())
                                                    .text_color(if subs.is_empty() {
                                                        p.text_tertiary
                                                    } else {
                                                        p.purple
                                                    })
                                                    .child(if subs.is_empty() {
                                                        "● Sub-SM: None"
                                                    } else {
                                                        "● Sub-SM: Active"
                                                    }),
                                            ),
                                    )
                                    .when(!subs.is_empty(), |card| {
                                        card.child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .pl(SpacePx::S8.px())
                                                .border_l(px(1.0))
                                                .border_color(with_alpha(p.purple_inactive, 0.55))
                                                .gap(SpacePx::S4.px())
                                                .children(subs.iter().map(|s| {
                                                    div()
                                                        .text_size(FontPx::F11.px())
                                                        .text_color(p.text_secondary)
                                                        .child(format!("● {s}"))
                                                })),
                                        )
                                    })
                                    .children(jobs.iter().map(|j| {
                                        GlassSurface::card(&p, &self.glass)
                                            .pad(SpacePx::S8)
                                            .gap(SpacePx::S4)
                                            .build()
                                            .child(
                                                div()
                                                    .text_size(FontPx::F12.px())
                                                    .text_color(p.text_primary)
                                                    .child(*j),
                                            )
                                    }))
                            })),
                    ),
            )
            .into_any_element()
    }
}

impl Render for ScrollAxisDemoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        window.set_continuous_present(Some(Duration::from_millis(14)), false);
        let root_bg = particle_wave_with(p.window_backdrop, p.wave_a, p.wave_b, self.glass.bg_brightness);

        div()
            .id("scroll-axis-demo-root")
            .size_full()
            .flex()
            .flex_col()
            .bg(root_bg)
            .text_color(p.text_primary)
            .pt(px(38.0))
            .px(SpacePx::S16.px())
            .pb(SpacePx::S16.px())
            .gap(SpacePx::S12.px())
            .child(self.render_header(cx))
            .child(
                div()
                    .id("districts-outer-scroll")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.vert_handle)
                    .child(
                        div()
                            .id("districts-outer-inner")
                            .flex()
                            .flex_col()
                            .flex_shrink_0()
                            .w_full()
                            .gap(SpacePx::S16.px())
                            .on_scroll_wheel(cx.listener(|this, ev: &ScrollWheelEvent, window, cx| {
                                cx.stop_propagation();
                                this.apply_scroll_event(None, ev, window, cx);
                            }))
                            .children(
                                DISTRICTS
                                    .iter()
                                    .enumerate()
                                    .map(|(i, d)| self.render_district(i, d, cx)),
                            ),
                    ),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1320.0), px(860.0)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Scroll Axis Lock Demo".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(14.0), px(14.0))),
                }),
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            },
            |_, cx| cx.new(|_| ScrollAxisDemoApp::new()),
        )
        .unwrap();
        cx.activate(true);
    });
}
