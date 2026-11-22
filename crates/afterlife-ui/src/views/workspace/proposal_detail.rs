use afterlife_core::{PhilosophyProposal, Principle, ProposalId, SpecProposal, StageDef};
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
    app::{motion::MotionSnapshot, state::AppState, window_state::DEFAULT_DETAIL_HEIGHT_PX},
    components::controls::chip_btn,
    views::{
        root::{AfterlifeApp, DetailResizeDrag},
        templates_view::render_template_proposal_body,
        workspace::proposals_list::{
            DiffMark, classify_principle_diff, classify_stage_diff, removed_stage_ids,
        },
    },
};

fn meta_row(st: &AppState, key: &str, val: String) -> AnyElement {
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
            [SharedString::from(key.to_string()).into_any_element()],
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
            [SharedString::from(val).into_any_element()],
        ))
        .into_any_element()
}

fn render_philosophy_body(st: &AppState, prop: &PhilosophyProposal) -> AnyElement {
    let p = st.palette;
    let current = st
        .engine
        .find_district(&prop.district_id)
        .map(|d| d.active_philosophy());
    let current_principles: &[Principle] = current.map(|c| c.principles.as_slice()).unwrap_or(&[]);

    div()
        .flex()
        .flex_col()
        .gap(SpacePx::S8.px())
        .child(
            div()
                .flex()
                .flex_col()
                .gap(SpacePx::S4.px())
                .child(
                    div()
                        .text_size(FontPx::F11.px())
                        .text_color(p.text_tertiary)
                        .child("Proposed Summary"),
                )
                .child(
                    div()
                        .text_size(FontPx::F12.px())
                        .text_color(p.text_primary)
                        .child(SharedString::from(prop.proposed.summary.clone())),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(SpacePx::S4.px())
                .child(
                    div()
                        .text_size(FontPx::F11.px())
                        .text_color(p.text_tertiary)
                        .child(SharedString::from(format!(
                            "Principles ({})",
                            prop.proposed.principles.len()
                        ))),
                )
                .children(prop.proposed.principles.iter().map(|pr| {
                    let mark = classify_principle_diff(current_principles, pr);
                    let mark_color = match mark {
                        DiffMark::Added => p.accent,
                        DiffMark::Modified => p.amber,
                        DiffMark::Unchanged => p.text_tertiary,
                    };
                    div()
                        .px(SpacePx::S8.px())
                        .py(SpacePx::S4.px())
                        .rounded(p.radius_f32(&st.glass, 0.55))
                        .bg(with_alpha(p.text_primary, 0.028))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(FontPx::F11.px())
                                .text_color(mark_color)
                                .child(SharedString::from(format!(
                                    "[{}] {} · {}",
                                    mark.prefix(),
                                    pr.id,
                                    pr.title
                                ))),
                        )
                        .child(
                            div()
                                .text_size(FontPx::F12.px())
                                .text_color(p.text_secondary)
                                .child(SharedString::from(pr.statement.clone())),
                        )
                })),
        )
        .when(!prop.proposed.anti_patterns.is_empty(), |d| {
            d.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(FontPx::F11.px())
                            .text_color(p.text_tertiary)
                            .child("Anti-Patterns"),
                    )
                    .children(prop.proposed.anti_patterns.iter().map(|ap| {
                        div()
                            .text_size(FontPx::F11.px())
                            .text_color(p.text_secondary)
                            .child(SharedString::from(format!("• {ap}")))
                    })),
            )
        })
        .when(!prop.proposed.decision_heuristics.is_empty(), |d| {
            d.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(FontPx::F11.px())
                            .text_color(p.text_tertiary)
                            .child("Decision Heuristics"),
                    )
                    .children(prop.proposed.decision_heuristics.iter().map(|h| {
                        div()
                            .text_size(FontPx::F11.px())
                            .text_color(p.text_secondary)
                            .child(SharedString::from(format!("• {h}")))
                    })),
            )
        })
        .into_any_element()
}

