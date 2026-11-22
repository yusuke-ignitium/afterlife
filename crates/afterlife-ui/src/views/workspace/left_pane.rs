use afterlife_core::{
    AfterlifeEngine, Artifact, DistrictId, EngineCommand, Job, JobLocation, StageId, StepFieldDef,
    StepRecord, StepRecordOutcome, TimestampMs, builtin_step_field_label, parse_blob_uri,
};
use afterlife_design::{
    EdgeAccent, FadePx, FontPx, GlassSurface, RadiusPx, RowHeightPx, ScrollFadeExt, SpacePx,
    single_line, with_alpha,
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

struct StepStageRun<'a> {
    district_id: &'a DistrictId,
    stage_id: &'a StageId,
    records: Vec<&'a StepRecord>,
}

fn group_step_records<'a>(records: &'a [StepRecord]) -> Vec<StepStageRun<'a>> {
    let mut runs: Vec<StepStageRun<'a>> = Vec::new();
    for rec in records {
        if let Some(last) = runs.last_mut()
            && last.district_id == &rec.district_id
            && last.stage_id == &rec.stage_id
            && last
                .records
                .last()
                .is_some_and(|prev| prev.outcome != StepRecordOutcome::Reworked)
        {
            last.records.push(rec);
            continue;
        }
        runs.push(StepStageRun {
            district_id: &rec.district_id,
            stage_id: &rec.stage_id,
            records: vec![rec],
        });
    }
    runs
}

fn format_relative_age(now: TimestampMs, recorded_at: TimestampMs) -> String {
    let elapsed_secs = now.saturating_sub(recorded_at) / 1_000;
    if elapsed_secs < 60 {
        "now".into()
    } else if elapsed_secs < 3_600 {
        format!("{}m", elapsed_secs / 60)
    } else if elapsed_secs < 86_400 {
        format!("{}h", elapsed_secs / 3_600)
    } else {
        format!("{}d", elapsed_secs / 86_400)
    }
}

fn resolve_step_field_defs<'a>(
    engine: &'a AfterlifeEngine,
    job: &Job,
    rec: &StepRecord,
) -> Option<&'a [StepFieldDef]> {
    let stage = engine.find_stage_for_job(job, &rec.district_id, &rec.stage_id)?;
    if let (Some(tid), Some(sub_id)) = (&stage.sub_template_id, &rec.sub_stage_id) {
        if let Some(sub) = engine
            .templates
            .get(tid)
            .and_then(|t| t.stages.iter().find(|s| &s.id == sub_id))
        {
            return Some(&sub.record_fields);
        }
    }
    Some(&stage.record_fields)
}

fn ordered_step_fields(
    engine: &AfterlifeEngine,
    job: &Job,
    rec: &StepRecord,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(defs) = resolve_step_field_defs(engine, job, rec) {
        for def in defs {
            if let Some(val) = rec.fields.get(&def.key) {
                seen.insert(def.key.as_str());
                out.push((def.label.clone(), val.clone()));
            }
        }
    }
    for (k, v) in &rec.fields {
        if !seen.contains(k.as_str()) {
            let label = builtin_step_field_label(k.as_str())
                .unwrap_or(k.as_str())
                .to_string();
            out.push((label, v.clone()));
        }
    }
    out
}

