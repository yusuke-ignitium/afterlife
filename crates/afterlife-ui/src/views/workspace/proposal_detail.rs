use afterlife_core::{
    DistrictProposal, DistrictProposalAction, PhilosophyProposal, Principle, ProposalId,
    SpecProposal, StageDef,
};
use afterlife_design::{FontPx, RadiusPx, RowHeightPx, SpacePx, single_line, with_alpha};
use gpui::{
    AnyElement, Context, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::FluentBuilder, px,
};

use crate::{
    app::{motion::MotionSnapshot, state::AppState},
    views::{
        root::AfterlifeApp,
        templates_view::render_template_proposal_body,
        workspace::{
            detail_sheet::{
                DetailSheetChrome, borderless_action_chip, render_detail_sheet_frame,
            },
            proposals_list::{
                DiffMark, classify_principle_diff, classify_stage_diff, removed_stage_ids,
            },
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

fn district_action_banner(
    st: &AppState,
    district_id: &afterlife_core::DistrictId,
    color: gpui::Rgba,
    message: &'static str,
) -> AnyElement {
    let p = st.palette;
    let name = st
        .engine
        .find_district(district_id)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| district_id.to_string());
    div()
        .flex()
        .flex_col()
        .gap(SpacePx::S8.px())
        .child(meta_row(st, "Target Name", name))
        .child(
            div()
                .px(SpacePx::S8.px())
                .py(SpacePx::S4.px())
                .rounded(p.radius_f32(&st.glass, 0.55))
                .bg(with_alpha(color, 0.12))
                .text_size(FontPx::F11.px())
                .text_color(color)
                .child(message),
        )
        .into_any_element()
}

fn render_district_proposal_body(st: &AppState, prop: &DistrictProposal) -> AnyElement {
    let p = st.palette;
    match &prop.action {
        DistrictProposalAction::Create {
            name,
            setup_by,
            philosophy,
            spec,
            ..
        } => div()
            .flex()
            .flex_col()
            .gap(SpacePx::S8.px())
            .child(meta_row(st, "Name", name.clone()))
            .child(meta_row(st, "Setup By", setup_by.clone()))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child("Initial Philosophy Summary"))
                    .child(div().text_size(FontPx::F12.px()).text_color(p.text_primary).child(SharedString::from(philosophy.summary.clone()))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(SpacePx::S4.px())
                    .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child(SharedString::from(format!("Principles ({})", philosophy.principles.len()))))
                    .children(philosophy.principles.iter().map(|pr| {
                        div()
                            .px(SpacePx::S8.px())
                            .py(SpacePx::S4.px())
                            .rounded(p.radius_f32(&st.glass, 0.55))
                            .bg(with_alpha(p.text_primary, 0.028))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(div().text_size(FontPx::F11.px()).text_color(p.accent).child(SharedString::from(format!("[+] {} · {}", pr.id, pr.title))))
                            .child(div().text_size(FontPx::F12.px()).text_color(p.text_secondary).child(SharedString::from(pr.statement.clone())))
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(SpacePx::S4.px())
                    .child(div().text_size(FontPx::F11.px()).text_color(p.text_tertiary).child(SharedString::from(format!("Initial Stages ({})", spec.stages.len()))))
                    .children(spec.stages.iter().map(|stage| {
                        let checks_n = stage.gate_rubric.as_ref().map(|r| r.checks.len()).unwrap_or(0);
                        div()
                            .px(SpacePx::S8.px())
                            .py(SpacePx::S4.px())
                            .rounded(p.radius_f32(&st.glass, 0.55))
                            .bg(with_alpha(p.text_primary, 0.028))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(div().text_size(FontPx::F11.px()).text_color(p.accent).child(SharedString::from(format!("[+] {}", stage.label))))
                            .child(div().text_size(FontPx::F11.px()).text_color(p.text_secondary).child(SharedString::from(format!("{} · {:?} · {} · {} checks", stage.id, stage.kind, stage.runner, checks_n))))
                    })),
            )
            .into_any_element(),
        DistrictProposalAction::Archive { district_id } => district_action_banner(
            st,
            district_id,
            p.coral,
            "Hides this District from the active board while preserving Philosophy/Spec history and completed job records.",
        ),
        DistrictProposalAction::Restore { district_id } => district_action_banner(
            st,
            district_id,
            p.accent,
            "Restores this archived District back to the active board with its full Philosophy/Spec version history intact.",
        ),
    }
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
    let dist_prop = st
        .engine
        .district_proposals
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
        } else if let Some(pr) = dist_prop {
            let (scope_val, action_label) = match &pr.action {
                DistrictProposalAction::Create {
                    proposed_district_id,
                    ..
                } => (
                    proposed_district_id
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "Auto-assigned on approval".to_string()),
                    "Create District",
                ),
                DistrictProposalAction::Archive { district_id } => {
                    (district_id.to_string(), "Archive District")
                }
                DistrictProposalAction::Restore { district_id } => {
                    (district_id.to_string(), "Restore District")
                }
            };
            (
                "District",
                scope_val,
                "District Lifecycle",
                action_label.to_string(),
                pr.rationale.clone(),
                pr.source_feedback_ids.clone(),
                None,
                render_district_proposal_body(st, pr),
            )
        } else {
            return None;
        };

    let current_district_name = st
        .engine
        .find_district(&afterlife_core::DistrictId::from(scope_val.as_str()))
        .map(|d| d.name.clone())
        .unwrap_or_else(|| scope_val.clone());
    let pid_approve = proposal_id.clone();

    let mut body_nodes: Vec<AnyElement> = vec![
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
            })
            .into_any_element(),
    ];

    if let Some(new_name) = rename {
        body_nodes.push(
            div()
                .px(SpacePx::S8.px())
                .py(SpacePx::S4.px())
                .rounded(p.radius_f32(&st.glass, 0.55))
                .bg(with_alpha(p.amber, 0.12))
                .text_size(FontPx::F11.px())
                .text_color(p.amber)
                .child(SharedString::from(format!(
                    "District Rename: {current_district_name} → {new_name}"
                )))
                .into_any_element(),
        );
    }

    body_nodes.push(
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
            )
            .into_any_element(),
    );
    body_nodes.push(body_el);

    let footer_actions = Some(
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w_full()
            .gap(SpacePx::S8.px())
            .child(borderless_action_chip(
                st,
                "proposal-approve-btn",
                "Approve",
                p.active_bg,
                p.accent,
                None,
                move |st| {
                    let _ = st.approve_proposal(pid_approve.clone());
                },
                cx,
            ))
            .into_any_element(),
    );

    let chrome = DetailSheetChrome {
        sheet_id: "left-proposal-detail-sheet",
        resize_handle_id: "proposal-resize-handle",
        resize_group_id: "proposal-resize-grp",
        scroll_id: "left-proposal-scroll",
        close_id: "proposal-detail-close",
        title: "Proposal Detail",
        item_id: proposal_id.to_string(),
        item_id_color: p.accent,
        detail_target_h,
        is_resizing_detail,
    };

    Some(render_detail_sheet_frame(
        st,
        motion,
        chrome,
        body_nodes,
        st.proposal_error.clone(),
        footer_actions,
        cx,
    ))
}