fn render_spec_body(st: &AppState, prop: &SpecProposal) -> AnyElement {
    let p = st.palette;
    let current_stages: &[StageDef] = st
        .engine
        .find_district(&prop.district_id)
        .map(|d| d.stages())
        .unwrap_or(&[]);
    let removed = removed_stage_ids(current_stages, &prop.proposed.stages);

    div()
        .flex()
        .flex_col()
        .gap(SpacePx::S8.px())
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(FontPx::F11.px())
                        .text_color(p.text_tertiary)
                        .child("Changelog"),
                )
                .child(
                    div()
                        .text_size(FontPx::F12.px())
                        .text_color(p.text_primary)
                        .child(SharedString::from(prop.proposed.changelog.clone())),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(SpacePx::S4.px())
                .child(
                    div()
                        .text_size(FontPx::F11.px())
                        .text_color(p.text_tertiary)
                        .child(SharedString::from(format!(
                            "Stages ({})",
                            prop.proposed.stages.len()
                        ))),
                )
                .children(prop.proposed.stages.iter().map(|stage| {
                    let mark = classify_stage_diff(current_stages, stage);
                    let mark_color = match mark {
                        DiffMark::Added => p.accent,
                        DiffMark::Modified => p.amber,
                        DiffMark::Unchanged => p.text_tertiary,
                    };
                    let checks_n = stage
                        .gate_rubric
                        .as_ref()
                        .map(|g| g.checks.len())
                        .unwrap_or(0);
                    let sub_info = stage
                        .sub_template_id
                        .as_ref()
                        .map(|t| format!(" · sm:{t}"))
                        .unwrap_or_default();
                    let meta = format!(
                        "{} · {}{} · fields:{} · artifacts:{} · checks:{}",
                        stage.id,
                        stage.runner,
                        sub_info,
                        stage.record_fields.len(),
                        stage.required_artifact_kinds.len(),
                        checks_n
                    );
                    div()
                        .px(SpacePx::S8.px())
                        .py(SpacePx::S4.px())
                        .rounded(p.radius_f32(&st.glass, 0.55))
                        .bg(with_alpha(p.text_primary, 0.028))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(FontPx::F11.px())
                                .text_color(mark_color)
                                .child(SharedString::from(format!(
                                    "[{}] {}",
                                    mark.prefix(),
                                    stage.label
                                ))),
                        )
                        .child(
                            div()
                                .text_size(FontPx::F11.px())
                                .text_color(p.text_secondary)
                                .child(SharedString::from(meta)),
                        )
                }))
                .children(removed.into_iter().map(|rem| {
                    div()
                        .text_size(FontPx::F11.px())
                        .text_color(p.coral)
                        .child(SharedString::from(format!("[-] Removed stage: {rem}")))
                })),
        )
        .into_any_element()
}