fn render_artifact_row(
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

fn render_step_history(
    st: &AppState,
    job: &Job,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    let now = now_ms();
    let runs = group_step_records(&job.step_records);

    div()
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
            st.show_centerline,
            p.centerline,
            [SharedString::from("Steps").into_any_element()],
        ))
        .children(runs.into_iter().enumerate().map(|(group_idx, run)| {
            let stage_def = st
                .engine
                .find_stage_for_job(job, run.district_id, run.stage_id);
            let stage_label = stage_def
                .map(|s| s.label.clone())
                .unwrap_or_else(|| run.stage_id.to_string());
            let collapsed = st
                .collapsed_step_groups
                .contains(&(job.id.clone(), group_idx));
            let has_evidence = run.records.iter().any(|r| !r.artifact_ids.is_empty());
            let is_approved = run
                .records
                .last()
                .is_some_and(|r| r.outcome == StepRecordOutcome::Approved);
            let is_reworked = run
                .records
                .last()
                .is_some_and(|r| r.outcome == StepRecordOutcome::Reworked);
            let right_meta = if collapsed {
                format!("{}", run.records.len())
            } else if is_approved {
                "Approved".to_string()
            } else if is_reworked {
                "Reworked".to_string()
            } else {
                run.records
                    .last()
                    .map(|r| format_relative_age(now, r.recorded_at))
                    .unwrap_or_default()
            };
            let right_meta_color = if is_approved {
                p.amber
            } else if is_reworked {
                p.coral
            } else {
                with_alpha(p.text_tertiary, 0.72)
            };
            let wash_bg = if is_reworked {
                with_alpha(p.coral, 0.045)
            } else {
                with_alpha(p.text_primary, 0.028)
            };
            let jid_toggle = job.id.clone();

            div()
                .flex()
                .flex_col()
                .gap(SpacePx::S4.px())
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "step-grp-{}-{}",
                            job.id, group_idx
                        )))
                        .cursor_pointer()
                        .px(SpacePx::S4.px())
                        .py(px(2.0))
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.state.update(cx, |st, _| {
                                st.toggle_step_group(jid_toggle.clone(), group_idx);
                            });
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
                                .when(has_evidence, |row| {
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
                                .text_color(right_meta_color)
                                .child(right_meta),
                        ),
                )
                .when(!collapsed, |d| {
                    d.child(
                        div()
                            .px(SpacePx::S8.px())
                            .py(SpacePx::S8.px())
                            .rounded(p.radius_f32(&st.glass, 0.65))
                            .bg(wash_bg)
                            .flex()
                            .flex_col()
                            .gap(SpacePx::S8.px())
                            .children(run.records.into_iter().enumerate().map(|(rec_idx, rec)| {
                                let sub_label = rec.sub_stage_id.as_ref().map(|sub_id| {
                                    stage_def
                                        .and_then(|s| s.sub_template_id.as_ref())
                                        .and_then(|tid| st.engine.templates.get(tid))
                                        .and_then(|t| t.stages.iter().find(|sub| &sub.id == sub_id))
                                        .map(|sub| sub.label.clone())
                                        .unwrap_or_else(|| sub_id.to_string())
                                });
                                let sub_label_color = if rec.outcome == StepRecordOutcome::Reworked
                                {
                                    p.coral
                                } else {
                                    p.text_secondary
                                };
                                let actor_label = rec.actor.display_label().to_string();
                                let fields = ordered_step_fields(&st.engine, job, rec);
                                let step_artifacts: Vec<&Artifact> = rec
                                    .artifact_ids
                                    .iter()
                                    .filter_map(|aid| job.artifacts.iter().find(|a| &a.id == aid))
                                    .collect();

                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(SpacePx::S4.px())
                                    .when_some(sub_label, |col, sub| {
                                        col.child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .justify_between()
                                                .gap(SpacePx::S8.px())
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F11.px())
                                                        .text_color(sub_label_color)
                                                        .child(sub),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F11.px())
                                                        .text_color(with_alpha(
                                                            p.text_tertiary,
                                                            0.72,
                                                        ))
                                                        .child(actor_label.clone()),
                                                ),
                                        )
                                    })
                                    .when(rec.sub_stage_id.is_none(), |col| {
                                        col.child(
                                            div()
                                                .text_size(FontPx::F11.px())
                                                .text_color(with_alpha(p.text_tertiary, 0.72))
                                                .child(actor_label.clone()),
                                        )
                                    })
                                    .children(fields.into_iter().enumerate().map(
                                        |(f_idx, (label, val))| {
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(1.0))
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F11.px())
                                                        .text_color(with_alpha(
                                                            p.text_tertiary,
                                                            0.78,
                                                        ))
                                                        .child(label),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F12.px())
                                                        .text_color(p.text_secondary)
                                                        .child(linkified_text(
                                                            SharedString::from(format!(
                                                                "step-f-{}-{}-{}-{}",
                                                                job.id, group_idx, rec_idx, f_idx
                                                            )),
                                                            &val,
                                                            p.accent,
                                                        )),
                                                )
                                        },
                                    ))
                                    .children(step_artifacts.into_iter().map(|art| {
                                        render_artifact_row(
                                            SharedString::from(format!(
                                                "step-art-{}-{}-{}-{}",
                                                job.id, group_idx, rec_idx, art.id
                                            )),
                                            art,
                                            p.accent,
                                            st,
                                            cx,
                                        )
                                    }))
                            })),
                    )
                })
        }))
        .into_any_element()
}

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
    let detail_target_h = clamp_detail_height(st.detail_height, pane_height);
    let queue_fade = if detail_job.is_some() {
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
                )
                .when_some(detail_job, |d, job| {
            let jid_adv = job.id.clone();
            let jid_rew = job.id.clone();
            let jid_arch = job.id.clone();
            let in_district = matches!(job.location, JobLocation::InDistrict { .. });
            let at_approval_gate = st.engine.is_job_at_approval_gate(&job);
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

            d.child(
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
                                                        st.close_artifact_preview();
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
        }),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use afterlife_core::{Actor, JobEvent, OwnerId, SpecVersion, SubStageId};

    use super::*;

    fn mk_record(
        stage: &str,
        sub: Option<&str>,
        outcome: StepRecordOutcome,
        fields: &[(&str, &str)],
        at: u64,
    ) -> StepRecord {
        StepRecord {
            district_id: DistrictId::from("DIST-001"),
            stage_id: StageId::from(stage),
            sub_stage_id: sub.map(SubStageId::from),
            revision: 1,
            actor: Actor::local_human(OwnerId::from("local-owner")),
            outcome,
            fields: fields
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect::<BTreeMap<_, _>>(),
            artifact_ids: Vec::new(),
            recorded_at: at,
        }
    }

    #[test]
    fn group_step_records_preserves_rework_runs_and_field_order() {
        let mut engine = AfterlifeEngine::new();
        let dist_id = DistrictId::from("DIST-001");
        let job_id = engine.queue_preset_job("Step order test", Some(&dist_id));
        engine.assign_job_to_district(&job_id, &dist_id).unwrap();
        let job = engine.find_job(&job_id).unwrap().clone();

        let records = vec![
            mk_record(
                "discover",
                Some("trace_sources"),
                StepRecordOutcome::Advanced,
                &[("findings", "f1"), ("sources_read", "s1")],
                1_000,
            ),
            mk_record(
                "discover",
                Some("isolate_probe"),
                StepRecordOutcome::Advanced,
                &[("probe_method", "p1"), ("observed_behavior", "o1")],
                2_000,
            ),
            mk_record(
                "design",
                None,
                StepRecordOutcome::Advanced,
                &[("decision", "d1")],
                3_000,
            ),
            mk_record(
                "discover",
                Some("trace_sources"),
                StepRecordOutcome::Advanced,
                &[("sources_read", "s2"), ("findings", "f2")],
                4_000,
            ),
        ];

        let runs = group_step_records(&records);
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].stage_id.as_str(), "discover");
        assert_eq!(runs[0].records.len(), 2);
        assert_eq!(runs[1].stage_id.as_str(), "design");
        assert_eq!(runs[2].stage_id.as_str(), "discover");
        assert_eq!(runs[2].records.len(), 1);

        let ordered = ordered_step_fields(&engine, &job, &records[0]);
        assert_eq!(ordered.len(), 2);
        assert_eq!(ordered[0].0, "Primary Sources Reviewed");
        assert_eq!(ordered[0].1, "s1");
        assert_eq!(ordered[1].0, "Confirmed Facts & Constraints");
        assert_eq!(ordered[1].1, "f1");
    }

    #[test]
    fn rework_step_record_splits_same_stage_runs_and_labels_reason() {
        let mut engine = AfterlifeEngine::new();
        let dist_id = DistrictId::from("DIST-001");
        let job_id = engine.queue_preset_job("Rework history test", Some(&dist_id));
        engine.assign_job_to_district(&job_id, &dist_id).unwrap();
        let job = engine.find_job(&job_id).unwrap().clone();

        let records = vec![
            mk_record(
                "build",
                Some("apply_change"),
                StepRecordOutcome::Advanced,
                &[("decision", "d1"), ("execution", "e1")],
                1_000,
            ),
            mk_record(
                "build",
                Some("run_checks"),
                StepRecordOutcome::Reworked,
                &[("reason", "境界値テスト不足")],
                2_000,
            ),
            mk_record(
                "build",
                Some("apply_change"),
                StepRecordOutcome::Advanced,
                &[("decision", "d2"), ("execution", "e2")],
                3_000,
            ),
        ];

        let runs = group_step_records(&records);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].records.len(), 2);
        assert_eq!(runs[0].records[1].outcome, StepRecordOutcome::Reworked);
        assert_eq!(runs[1].records.len(), 1);

        let ordered = ordered_step_fields(&engine, &job, &records[1]);
        assert_eq!(
            ordered,
            vec![("Rework Reason".to_string(), "境界値テスト不足".to_string())]
        );
    }

    #[test]
    fn step_labels_prefer_job_pinned_spec_version_over_active_spec() {
        let mut engine = AfterlifeEngine::new();
        let human = engine.local_actor();
        let dist_id = DistrictId::from("DIST-001");
        let v1_job_id = engine.queue_preset_job("V1 job", Some(&dist_id));
        engine
            .assign_job_to_district(&v1_job_id, &dist_id)
            .unwrap();

        let mut v2_stages = engine
            .find_district(&dist_id)
            .unwrap()
            .active_spec()
            .stages
            .clone();
        let design = v2_stages
            .iter_mut()
            .find(|s| s.id == StageId::from("design"))
            .unwrap();
        design.label = "Design V2".into();
        design.record_fields[0].label = "V2設計判断ラベル".into();

        let prop_ev = engine
            .apply(
                &human,
                EngineCommand::ProposeSpecUpdate {
                    district_id: dist_id.clone(),
                    base_version: SpecVersion::new(1),
                    stages: v2_stages,
                    changelog: "Update design labels in v2".into(),
                    rationale: "Verify pinned spec resolution in UI".into(),
                    source_feedback_ids: Vec::new(),
                    at: 1_900_000_000_000,
                },
            )
            .unwrap();
        let prop_id = match &prop_ev[0] {
            JobEvent::SpecProposed { proposal_id, .. } => proposal_id.clone(),
            _ => unreachable!(),
        };
        engine
            .apply(
                &human,
                EngineCommand::ApproveSpecProposal {
                    proposal_id: prop_id,
                    at: 1_900_000_001_000,
                },
            )
            .unwrap();

        let v2_job_id = engine.queue_preset_job("V2 job", Some(&dist_id));
        engine
            .assign_job_to_district(&v2_job_id, &dist_id)
            .unwrap();

        let v1_job = engine.find_job(&v1_job_id).unwrap().clone();
        let v2_job = engine.find_job(&v2_job_id).unwrap().clone();
        let rec = mk_record(
            "design",
            None,
            StepRecordOutcome::Advanced,
            &[("decision", "design choice")],
            5_000,
        );

        let v1_fields = ordered_step_fields(&engine, &v1_job, &rec);
        assert_eq!(v1_fields[0].0, "Chosen Design & System Fit");
        let v2_fields = ordered_step_fields(&engine, &v2_job, &rec);
        assert_eq!(v2_fields[0].0, "V2設計判断ラベル");
    }

    #[test]
    fn builtin_fallback_step_labels_are_english_while_user_japanese_passes_through() {
        let mut engine = AfterlifeEngine::new();
        let dist_id = DistrictId::from("DIST-001");
        let job_id = engine.queue_preset_job("Fallback label test", Some(&dist_id));
        engine.assign_job_to_district(&job_id, &dist_id).unwrap();
        let job = engine.find_job(&job_id).unwrap().clone();

        let rec = mk_record(
            "approve",
            None,
            StepRecordOutcome::Approved,
            &[
                ("decision", "採用判断"),
                ("execution", "実行結果"),
                ("note", "承認メモ本文"),
                ("reason", "差し戻し理由本文"),
                ("カスタム項目", "日本語の記録内容"),
            ],
            6_000,
        );

        let ordered = ordered_step_fields(&engine, &job, &rec);
        assert_eq!(
            ordered,
            vec![
                ("Decision & Rationale".to_string(), "採用判断".to_string()),
                ("Execution & Outcome".to_string(), "実行結果".to_string()),
                ("Approval Note".to_string(), "承認メモ本文".to_string()),
                ("Rework Reason".to_string(), "差し戻し理由本文".to_string()),
                ("カスタム項目".to_string(), "日本語の記録内容".to_string()),
            ]
        );
        for (label, _) in &ordered[..4] {
            assert!(
                !label.chars().any(|c| matches!(
                    c,
                    '\u{3000}'..='\u{303F}'
                        | '\u{3040}'..='\u{30FF}'
                        | '\u{3400}'..='\u{4DBF}'
                        | '\u{4E00}'..='\u{9FFF}'
                        | '\u{FF00}'..='\u{FFEF}'
                )),
                "built-in fallback label `{label}` must be English-only"
            );
        }
    }

    #[test]
    fn format_relative_age_covers_units() {
        let base = 1_000_000_000u64;
        assert_eq!(format_relative_age(base, base + 5_000), "now");
        assert_eq!(format_relative_age(base, base - 30_000), "now");
        assert_eq!(format_relative_age(base, base - 180_000), "3m");
        assert_eq!(format_relative_age(base, base - 7_200_000), "2h");
        assert_eq!(format_relative_age(base, base - 172_800_000), "2d");
    }
}
