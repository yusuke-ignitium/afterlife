use afterlife_design::{FontPx, RadiusPx, RowHeightPx, SpacePx, single_line, with_alpha};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};

use crate::{
    StepHistoryDemoApp,
    data::{DEMO_STEPS, DemoStep, STAGE_GROUPS},
};

impl StepHistoryDemoApp {
    pub(crate) fn render_3a_hairline(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let body = div()
            .flex()
            .flex_col()
            .gap(SpacePx::S12.px())
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
                [SharedString::from("Steps").into_any_element()],
            ))
            .children(STAGE_GROUPS.iter().map(|&(stage_id, stage_label)| {
                let steps: Vec<&DemoStep> = DEMO_STEPS
                    .iter()
                    .filter(|s| s.stage_id == stage_id)
                    .collect();
                let collapsed = self.a_collapsed.contains(stage_id);
                let has_art = steps.iter().any(|s| s.artifact.is_some());

                div()
                    .flex()
                    .flex_col()
                    .gap(SpacePx::S4.px())
                    .child(
                        div()
                            .id(SharedString::from(format!("a-stage-{stage_id}")))
                            .cursor_pointer()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.a_collapsed.remove(stage_id) {
                                    this.a_collapsed.insert(stage_id);
                                }
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(SpacePx::S8.px())
                                    .child(
                                        div()
                                            .w(px(5.0))
                                            .h(px(5.0))
                                            .rounded(px(3.0))
                                            .bg(if has_art {
                                                with_alpha(p.accent, 0.8)
                                            } else {
                                                with_alpha(p.text_tertiary, 0.6)
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_size(FontPx::F12.px())
                                            .text_color(p.text_primary)
                                            .child(stage_label),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(FontPx::F11.px())
                                    .text_color(with_alpha(p.text_tertiary, 0.7))
                                    .child(if collapsed { "▸" } else { "▾" }),
                            ),
                    )
                    .when(!collapsed, |d| {
                        d.child(
                            div()
                                .ml(px(2.0))
                                .pl(SpacePx::S12.px())
                                .border_l_1()
                                .border_color(with_alpha(p.text_primary, 0.08))
                                .flex()
                                .flex_col()
                                .gap(SpacePx::S8.px())
                                .children(steps.into_iter().map(|step| {
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
                                                        .text_color(p.text_secondary)
                                                        .child(step.sub_label.unwrap_or(step.runner)),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F11.px())
                                                        .text_color(with_alpha(p.text_tertiary, 0.75))
                                                        .child(if step.sub_label.is_some() {
                                                            format!("{} · {}", step.runner, step.time_label)
                                                        } else {
                                                            step.time_label.to_string()
                                                        }),
                                                ),
                                        )
                                        .children(step.fields.iter().map(|f| {
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(1.0))
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F11.px())
                                                        .text_color(with_alpha(p.text_tertiary, 0.8))
                                                        .child(f.label),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F12.px())
                                                        .text_color(p.text_secondary)
                                                        .child(f.value),
                                                )
                                        }))
                                        .when_some(step.artifact, |col, art| {
                                            col.child(self.artifact_row(art))
                                        })
                                })),
                        )
                    })
            }))
            .into_any_element();

        self.render_pattern_frame(
            "3A. Hairline Whisper",
            "no cards, no pills · 8% hairline rail + native typography",
            "scroll-3a",
            "p3a",
            body,
        )
    }

    pub(crate) fn render_3b_pure_typo(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let body = div()
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
                        [SharedString::from("Steps").into_any_element()],
                    ))
                    .child(
                        div()
                            .id("b-toggle-all")
                            .cursor_pointer()
                            .text_size(FontPx::F11.px())
                            .text_color(p.text_tertiary)
                            .hover(move |s| s.text_color(p.text_primary))
                            .child(if self.b_expanded.len() == STAGE_GROUPS.len() {
                                "Fold"
                            } else {
                                "All"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.b_expanded.len() == STAGE_GROUPS.len() {
                                    this.b_expanded.clear();
                                    this.b_expanded.insert("verify");
                                } else {
                                    for &(sid, _) in STAGE_GROUPS {
                                        this.b_expanded.insert(sid);
                                    }
                                }
                                cx.notify();
                            })),
                    ),
            )
            .children(STAGE_GROUPS.iter().map(|&(stage_id, stage_label)| {
                let steps: Vec<&DemoStep> = DEMO_STEPS
                    .iter()
                    .filter(|s| s.stage_id == stage_id)
                    .collect();
                let open = self.b_expanded.contains(stage_id);
                let has_art = steps.iter().any(|s| s.artifact.is_some());
                let preview = steps
                    .last()
                    .and_then(|s| s.fields.first())
                    .map(|f| f.value)
                    .unwrap_or("");

                div()
                    .flex()
                    .flex_col()
                    .gap(SpacePx::S4.px())
                    .child(
                        div()
                            .id(SharedString::from(format!("b-stage-{stage_id}")))
                            .cursor_pointer()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.b_expanded.remove(stage_id) {
                                    this.b_expanded.insert(stage_id);
                                }
                                cx.notify();
                            }))
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
                                            .gap(SpacePx::S8.px())
                                            .child(
                                                div()
                                                    .w(px(4.0))
                                                    .h(px(4.0))
                                                    .rounded(px(2.0))
                                                    .bg(if open || has_art {
                                                        with_alpha(p.accent, 0.8)
                                                    } else {
                                                        with_alpha(p.text_tertiary, 0.5)
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .text_size(FontPx::F12.px())
                                                    .text_color(if open {
                                                        p.text_primary
                                                    } else {
                                                        p.text_secondary
                                                    })
                                                    .child(stage_label),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(FontPx::F11.px())
                                            .text_color(with_alpha(p.text_tertiary, 0.7))
                                            .child(steps.last().map(|s| s.time_label).unwrap_or("")),
                                    ),
                            )
                            .when(!open, |d| {
                                d.child(
                                    div()
                                        .pl(SpacePx::S12.px())
                                        .text_size(FontPx::F11.px())
                                        .text_color(p.text_tertiary)
                                        .truncate()
                                        .child(preview),
                                )
                            }),
                    )
                    .when(open, |d| {
                        d.child(
                            div()
                                .pl(SpacePx::S12.px())
                                .pb(SpacePx::S4.px())
                                .flex()
                                .flex_col()
                                .gap(SpacePx::S8.px())
                                .children(steps.into_iter().map(|step| {
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(SpacePx::S4.px())
                                        .when_some(step.sub_label, |col, sub| {
                                            col.child(
                                                div()
                                                    .text_size(FontPx::F11.px())
                                                    .text_color(p.text_tertiary)
                                                    .child(format!("{sub} · {}", step.runner)),
                                            )
                                        })
                                        .children(step.fields.iter().map(|f| {
                                            div()
                                                .text_size(FontPx::F12.px())
                                                .text_color(p.text_secondary)
                                                .child(format!("{} — {}", f.label, f.value))
                                        }))
                                        .when_some(step.artifact, |col, art| {
                                            col.child(self.artifact_row(art))
                                        })
                                })),
                        )
                    })
            }))
            .into_any_element();

        self.render_pattern_frame(
            "3B. Pure Typography",
            "zero lines, zero boxes · indent + quiet key—value prose",
            "scroll-3b",
            "p3b",
            body,
        )
    }

    pub(crate) fn render_3c_quiet_wash(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let body = div()
            .flex()
            .flex_col()
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
                [SharedString::from("Steps").into_any_element()],
            ))
            .children(STAGE_GROUPS.iter().map(|&(stage_id, stage_label)| {
                let steps: Vec<&DemoStep> = DEMO_STEPS
                    .iter()
                    .filter(|s| s.stage_id == stage_id)
                    .collect();
                let collapsed = self.c_collapsed.contains(stage_id);
                let has_art = steps.iter().any(|s| s.artifact.is_some());

                div()
                    .flex()
                    .flex_col()
                    .gap(SpacePx::S4.px())
                    .child(
                        div()
                            .id(SharedString::from(format!("c-stage-{stage_id}")))
                            .cursor_pointer()
                            .px(SpacePx::S4.px())
                            .py(px(2.0))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.c_collapsed.remove(stage_id) {
                                    this.c_collapsed.insert(stage_id);
                                }
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(SpacePx::S8.px())
                                    .child(
                                        div()
                                            .text_size(FontPx::F12.px())
                                            .text_color(p.text_primary)
                                            .child(stage_label),
                                    )
                                    .when(has_art, |row| {
                                        row.child(
                                            div()
                                                .w(px(4.0))
                                                .h(px(4.0))
                                                .rounded(px(2.0))
                                                .bg(p.accent),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(FontPx::F11.px())
                                    .text_color(with_alpha(p.text_tertiary, 0.7))
                                    .child(if collapsed {
                                        format!("{}", steps.len())
                                    } else {
                                        steps.last().map(|s| s.time_label).unwrap_or("").to_string()
                                    }),
                            ),
                    )
                    .when(!collapsed, |d| {
                        d.child(
                            div()
                                .px(SpacePx::S8.px())
                                .py(SpacePx::S8.px())
                                .rounded(p.radius_f32(&self.glass, 0.65))
                                .bg(with_alpha(p.text_primary, 0.028))
                                .flex()
                                .flex_col()
                                .gap(SpacePx::S8.px())
                                .children(steps.into_iter().map(|step| {
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(SpacePx::S4.px())
                                        .when_some(step.sub_label, |col, sub| {
                                            col.child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .justify_between()
                                                    .child(
                                                        div()
                                                            .text_size(FontPx::F11.px())
                                                            .text_color(p.text_secondary)
                                                            .child(sub),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(FontPx::F11.px())
                                                            .text_color(with_alpha(p.text_tertiary, 0.7))
                                                            .child(step.runner),
                                                    ),
                                            )
                                        })
                                        .children(step.fields.iter().map(|f| {
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(1.0))
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F11.px())
                                                        .text_color(with_alpha(p.text_tertiary, 0.78))
                                                        .child(f.label),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F12.px())
                                                        .text_color(p.text_secondary)
                                                        .child(f.value),
                                                )
                                        }))
                                        .when_some(step.artifact, |col, art| {
                                            col.child(self.artifact_row(art))
                                        })
                                })),
                        )
                    })
            }))
            .into_any_element();

        self.render_pattern_frame(
            "3C. Quiet Wash",
            "borderless 2.8% glass wash per stage · dot only for evidence",
            "scroll-3c",
            "p3c",
            body,
        )
    }
}