pub(super) fn render_proposal_detail_sheet(
    st: &AppState,
    proposal_id: ProposalId,
    motion: &MotionSnapshot,
    detail_target_h: f32,
    is_resizing_detail: bool,
    cx: &mut Context<AfterlifeApp>,
) -> Option<AnyElement> {
    let p = st.palette;
    let detail_t = motion.detail_progress;
    let swap_t = motion.detail_swap_progress;
    let inner_pt = (1.0 - detail_t) * 6.0 + (1.0 - swap_t) * 2.0;

    let phil_prop = st
        .engine
        .philosophy_proposals
        .iter()
        .find(|pr| pr.id == proposal_id);
    let spec_prop = st
        .engine
        .spec_proposals
        .iter()
        .find(|pr| pr.id == proposal_id);
    let tmpl_prop = st
        .engine
        .template_proposals
        .iter()
        .find(|pr| pr.id == proposal_id);

    let (scope_label, scope_val, kind_label, version_str, rationale, fb_ids, rename, body_el) =
        if let Some(pr) = phil_prop {
            (
                "District",
                pr.district_id.to_string(),
                "Philosophy",
                format!("Φ v{} → v{}", pr.base_version.0, pr.proposed.version.0),
                pr.rationale.clone(),
                pr.source_feedback_ids.clone(),
                pr.proposed_district_name.clone(),
                render_philosophy_body(st, pr),
            )
        } else if let Some(pr) = spec_prop {
            (
                "District",
                pr.district_id.to_string(),
                "ProcedureSpec",
                format!("S v{} → v{}", pr.base_version.0, pr.proposed.version.0),
                pr.rationale.clone(),
                pr.source_feedback_ids.clone(),
                pr.proposed_district_name.clone(),
                render_spec_body(st, pr),
            )
        } else if let Some(pr) = tmpl_prop {
            let status_label = if st.engine.templates.contains_key(&pr.template_id) {
                "Update Existing"
            } else {
                "New Template"
            };
            (
                "Template",
                pr.template_id.to_string(),
                "Sub-Machine Template",
                status_label.to_string(),
                pr.rationale.clone(),
                pr.source_feedback_ids.clone(),
                None,
                render_template_proposal_body(st, pr),
            )
        } else {
            return None;
        };

    let current_district_name = st
        .engine
        .find_district(&afterlife_core::DistrictId::from(scope_val.as_str()))
        .map(|d| d.name.clone())
        .unwrap_or_else(|| scope_val.clone());
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
    let pid_approve = proposal_id.clone();

    Some(
        GlassSurface::sheet(&p, &st.glass)
            .border(Some(card_border))
            .build()
            .id("left-proposal-detail-sheet")
            .absolute()
            .left(SpacePx::S4.px())
            .right(SpacePx::S4.px())
            .bottom(SpacePx::S4.px())
            .h(px(detail_target_h * detail_t))
            .overflow_hidden()
            .opacity(detail_t)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("proposal-resize-handle")
                    .group("proposal-resize-grp")
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
                            .group_hover("proposal-resize-grp", move |s| {
                                s.w(px(44.0)).bg(grip_hover)
                            }),
                    ),
            )
            .child(
                div()
                    .id("left-proposal-scroll")
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
                                [SharedString::from("Proposal").into_any_element()],
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
                                        [SharedString::from(proposal_id.to_string())
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
                                        .id("proposal-detail-close")
                                        .cursor_pointer()
                                        .hover(move |s| s.text_color(p.text_primary))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.state.update(cx, |st, _| st.close_detail());
                                            cx.notify();
                                        })),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(SpacePx::S4.px())
                            .child(meta_row(st, scope_label, scope_val))
                            .child(meta_row(st, "Kind", kind_label.to_string()))
                            .child(meta_row(st, "Version", version_str))
                            .when(!fb_ids.is_empty(), |d| {
                                let joined = fb_ids
                                    .iter()
                                    .map(ToString::to_string)
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                d.child(meta_row(st, "Feedback", joined))
                            }),
                    )
                    .when_some(rename, |d, new_name| {
                        d.child(
                            div()
                                .px(SpacePx::S8.px())
                                .py(SpacePx::S4.px())
                                .rounded(p.radius_f32(&st.glass, 0.55))
                                .bg(with_alpha(p.amber, 0.12))
                                .text_size(FontPx::F11.px())
                                .text_color(p.amber)
                                .child(SharedString::from(format!(
                                    "District Rename: {current_district_name} → {new_name}"
                                ))),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(FontPx::F11.px())
                                    .text_color(p.text_tertiary)
                                    .child("Rationale"),
                            )
                            .child(
                                div()
                                    .text_size(FontPx::F12.px())
                                    .text_color(p.text_secondary)
                                    .child(SharedString::from(rationale)),
                            ),
                    )
                    .child(body_el)
                    .when_some(st.proposal_error.clone(), |d, err| {
                        d.child(
                            div()
                                .px(SpacePx::S8.px())
                                .py(SpacePx::S4.px())
                                .rounded(p.radius_f32(&st.glass, 0.55))
                                .bg(with_alpha(p.coral, 0.14))
                                .text_size(FontPx::F11.px())
                                .text_color(p.coral)
                                .child(SharedString::from(err)),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(SpacePx::S8.px())
                            .pt(SpacePx::S4.px())
                            .child(chip_btn(
                                st,
                                "proposal-approve-btn",
                                "Approve",
                                RowHeightPx::H28,
                                SpacePx::S12,
                                FontPx::F12,
                                true,
                                move |st, _, _| {
                                    let _ = st.approve_proposal(pid_approve.clone());
                                },
                                cx,
                            )),
                    ),
            )
            .into_any_element(),
    )
}

