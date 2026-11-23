use afterlife_core::{Artifact, EngineCommand, Job, JobLocation, parse_blob_uri};
use afterlife_design::{FontPx, RadiusPx, RowHeightPx, SpacePx, single_line, with_alpha};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};

use crate::{
    app::{
        motion::MotionSnapshot,
        state::{AppState, now_ms},
    },
    components::inline_text::{LinkRowStyle, clickable_link_row, linkified_text},
    views::{
        root::AfterlifeApp,
        workspace::{
            detail_sheet::{
                DETAIL_TOGGLE_WIDTH_PX, DetailSheetChrome, borderless_action_chip,
                detail_footer_visible_for_job, ghost_action_btn, render_detail_sheet_frame,
            },
            step_history::render_step_history,
        },
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

fn meta_row(st: &AppState, label: &'static str, value: String, value_color: gpui::Rgba) -> AnyElement {
    let p = st.palette;
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
            st.show_centerline,
            p.centerline,
            [SharedString::from(value).into_any_element()],
        ))
        .into_any_element()
}

pub(super) fn render_job_detail_sheet(
    st: &AppState,
    job: Job,
    motion: &MotionSnapshot,
    detail_target_h: f32,
    is_resizing_detail: bool,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    let jid_adv = job.id.clone();
    let jid_rew = job.id.clone();
    let jid_stop = job.id.clone();
    let jid_arch = job.id.clone();
    let in_district = matches!(job.location, JobLocation::InDistrict { .. });
    let is_stopped = job.stopped;
    let at_approval_gate = st.engine.is_job_at_approval_gate(&job);
    let show_footer = detail_footer_visible_for_job(&job);

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
    let stage_color = if is_stopped {
        with_alpha(p.coral, 0.85)
    } else {
        p.text_secondary
    };

    let mut body_nodes: Vec<AnyElement> = vec![
        div()
            .text_size(FontPx::F13.px())
            .text_color(p.text_primary)
            .child(linkified_text(
                SharedString::from(format!("d-title-{}", job.id)),
                &job.title,
                p.accent,
            ))
            .into_any_element(),
        div()
            .flex()
            .flex_col()
            .gap(SpacePx::S4.px())
            .child(meta_row(st, "State", stage_label, stage_color))
            .child(meta_row(
                st,
                "Runner",
                job.claimed_by().unwrap_or("Unclaimed").to_string(),
                p.text_secondary,
            ))
            .into_any_element(),
        div()
            .text_size(FontPx::F12.px())
            .text_color(p.text_secondary)
            .child(linkified_text(
                SharedString::from(format!("d-spec-{}", job.id)),
                &job.spec_summary,
                p.accent,
            ))
            .into_any_element(),
    ];

    if !job.raw_input.trim().is_empty() && job.raw_input.trim() != job.spec_summary.trim() {
        body_nodes.push(
            div()
                .text_size(FontPx::F12.px())
                .text_color(p.text_tertiary)
                .child(linkified_text(
                    SharedString::from(format!("d-raw-{}", job.id)),
                    &job.raw_input,
                    p.accent,
                ))
                .into_any_element(),
        );
    }

    if !job.step_records.is_empty() {
        body_nodes.push(render_step_history(st, &job, cx));
    }

    if !job.artifacts.is_empty() {
        body_nodes.push(
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
                }))
                .into_any_element(),
        );
    }

    let footer_actions = show_footer.then(|| {
        let stop_bg = if is_stopped {
            p.active_bg
        } else {
            with_alpha(p.coral_inactive, 0.30)
        };
        let stop_fg = if is_stopped { p.accent } else { p.coral };

        let left_group = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(SpacePx::S8.px())
            .when(in_district && at_approval_gate, |d| {
                d.child(borderless_action_chip(
                    st,
                    "detail-approve",
                    "Approve",
                    p.active_bg,
                    p.accent,
                    None,
                    move |st| {
                        let _ = st.dispatch(EngineCommand::ApproveJob {
                            job_id: jid_adv.clone(),
                            note: None,
                            expected_revision: None,
                            at: now_ms(),
                        });
                    },
                    cx,
                ))
            })
            .when(in_district, |d| {
                d.child(borderless_action_chip(
                    st,
                    "detail-stop-resume",
                    if is_stopped { "Resume" } else { "Stop" },
                    stop_bg,
                    stop_fg,
                    Some(DETAIL_TOGGLE_WIDTH_PX),
                    move |st| {
                        if is_stopped {
                            let _ = st.resume_job(jid_stop.clone());
                        } else {
                            let _ = st.stop_job(jid_stop.clone());
                        }
                    },
                    cx,
                ))
                .child(borderless_action_chip(
                    st,
                    "detail-rework",
                    "Rework",
                    p.btn_bg,
                    p.text_secondary,
                    None,
                    move |st| {
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
            });

        let right_archive = ghost_action_btn(
            st,
            "detail-archive",
            "Archive",
            move |st| {
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
        );

        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w_full()
            .gap(SpacePx::S8.px())
            .child(left_group)
            .child(right_archive)
            .into_any_element()
    });

    let chrome = DetailSheetChrome {
        sheet_id: "left-detail-sheet",
        resize_handle_id: "detail-resize-handle",
        resize_group_id: "detail-resize-grp",
        scroll_id: "left-detail-scroll",
        close_id: "detail-close",
        title: "Job Detail",
        item_id: job.id.to_string(),
        item_id_color: if is_stopped { p.coral } else { p.accent },
        detail_target_h,
        is_resizing_detail,
    };

    render_detail_sheet_frame(st, motion, chrome, body_nodes, None, footer_actions, cx)
}
