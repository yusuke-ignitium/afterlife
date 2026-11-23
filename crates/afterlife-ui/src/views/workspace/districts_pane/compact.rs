use afterlife_core::{District, Job, JobLocation, StageDef};
use afterlife_design::{FontPx, RowHeightPx, SpacePx, with_alpha};
use gpui::{
    AnyElement, Context, Div, InteractiveElement, IntoElement, ParentElement, SharedString,
    Stateful, StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};

use crate::{
    app::{
        motion::{MotionSnapshot, lerp_rgba},
        state::AppState,
    },
    views::root::AfterlifeApp,
};

use super::{COMPACT_STAGE_HEIGHT_PX, MAX_COMPACT_JOBS, compact_stage_overflow};

pub(super) fn render_compact_district(
    panel: Stateful<Div>,
    st: &AppState,
    motion: &MotionSnapshot,
    district: &District,
    district_jobs_count: usize,
    stage_w: f32,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;

    panel
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .px(SpacePx::S4.px())
                .child(
                    div()
                        .text_size(FontPx::F12.px())
                        .text_color(p.text_primary)
                        .child(district.name.clone()),
                )
                .child(
                    div()
                        .text_size(FontPx::F11.px())
                        .text_color(if district_jobs_count > 0 {
                            p.accent
                        } else {
                            p.text_tertiary
                        })
                        .child(format!("{} active", district_jobs_count)),
                ),
        )
        .child(
            div()
                .id(SharedString::from(format!("stages-scroll-{}", district.id)))
                .flex()
                .flex_row()
                .items_start()
                .gap(SpacePx::S8.px())
                .w_full()
                .min_w(px(0.0))
                .overflow_x_scroll()
                .restrict_scroll_to_axis()
                .children(district.stages().iter().enumerate().map(|(idx, stage)| {
                    render_compact_stage(st, motion, district, idx, stage, stage_w, cx)
                })),
        )
        .into_any_element()
}

fn render_compact_stage(
    st: &AppState,
    motion: &MotionSnapshot,
    district: &District,
    idx: usize,
    stage: &StageDef,
    stage_w: f32,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    let stage_jobs: Vec<&Job> = st
        .engine
        .jobs
        .iter()
        .filter(|j| j.is_in_stage(&district.id, &stage.id))
        .collect();
    let sub_tmpl = stage
        .sub_template_id
        .as_ref()
        .and_then(|tid| st.engine.templates.get(tid));

    let is_gate = stage.is_gate();
    let is_terminal = stage.is_terminal();
    let overflow = compact_stage_overflow(stage_jobs.len());
    let stage_fg = if is_gate {
        p.amber
    } else if is_terminal {
        p.green
    } else {
        p.text_primary
    };
    let flat_bg = if is_gate {
        with_alpha(
            p.amber_inactive,
            if stage_jobs.is_empty() { 0.06 } else { 0.13 },
        )
    } else {
        with_alpha(
            p.text_primary,
            if stage_jobs.is_empty() { 0.028 } else { 0.052 },
        )
    };

    let header = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(SpacePx::S4.px())
        .h(px(18.0))
        .flex_shrink_0()
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(FontPx::F11.px())
                .text_color(stage_fg)
                .child(format!("{}. {}", idx + 1, stage.label)),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(SpacePx::S4.px())
                .flex_shrink_0()
                .when_some(sub_tmpl.filter(|t| !t.stages.is_empty()), |d, tmpl| {
                    d.child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(3.0))
                            .children(tmpl.stages.iter().map(|sub| {
                                let active_here = stage_jobs.iter().any(|j| {
                                    matches!(
                                        &j.location,
                                        JobLocation::InDistrict { sub_stage_id: Some(sid), .. }
                                        if sid == &sub.id
                                    )
                                });
                                div()
                                    .w(px(5.0))
                                    .h(px(5.0))
                                    .rounded(px(2.5))
                                    .bg(if active_here {
                                        p.purple
                                    } else {
                                        with_alpha(p.text_tertiary, 0.38)
                                    })
                            })),
                    )
                })
                .when(overflow > 0, |d| {
                    let badge_fg = if is_gate { p.amber } else { p.accent };
                    d.child(
                        div()
                            .px(px(5.0))
                            .h(px(16.0))
                            .rounded(px(8.0))
                            .bg(with_alpha(badge_fg, 0.16))
                            .flex()
                            .items_center()
                            .text_size(FontPx::F11.px())
                            .text_color(badge_fg)
                            .child(format!("+{overflow}")),
                    )
                }),
        );

    let jobs_box = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.0))
        .gap(px(2.0))
        .overflow_hidden()
        .children(
            stage_jobs
                .into_iter()
                .take(MAX_COMPACT_JOBS)
                .map(|job| render_compact_job_row(st, motion, job, is_gate, is_terminal, cx)),
        );

    div()
        .flex()
        .flex_col()
        .w(px(stage_w))
        .min_w(px(stage_w))
        .max_w(px(stage_w))
        .flex_shrink_0()
        .h(px(COMPACT_STAGE_HEIGHT_PX))
        .overflow_hidden()
        .p(SpacePx::S8.px())
        .gap(SpacePx::S4.px())
        .rounded(p.radius_f32(&st.glass, 0.85))
        .bg(flat_bg)
        .child(header)
        .child(jobs_box)
        .into_any_element()
}

