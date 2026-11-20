use afterlife_core::{EngineCommand, JobLocation};
use afterlife_design::{
    EdgeAccent, FontPx, GlassSurface, RadiusPx, RowHeightPx, SpacePx, single_line, with_alpha,
};
use gpui::{
    AnyElement, AppContext, Context, CursorStyle, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, SharedString, StatefulInteractiveElement, Styled, div,
    prelude::FluentBuilder, px,
};

use crate::{
    app::{
        motion::{MotionSnapshot, lerp_glass_rgba, lerp_rgba},
        state::{AppState, now_ms},
        window_state::{DEFAULT_DETAIL_HEIGHT_PX, clamp_detail_height},
    },
    components::{
        controls::{card_hover_sheen, chip_btn, status_dot_label},
        drag::{DragPreview, DraggedJob},
        inline_text::{LinkRowStyle, clickable_link_row, linkified_text},
    },
    views::root::{AfterlifeApp, DetailResizeDrag},
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
    let detail_job = (detail_t > 0.005)
        .then(|| motion.detail_job.clone())
        .flatten();
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
                        .overflow_fade_b(px(40.0))
                        .pt(SpacePx::S12.px())
                        .px(SpacePx::S12.px())
                        .pb(SpacePx::S24.px())
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
                                        let is_sel = st.detail_open && st.selected_job_id == job.id;
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
                                                    st.selected_job_id = jid_sel.clone();
                                                    st.detail_open = true;
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
                                                                EngineCommand::AdvanceJob {
                                                                    job_id: jid_approve.clone(),
                                                                    expected_revision: None,
                                                                    fencing_token: None,
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
                                    let is_sel = st.detail_open && st.selected_job_id == job.id;
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
                                                st.selected_job_id = jid_click.clone();
                                                st.detail_open = true;
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
                        ),
                ),
        )
        .when_some(detail_job, |d, job| {
            let jid_adv = job.id.clone();
            let jid_rew = job.id.clone();
            let jid_arch = job.id.clone();
            let in_district = matches!(job.location, JobLocation::InDistrict { .. });
            let is_archived = matches!(job.location, JobLocation::Archived);
            let stage_label = match &job.location {
                JobLocation::Queue => "Jobs (Queued)".to_string(),
                JobLocation::InDistrict {
                    district_id,
                    stage_id,
                    sub_stage_id,
                } => {
                    let sname = st
                        .engine
                        .find_stage(district_id, stage_id)
                        .map(|s| s.label.clone())
                        .unwrap_or_else(|| stage_id.to_string());
                    match sub_stage_id {
                        Some(sub) => format!("{} ({})", sname, sub),
                        None => sname,
                    }
                }
                JobLocation::Completed {
                    district_id,
                    terminal_stage_id,
                    ..
                } => st
                    .engine
                    .find_stage(district_id, terminal_stage_id)
                    .map(|s| format!("{} (Done)", s.label))
                    .unwrap_or_else(|| "Completed".into()),
                JobLocation::Archived => "Archived".into(),
            };
            let swap_t = motion.detail_swap_progress;
            let detail_mt = 16.0 * detail_t;
            let inner_pt = (1.0 - detail_t) * 6.0 + (1.0 - swap_t) * 2.0;
            let detail_target_h = clamp_detail_height(st.detail_height, pane_height);
            let card_border = if is_resizing_detail {
                with_alpha(p.accent, 0.58)
            } else {
                p.border_rim
            };
            let grip_color = if is_resizing_detail {
                with_alpha(p.accent, 0.85)
            } else {
                with_alpha(p.text_primary, 0.16)
            };
            let grip_hover = with_alpha(p.text_primary, 0.44);

            d.child(
                GlassSurface::card(&p, &st.glass)
                    .pad(SpacePx::S12)
                    .gap(SpacePx::S12)
                    .radius_factor(1.4)
                    .border(Some(card_border))
                    .bg(Some(with_alpha(p.window_backdrop, 0.34)))
                    .accent(EdgeAccent::Neutral)
                    .build()
                    .relative()
                    .mt(px(detail_mt))
                    .h(px(detail_target_h * detail_t))
                    .overflow_hidden()
                    .opacity(detail_t)
                    .flex_shrink_0()
                    .child(
                        div()
                            .id("detail-resize-handle")
                            .group("detail-resize-grp")
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .h(px(12.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor(CursorStyle::ResizeUpDown)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                                    if ev.click_count == 2 {
                                        this.detail_resize = None;
                                        this.state.update(cx, |st, _| {
                                            st.detail_height = DEFAULT_DETAIL_HEIGHT_PX;
                                        });
                                        this.persist_window_state(window, cx);
                                    } else {
                                        this.detail_resize = Some(DetailResizeDrag {
                                            start_mouse_y: f32::from(ev.position.y),
                                            start_height: detail_target_h,
                                        });
                                    }
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            )
                            .child(
                                div()
                                    .w(px(if is_resizing_detail { 48.0 } else { 32.0 }))
                                    .h(px(2.0))
                                    .rounded(px(1.0))
                                    .bg(grip_color)
                                    .group_hover("detail-resize-grp", move |s| {
                                        s.w(px(44.0)).bg(grip_hover)
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .id("left-detail-scroll")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .restrict_scroll_to_axis()
                            .overflow_fade_b(px(24.0))
                            .pt(px(inner_pt))
                            .pb(SpacePx::S12.px())
                            .opacity(0.72 + 0.28 * swap_t)
                            .gap(SpacePx::S12.px())
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
                                        st.show_centerline,
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
                                                st.show_centerline,
                                                p.centerline,
                                                [SharedString::from(job.id.to_string())
                                                    .into_any_element()],
                                            ))
                                            .child(
                                                single_line(
                                                    RowHeightPx::H22,
                                                    SpacePx::S4,
                                                    SpacePx::S0,
                                                    FontPx::F11,
                                                    RadiusPx::R0,
                                                    None,
                                                    None,
                                                    p.text_tertiary,
                                                    st.show_centerline,
                                                    p.centerline,
                                                    [SharedString::from("Close").into_any_element()],
                                                )
                                                .id("detail-close")
                                                .cursor_pointer()
                                                .hover(move |s| s.text_color(p.text_primary))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.state.update(cx, |st, _| {
                                                        st.detail_open = false;
                                                    });
                                                    cx.notify();
                                                })),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(FontPx::F13.px())
                                    .text_color(p.text_primary)
                                    .child(linkified_text(
                                        SharedString::from(format!("d-title-{}", job.id)),
                                        &job.title,
                                        p.accent,
                                    )),
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
                                            .child(single_line(
                                                RowHeightPx::H22,
                                                SpacePx::S0,
                                                SpacePx::S4,
                                                FontPx::F11,
                                                RadiusPx::R0,
                                                None,
                                                None,
                                                p.text_tertiary,
                                                st.show_centerline,
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
                                                p.text_secondary,
                                                st.show_centerline,
                                                p.centerline,
                                                [SharedString::from(stage_label)
                                                    .into_any_element()],
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
                                                st.show_centerline,
                                                p.centerline,
                                                [SharedString::from("Runner").into_any_element()],
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
                                                st.show_centerline,
                                                p.centerline,
                                                [SharedString::from(
                                                    job.claimed_by()
                                                        .unwrap_or("Unclaimed")
                                                        .to_string(),
                                                )
                                                .into_any_element()],
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(FontPx::F12.px())
                                    .text_color(p.text_secondary)
                                    .child(linkified_text(
                                        SharedString::from(format!("d-spec-{}", job.id)),
                                        &job.spec_summary,
                                        p.accent,
                                    )),
                            )
                            .when(
                                !job.raw_input.trim().is_empty()
                                    && job.raw_input.trim() != job.spec_summary.trim(),
                                |d| {
                                    d.child(
                                        div()
                                            .text_size(FontPx::F12.px())
                                            .text_color(p.text_tertiary)
                                            .child(linkified_text(
                                                SharedString::from(format!("d-raw-{}", job.id)),
                                                &job.raw_input,
                                                p.accent,
                                            )),
                                    )
                                },
                            )
                            .when(!job.artifacts.is_empty(), |d| {
                                d.child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(SpacePx::S4.px())
                                        .child(single_line(
                                            RowHeightPx::H22,
                                            SpacePx::S0,
                                            SpacePx::S4,
                                            FontPx::F11,
                                            RadiusPx::R0,
                                            None,
                                            None,
                                            p.text_tertiary,
                                            st.show_centerline,
                                            p.centerline,
                                            [SharedString::from("Artifacts").into_any_element()],
                                        ))
                                        .children(job.artifacts.iter().map(|art| {
                                            clickable_link_row(
                                                SharedString::from(format!("art-{}", art.id)),
                                                art.label.clone(),
                                                art.uri.clone(),
                                                LinkRowStyle {
                                                    dot_color: p.text_tertiary,
                                                    link_color: p.accent,
                                                    plain_color: p.text_secondary,
                                                    show_centerline: st.show_centerline,
                                                    centerline: p.centerline,
                                                },
                                            )
                                        })),
                                )
                            })
                            .when(!is_archived, |d| {
                                d.child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .gap(SpacePx::S8.px())
                                        .pt(SpacePx::S4.px())
                                        .when(in_district, |row| {
                                            row.child(chip_btn(
                                                st,
                                                "detail-advance",
                                                "Advance Step",
                                                RowHeightPx::H28,
                                                SpacePx::S12,
                                                FontPx::F12,
                                                true,
                                                move |st, _, _| {
                                                    let _ = st.dispatch(EngineCommand::AdvanceJob {
                                                        job_id: jid_adv.clone(),
                                                        expected_revision: None,
                                                        fencing_token: None,
                                                        at: now_ms(),
                                                    });
                                                },
                                                cx,
                                            ))
                                            .child(chip_btn(
                                                st,
                                                "detail-rework",
                                                "Rework",
                                                RowHeightPx::H28,
                                                SpacePx::S12,
                                                FontPx::F12,
                                                false,
                                                move |st, _, _| {
                                                    let _ = st.dispatch(EngineCommand::ReworkJob {
                                                        job_id: jid_rew.clone(),
                                                        expected_revision: None,
                                                        reason: Some("Operator triggered rework from job detail".to_string()),
                                                        fencing_token: None,
                                                        at: now_ms(),
                                                    });
                                                },
                                                cx,
                                            ))
                                        })
                                        .child(chip_btn(
                                            st,
                                            "detail-archive",
                                            "Archive",
                                            RowHeightPx::H28,
                                            SpacePx::S12,
                                            FontPx::F12,
                                            false,
                                            move |st, _, _| {
                                                if st
                                                    .dispatch(EngineCommand::ArchiveJob {
                                                        job_id: jid_arch.clone(),
                                                        reason: Some("Archived from desktop workspace".to_string()),
                                                        expected_revision: None,
                                                        fencing_token: None,
                                                        at: now_ms(),
                                                    })
                                                    .is_ok()
                                                {
                                                    st.detail_open = false;
                                                }
                                            },
                                            cx,
                                        )),
                                )
                            }),
                    ),
            )
        })
        .into_any_element()
}
