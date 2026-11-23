use afterlife_core::{
    Artifact, ArtifactKind, EngineCommand, Job, JobId, JobLocation, canonical_pull_request_key,
    is_openable_url, parse_blob_uri, parse_pull_request_ref,
};
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JobPullRequest {
    pub label: String,
    pub uri: String,
    pub openable: bool,
}

pub(crate) fn extract_job_pull_requests(artifacts: &[Artifact]) -> Vec<JobPullRequest> {
    let mut keys: Vec<String> = Vec::new();
    let mut out: Vec<JobPullRequest> = Vec::new();
    for art in artifacts
        .iter()
        .filter(|a| a.kind == ArtifactKind::PullRequest)
    {
        let uri = art.uri.trim().to_string();
        let (label, openable) = match parse_pull_request_ref(&uri) {
            Some(pr) => (pr.display_label(), true),
            None => {
                let trimmed_label = art.label.trim();
                let fallback = if !trimmed_label.is_empty() {
                    trimmed_label.to_string()
                } else {
                    uri.clone()
                };
                (fallback, is_openable_url(&uri))
            }
        };
        if label.is_empty() {
            continue;
        }
        let key = canonical_pull_request_key(&uri);
        let item = JobPullRequest {
            label,
            uri,
            openable,
        };
        if let Some(pos) = keys.iter().position(|k| k == &key) {
            out[pos] = item;
        } else {
            keys.push(key);
            out.push(item);
        }
    }
    out
}

fn render_pr_meta_row(
    st: &AppState,
    job_id: &JobId,
    prs: &[JobPullRequest],
) -> Option<AnyElement> {
    if prs.is_empty() {
        return None;
    }
    let p = st.palette;
    let items = prs.iter().enumerate().map(|(idx, pr)| {
        let id = SharedString::from(format!("d-pr-{}-{idx}", job_id));
        let grp = SharedString::from(format!("d-pr-grp-{}-{idx}", job_id));
        if pr.openable {
            let open_uri = pr.uri.clone();
            div()
                .id(id)
                .group(grp.clone())
                .cursor_pointer()
                .h(RowHeightPx::H22.px())
                .max_w_full()
                .px(SpacePx::S4.px())
                .mr(px(-4.0))
                .rounded(p.radius_f32(&st.glass, 0.55))
                .hover(move |s| s.bg(with_alpha(p.accent_inactive, 0.24)))
                .flex()
                .flex_row()
                .items_center()
                .justify_end()
                .gap(px(4.0))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    cx.open_url(&open_uri);
                })
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(FontPx::F11.px())
                        .text_color(p.accent)
                        .child(SharedString::from(pr.label.clone())),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(FontPx::F11.px())
                        .text_color(with_alpha(p.accent, 0.55))
                        .group_hover(grp, move |s| s.text_color(p.accent))
                        .child("↗"),
                )
                .into_any_element()
        } else {
            div()
                .id(id)
                .h(RowHeightPx::H22.px())
                .max_w_full()
                .flex()
                .flex_row()
                .items_center()
                .justify_end()
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(FontPx::F11.px())
                        .text_color(p.text_secondary)
                        .child(SharedString::from(pr.label.clone())),
                )
                .into_any_element()
        }
    });

    Some(
        div()
            .flex()
            .flex_row()
            .items_start()
            .justify_between()
            .gap(SpacePx::S12.px())
            .child(div().flex_shrink_0().child(single_line(
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
                [SharedString::from("PR").into_any_element()],
            )))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_end()
                    .flex_1()
                    .min_w(px(0.0))
                    .gap(px(2.0))
                    .children(items),
            )
            .into_any_element(),
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

    let pull_requests = extract_job_pull_requests(&job.artifacts);
    let non_pr_artifacts: Vec<&Artifact> = job
        .artifacts
        .iter()
        .filter(|a| a.kind != ArtifactKind::PullRequest)
        .collect();

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
            .when_some(render_pr_meta_row(st, &job.id, &pull_requests), |d, row| {
                d.child(row)
            })
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

    if !non_pr_artifacts.is_empty() {
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
                .children(non_pr_artifacts.into_iter().map(|art| {
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

#[cfg(test)]
mod tests {
    use afterlife_core::ArtifactId;

    use super::*;

    fn mk_artifact(id: &str, kind: ArtifactKind, label: &str, uri: &str) -> Artifact {
        Artifact {
            id: ArtifactId::from(id),
            kind,
            label: label.to_string(),
            uri: uri.to_string(),
            sha256: None,
            produced_at_stage: None,
            produced_at_sub_stage: None,
            revision_at_capture: 1,
        }
    }

    #[test]
    fn extract_job_pull_requests_formats_multi_repo_dedups_and_stays_fail_closed() {
        assert!(extract_job_pull_requests(&[]).is_empty());

        let artifacts = vec![
            mk_artifact(
                "art-1",
                ArtifactKind::VerificationReport,
                "report.md",
                "https://example.com/report",
            ),
            mk_artifact(
                "art-2",
                ArtifactKind::PullRequest,
                "PR #14",
                "https://github.com/org/afterlife/pull/14",
            ),
            mk_artifact(
                "art-3",
                ArtifactKind::PullRequest,
                "PR #14 files",
                "https://GITHUB.COM/org/afterlife/pull/14/files?w=1",
            ),
            mk_artifact(
                "art-4",
                ArtifactKind::PullRequest,
                "MR #42",
                "https://gitlab.com/org/sub/afterlife-cloud/-/merge_requests/42",
            ),
            mk_artifact(
                "art-5",
                ArtifactKind::PullRequest,
                "Custom PR Label",
                "https://review.example.com/changes/99",
            ),
            mk_artifact(
                "art-6",
                ArtifactKind::PullRequest,
                "Unsafe PR",
                "javascript:alert(1)",
            ),
        ];

        let prs = extract_job_pull_requests(&artifacts);
        assert_eq!(prs.len(), 4);
        assert_eq!(prs[0].label, "afterlife #14");
        assert!(prs[0].openable);
        assert_eq!(prs[1].label, "afterlife-cloud #42");
        assert!(prs[1].openable);
        assert_eq!(prs[2].label, "Custom PR Label");
        assert!(prs[2].openable);
        assert_eq!(prs[3].label, "Unsafe PR");
        assert!(!prs[3].openable);
    }
}