fn render_compact_job_row(
    st: &AppState,
    motion: &MotionSnapshot,
    job: &Job,
    is_gate: bool,
    is_terminal: bool,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    let is_sel =
        st.detail_open && st.selected_proposal_id.is_none() && st.selected_job_id == job.id;
    let sel_t = motion.selection_weight(&job.id, is_sel);
    let is_stopped = job.stopped;
    let can_toggle = matches!(job.location, JobLocation::InDistrict { .. }) && !is_terminal;
    let id_color = if is_stopped {
        p.coral
    } else if is_gate {
        p.amber
    } else {
        lerp_rgba(p.text_secondary, p.accent, sel_t)
    };
    let bar_base = if is_stopped {
        with_alpha(p.coral, 0.7)
    } else {
        with_alpha(p.text_primary, 0.0)
    };
    let bar_bg = lerp_rgba(bar_base, p.accent, sel_t);
    let title_color = if is_stopped {
        p.text_secondary
    } else {
        lerp_rgba(with_alpha(p.text_primary, 0.86), p.text_primary, sel_t)
    };
    let hover_bg = if is_stopped {
        with_alpha(p.coral_inactive, 0.24)
    } else if is_gate {
        with_alpha(p.amber_inactive, 0.22)
    } else {
        with_alpha(p.text_primary, 0.09)
    };
    let grp = SharedString::from(format!("stage-job-grp-{}", job.id));
    let jid = job.id.clone();
    let jid_toggle = job.id.clone();

    let toggle_btn = can_toggle.then(|| {
        let grp_hover = grp.clone();
        div()
            .id(SharedString::from(format!("stage-job-toggle-{}", job.id)))
            .cursor_pointer()
            .px(px(4.0))
            .rounded(px(3.0))
            .flex_shrink_0()
            .when(!is_stopped, |d| {
                d.opacity(0.0).group_hover(grp_hover, |s| s.opacity(1.0))
            })
            .hover(move |s| {
                s.bg(with_alpha(
                    if is_stopped {
                        p.accent_inactive
                    } else {
                        p.coral_inactive
                    },
                    0.30,
                ))
            })
            .text_size(FontPx::F11.px())
            .text_color(if is_stopped { p.coral } else { p.text_tertiary })
            .child(if is_stopped { "Resume" } else { "Stop" })
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.state.update(cx, |st, _| {
                    if is_stopped {
                        let _ = st.resume_job(jid_toggle.clone());
                    } else {
                        let _ = st.stop_job(jid_toggle.clone());
                    }
                });
                cx.notify();
            }))
    });

    div()
        .id(SharedString::from(format!("stage-job-{}", job.id)))
        .group(grp)
        .cursor_pointer()
        .h(RowHeightPx::H24.px())
        .px(SpacePx::S4.px())
        .rounded(p.radius_f32(&st.glass, 0.55))
        .hover(move |s| s.bg(hover_bg))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(SpacePx::S4.px())
        .on_click(cx.listener(move |this, _, _, cx| {
            this.state.update(cx, |st, _| {
                st.open_job_detail(jid.clone());
            });
            cx.notify();
        }))
        .child(
            div()
                .w(px(2.0))
                .h(px(11.0))
                .rounded(px(1.0))
                .flex_shrink_0()
                .bg(bar_bg),
        )
        .child(
            div()
                .text_size(FontPx::F11.px())
                .text_color(id_color)
                .flex_shrink_0()
                .child(job.id.to_string()),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(FontPx::F11.px())
                .text_color(title_color)
                .child(job.title.clone()),
        )
        .when_some(toggle_btn, |d, btn| d.child(btn))
        .into_any_element()
}
