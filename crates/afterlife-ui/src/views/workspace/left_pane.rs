use afterlife_core::EngineCommand;
use afterlife_design::{
    EdgeAccent, FadePx, FontPx, GlassSurface, RadiusPx, RowHeightPx, ScrollFadeExt, SpacePx,
    single_line, with_alpha,
};
use gpui::{
    AnyElement, AppContext, Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};

use crate::{
    app::{
        motion::{MotionSnapshot, lerp_glass_rgba, lerp_rgba},
        state::{AppState, now_ms},
        window_state::clamp_detail_height,
    },
    components::{
        controls::{card_hover_sheen, chip_btn, status_dot_label},
        drag::{DragPreview, DraggedJob},
        inline_text::linkified_text,
    },
    views::{
        root::AfterlifeApp,
        workspace::{
            job_detail::render_job_detail_sheet,
            proposal_detail::render_proposal_detail_sheet,
            proposals_list::render_proposals_section,
        },
    },
};

pub fn render_left_pane(
    st: &AppState,
    motion: &MotionSnapshot,
    pane_height: f32,
    is_resizing_detail: bool,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    let jobs = st.engine.queued_jobs();
    let approvals = st.engine.approval_jobs();
    let detail_t = motion.detail_progress;
    let detail_proposal = (detail_t > 0.005)
        .then(|| motion.detail_proposal_id.clone())
        .flatten();
    let detail_job = (detail_t > 0.005 && detail_proposal.is_none())
        .then(|| motion.detail_job.clone())
        .flatten();
    let detail_target_h = clamp_detail_height(st.detail_height, pane_height);
    let queue_fade = if detail_proposal.is_some() || detail_job.is_some() {
        FadePx::F0
    } else {
        FadePx::F16
    };
    let queue_pb = SpacePx::S24.val() + detail_target_h * detail_t;
    let card_radius = p.radius_f32(&st.glass, 1.0);

    div()
        .flex()
        .flex_col()
        .w(px(368.0))
        .min_w(px(368.0))
        .h_full()
        .min_h(px(0.0))
        .child(
            div()
                .relative()
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(0.0))
                .rounded(p.radius_f32(&st.glass, 1.4))
                .bg(p.panel_base)
                .overflow_hidden()
                .child(
                    div()
                        .id("left-queue-scroll")
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h(px(0.0))
                        .overflow_y_scroll()
                        .restrict_scroll_to_axis()
                        .overflow_fade_bottom(queue_fade)
                        .pt(SpacePx::S12.px())
                        .px(SpacePx::S12.px())
                        .pb(px(queue_pb))
                        .gap(SpacePx::S16.px())
                        .when(!approvals.is_empty(), |d| {
                            d.child(
                                GlassSurface::panel(&p, &st.glass)
                                    .pad(SpacePx::S0)
                                    .gap(SpacePx::S12)
                                    .border(None)
                                    .bg(None)
                                    .build()
                                    .flex_shrink_0()
                                    .child(single_line(
                                        RowHeightPx::H28,
                                        SpacePx::S0,
                                        SpacePx::S4,
                                        FontPx::F14,
                                        p.radius(RadiusPx::R4),
                                        None,
                                        None,
                                        p.amber,
                                        st.show_centerline,
                                        p.centerline,
                                        [SharedString::from(format!(
                                            "Approvals: {}",
                                            approvals.len()
                                        ))
                                        .into_any_element()],
                                    ))
                                    .children(approvals.into_iter().map(|job| {
                                        let jid_approve = job.id.clone();
                                        let jid_rework = job.id.clone();
                                        let jid_sel = job.id.clone();
                                        let is_sel = st.detail_open
                                            && st.selected_proposal_id.is_none()
                                            && st.selected_job_id == job.id;
                                        let sel_t = motion.selection_weight(&job.id, is_sel);
                                        let card_bg =
                                            lerp_glass_rgba(p.glass_card, p.glass_gate, sel_t);
                                        let card_border = lerp_rgba(p.amber, p.accent, sel_t);
                                        let hover_tint =
                                            with_alpha(p.amber_inactive, 0.14 * (1.0 - sel_t));
                                        let grp =
                                            SharedString::from(format!("approval-grp-{}", job.id));

                                        GlassSurface::card(&p, &st.glass)
                                            .pad(SpacePx::S12)
                                            .gap(SpacePx::S8)
                                            .border(Some(card_border))
                                            .bg(Some(card_bg))
                                            .accent(EdgeAccent::Gate)
                                            .build()
                                            .overflow_hidden()
                                            .id(SharedString::from(format!("approval-{}", job.id)))
                                            .group(grp.clone())
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.state.update(cx, |st, _| {
                                                    st.open_job_detail(jid_sel.clone());
                                                });
                                                cx.notify();
                                            }))
                                            .child(card_hover_sheen(grp, card_radius, hover_tint))
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
                                                        p.radius(RadiusPx::R2),
                                                        None,
                                                        None,
                                                        p.amber,
                                                        st.show_centerline,
                                                        p.centerline,
                                                        [SharedString::from(job.id.to_string())
                                                            .into_any_element()],
                                                    ))
                                                    .child(status_dot_label(
                                                        p.amber,
                                                        "Needs Review",
                                                        p.amber,
                                                        st.show_centerline,
                                                        p.centerline,
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .text_size(FontPx::F13.px())
                                                    .text_color(p.text_primary)
                                                    .child(linkified_text(
                                                        SharedString::from(format!(
                                                            "a-title-{}",
                                                            job.id
                                                        )),
                                                        &job.title,
                                                        p.accent,
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .gap(SpacePx::S8.px())
                                                    .child(chip_btn(
                                                        st,
                                                        SharedString::from(format!(
                                                            "gate-approve-{}",
                                                            job.id
                                                        )),
                                                        "Approve",
                                                        RowHeightPx::H28,
                                                        SpacePx::S12,
                                                        FontPx::F12,
                                                        true,
                                                        move |st, _, _| {
                                                            let _ = st.dispatch(
                                                                EngineCommand::ApproveJob {
                                                                    job_id: jid_approve.clone(),
                                                                    note: None,
                                                                    expected_revision: None,
                                                                    at: now_ms(),
                                                                },
                                                            );
                                                        },
                                                        cx,
                                                    ))
                                                    .child(chip_btn(
                                                        st,
                                                        SharedString::from(format!(
                                                            "gate-rework-{}",
                                                            job.id
                                                        )),
                                                        "Rework",
                                                        RowHeightPx::H28,
                                                        SpacePx::S12,
                                                        FontPx::F12,
                                                        false,
                                                        move |st, _, _| {
                                                            let _ = st.dispatch(
                                                                EngineCommand::ReworkJob {
                                                                    job_id: jid_rework.clone(),
                                                                    expected_revision: None,
                                                                    reason: Some("Human requested rework at approval gate".to_string()),
                                                                    fencing_token: None,
                                                                    at: now_ms(),
                                                                },
                                                            );
                                                        },
                                                        cx,
                                                    )),
                                            )
                                    })),
                            )
                        })
                        .child(
                            GlassSurface::panel(&p, &st.glass)
                                .pad(SpacePx::S0)
                                .gap(SpacePx::S12)
                                .border(None)
                                .bg(None)
                                .build()
                                .flex_shrink_0()
                                .child(single_line(
                                    RowHeightPx::H28,
                                    SpacePx::S0,
                                    SpacePx::S4,
                                    FontPx::F14,
                                    p.radius(RadiusPx::R4),
                                    None,
                                    None,
                                    p.text_primary,
                                    st.show_centerline,
                                    p.centerline,
                                    [SharedString::from(format!("Jobs: {}", jobs.len()))
                                        .into_any_element()],
                                ))
                                .children(jobs.into_iter().map(|job| {
                                    let jid = job.id.clone();
                                    let jid_click = job.id.clone();
                                    let jid_drag = job.id.clone();
                                    let title_drag = job.title.clone();
                                    let is_sel = st.detail_open
                                        && st.selected_proposal_id.is_none()
                                        && st.selected_job_id == job.id;
                                    let sel_t = motion.selection_weight(&job.id, is_sel);
                                    let card_bg =
                                        lerp_glass_rgba(p.glass_card, p.glass_selected, sel_t);
                                    let card_border = lerp_rgba(p.border_rim, p.accent, sel_t);
                                    let id_color = lerp_rgba(p.text_secondary, p.accent, sel_t);
                                    let hover_tint =
                                        with_alpha(p.accent_inactive, 0.14 * (1.0 - sel_t));
                                    let grp = SharedString::from(format!("job-grp-{}", job.id));
                                    let pal = p;
                                    let glass_snap = st.glass;

                                    GlassSurface::card(&p, &st.glass)
                                        .pad(SpacePx::S12)
                                        .gap(SpacePx::S4)
                                        .border(Some(card_border))
                                        .bg(Some(card_bg))
                                        .accent(EdgeAccent::Selected)
                                        .build()
                                        .overflow_hidden()
                                        .id(SharedString::from(format!("job-{}", jid)))
                                        .group(grp.clone())
                                        .cursor_grab()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.state.update(cx, |st, _| {
                                                st.open_job_detail(jid_click.clone());
                                            });
                                            cx.notify();
                                        }))
                                        .on_drag(
                                            DraggedJob {
                                                job_id: jid_drag.clone(),
                                                title: title_drag.clone(),
                                            },
                                            move |drag: &DraggedJob, _, _, cx| {
                                                cx.new(|_| DragPreview {
                                                    job_id: drag.job_id.clone(),
                                                    title: drag.title.clone(),
                                                    palette: pal,
                                                    glass: glass_snap,
                                                })
                                            },
                                        )
                                        .child(card_hover_sheen(grp, card_radius, hover_tint))
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
                                                    p.radius(RadiusPx::R2),
                                                    None,
                                                    None,
                                                    id_color,
                                                    st.show_centerline,
                                                    p.centerline,
                                                    [SharedString::from(job.id.to_string())
                                                        .into_any_element()],
                                                ))
                                                .child(status_dot_label(
                                                    p.text_tertiary,
                                                    "Queued",
                                                    p.text_tertiary,
                                                    st.show_centerline,
                                                    p.centerline,
                                                )),
                                        )
                                        .child(
                                            div()
                                                .text_size(FontPx::F13.px())
                                                .text_color(p.text_primary)
                                            .child(linkified_text(
                                                SharedString::from(format!("q-title-{}", job.id)),
                                                &job.title,
                                                p.accent,
                                            )),
                                    )
                                })),
                        )
                        .when_some(render_proposals_section(st, cx), |d, sec| d.child(sec)),
                )
                .when_some(detail_proposal, |d, pid| {
                    d.when_some(
                        render_proposal_detail_sheet(
                            st,
                            pid,
                            motion,
                            detail_target_h,
                            is_resizing_detail,
                            cx,
                        ),
                        |el, sheet| el.child(sheet),
                    )
                })
                .when_some(detail_job, |d, job| {
                    d.child(render_job_detail_sheet(
                        st,
                        job,
                        motion,
                        detail_target_h,
                        is_resizing_detail,
                        cx,
                    ))
                }),
        )
        .into_any_element()
}
