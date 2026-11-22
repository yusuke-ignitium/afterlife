use afterlife_core::{Artifact, EngineCommand, Job, JobLocation, parse_blob_uri};
use afterlife_design::{
    FadePx, FontPx, GlassSurface, RadiusPx, RowHeightPx, ScrollFadeExt, SpacePx, single_line,
    with_alpha,
};
use gpui::{
    AnyElement, Context, CursorStyle, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, div, prelude::FluentBuilder,
    px,
};

use crate::{
    app::{
        motion::MotionSnapshot,
        state::{AppState, now_ms},
        window_state::DEFAULT_DETAIL_HEIGHT_PX,
    },
    components::{
        controls::chip_btn,
        inline_text::{LinkRowStyle, clickable_link_row, linkified_text},
    },
    views::{
        root::{AfterlifeApp, DetailResizeDrag},
        workspace::step_history::render_step_history,
    },
};

pub(super) fn render_artifact_row(
    id: SharedString,
    art: &Artifact,
    dot_color: gpui::Rgba,
    st: &AppState,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    if parse_blob_uri(&art.uri).is_some() {
        let art_clone = art.clone();
        return div()
            .id(id)
            .flex()
            .flex_row()
            .items_center()
            .gap(SpacePx::S8.px())
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.state.update(cx, |st, _| {
                    st.open_artifact_preview(&art_clone);
                });
                cx.stop_propagation();
                cx.notify();
            }))
            .child(div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(dot_color))
            .child(
                div()
                    .text_size(FontPx::F11.px())
                    .text_color(p.accent)
                    .hover(|s| s.underline())
                    .child(SharedString::from(art.label.clone())),
            )
            .into_any_element();
    }
    clickable_link_row(
        id,
        art.label.clone(),
        art.uri.clone(),
        LinkRowStyle {
            dot_color,
            link_color: p.accent,
            plain_color: p.text_secondary,
            show_centerline: st.show_centerline,
            centerline: p.centerline,
        },
    )
}

const DETAIL_TOGGLE_WIDTH_PX: f32 = 76.0;

pub(super) fn render_job_detail_sheet(
    st: &AppState,
    job: Job,
    motion: &MotionSnapshot,
    detail_target_h: f32,
    is_resizing_detail: bool,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    let detail_t = motion.detail_progress;
            let jid_adv = job.id.clone();
            let jid_rew = job.id.clone();
            let jid_stop = job.id.clone();
            let jid_arch = job.id.clone();
            let in_district = matches!(job.location, JobLocation::InDistrict { .. });
            let is_stopped = job.stopped;
            let at_approval_gate = st.engine.is_job_at_approval_gate(&job);
            let is_archived = matches!(job.location, JobLocation::Archived);
            let base_stage_label = match &job.location {
                JobLocation::Queue => "Jobs (Queued)".to_string(),
                JobLocation::InDistrict {
                    district_id,
                    stage_id,
                    sub_stage_id,
                } => {
                    let sname = st
                        .engine
                        .find_stage_for_job(&job, district_id, stage_id)
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
                    .find_stage_for_job(&job, district_id, terminal_stage_id)
                    .map(|s| format!("{} (Done)", s.label))
                    .unwrap_or_else(|| "Completed".into()),
                JobLocation::Archived => "Archived".into(),
            };
            let stage_label = if is_stopped {
                format!("{base_stage_label} · Stopped")
            } else {
                base_stage_label
            };
            let swap_t = motion.detail_swap_progress;
            let inner_pt = (1.0 - detail_t) * 6.0 + (1.0 - swap_t) * 2.0;
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

                GlassSurface::sheet(&p, &st.glass)
                    .border(Some(card_border))
                    .build()
                    .id("left-detail-sheet")
                    .absolute()
                    .left(SpacePx::S4.px())
                    .right(SpacePx::S4.px())
                    .bottom(SpacePx::S4.px())
                    .h(px(detail_target_h * detail_t))
                    .overflow_hidden()
                    .opacity(detail_t)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_mouse_down(MouseButton::Right, |_, _, cx| {
                        cx.stop_propagation();
                    })
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
                            .overflow_fade_bottom(FadePx::F16)
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
                                                if is_stopped { p.coral } else { p.accent },
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
                                                        st.close_detail();
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
                                                if is_stopped {
                                                    with_alpha(p.coral, 0.85)
                                                } else {
                                                    p.text_secondary
                                                },
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
                            .when(!job.step_records.is_empty(), |d| {
                                d.child(render_step_history(st, &job, cx))
                            })
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
                                            render_artifact_row(
                                                SharedString::from(format!("art-{}", art.id)),
                                                art,
                                                p.text_tertiary,
                                                st,
                                                cx,
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
                                            let stop_bg = if is_stopped {
                                                p.active_bg
                                            } else {
                                                with_alpha(p.coral_inactive, 0.30)
                                            };
                                            let stop_fg = if is_stopped { p.accent } else { p.coral };
                                            let stop_chip = div()
                                                .id("detail-stop-resume")
                                                .cursor_pointer()
                                                .w(px(DETAIL_TOGGLE_WIDTH_PX))
                                                .h(RowHeightPx::H28.px())
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded(p.radius_f32(&st.glass, 0.75))
                                                .bg(stop_bg)
                                                .hover(|s| s.opacity(0.88))
                                                .text_size(FontPx::F12.px())
                                                .text_color(stop_fg)
                                                .child(if is_stopped { "Resume" } else { "Stop" })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.state.update(cx, |st, _| {
                                                        if is_stopped {
                                                            let _ = st.resume_job(jid_stop.clone());
                                                        } else {
                                                            let _ = st.stop_job(jid_stop.clone());
                                                        }
                                                    });
                                                    cx.notify();
                                                }));
                                            row.when(at_approval_gate, |r| {
                                                r.child(chip_btn(
                                                    st,
                                                    "detail-approve",
                                                    "Approve",
                                                    RowHeightPx::H28,
                                                    SpacePx::S12,
                                                    FontPx::F12,
                                                    true,
                                                    move |st, _, _| {
                                                        let _ = st.dispatch(
                                                            EngineCommand::ApproveJob {
                                                                job_id: jid_adv.clone(),
                                                                note: None,
                                                                expected_revision: None,
                                                                at: now_ms(),
                                                            },
                                                        );
                                                    },
                                                    cx,
                                                ))
                                            })
                                            .child(stop_chip)
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
                    )
        .into_any_element()
}
