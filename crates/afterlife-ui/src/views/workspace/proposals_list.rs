use afterlife_core::{
    AfterlifeEngine, DistrictProposalAction, Principle, ProposalId, StageDef, SubStageDef,
    TimestampMs,
};
use afterlife_design::{
    EdgeAccent, FontPx, GlassSurface, RadiusPx, RowHeightPx, SpacePx, single_line, with_alpha,
};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder,
};

use crate::{
    app::state::AppState,
    components::controls::{card_hover_sheen, status_dot_label},
    views::root::AfterlifeApp,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProposalKindTag {
    Philosophy,
    Spec,
    Template,
    District,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingProposalCard {
    pub id: ProposalId,
    pub scope_id: String,
    pub kind: ProposalKindTag,
    pub version_label: String,
    pub summary_line: String,
    pub proposed_district_name: Option<String>,
    pub created_at: TimestampMs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffMark {
    Unchanged,
    Added,
    Modified,
}

impl DiffMark {
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Unchanged => "=",
            Self::Added => "+",
            Self::Modified => "~",
        }
    }
}

pub fn collect_pending_proposals(engine: &AfterlifeEngine) -> Vec<PendingProposalCard> {
    let mut items = Vec::new();
    for p in engine.pending_philosophy_proposals() {
        items.push(PendingProposalCard {
            id: p.id.clone(),
            scope_id: p.district_id.to_string(),
            kind: ProposalKindTag::Philosophy,
            version_label: format!("Φ v{} → v{}", p.base_version.0, p.proposed.version.0),
            summary_line: p.rationale.clone(),
            proposed_district_name: p.proposed_district_name.clone(),
            created_at: p.created_at,
        });
    }
    for p in engine.pending_spec_proposals() {
        items.push(PendingProposalCard {
            id: p.id.clone(),
            scope_id: p.district_id.to_string(),
            kind: ProposalKindTag::Spec,
            version_label: format!("S v{} → v{}", p.base_version.0, p.proposed.version.0),
            summary_line: p.proposed.changelog.clone(),
            proposed_district_name: p.proposed_district_name.clone(),
            created_at: p.created_at,
        });
    }
    for p in engine.pending_template_proposals() {
        let action_label = if engine.templates.contains_key(&p.template_id) {
            "SM Update"
        } else {
            "SM New"
        };
        items.push(PendingProposalCard {
            id: p.id.clone(),
            scope_id: p.template_id.to_string(),
            kind: ProposalKindTag::Template,
            version_label: action_label.into(),
            summary_line: p.rationale.clone(),
            proposed_district_name: None,
            created_at: p.created_at,
        });
    }
    for p in engine.pending_district_proposals() {
        let (scope_id, version_label, summary_line, proposed_name) = match &p.action {
            DistrictProposalAction::Create {
                proposed_district_id,
                name,
                ..
            } => (
                proposed_district_id
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "NEW".to_string()),
                "Create".to_string(),
                p.rationale.clone(),
                Some(name.clone()),
            ),
            DistrictProposalAction::Archive { district_id } => (
                district_id.to_string(),
                "Archive".to_string(),
                p.rationale.clone(),
                None,
            ),
            DistrictProposalAction::Restore { district_id } => (
                district_id.to_string(),
                "Restore".to_string(),
                p.rationale.clone(),
                None,
            ),
        };
        items.push(PendingProposalCard {
            id: p.id.clone(),
            scope_id,
            kind: ProposalKindTag::District,
            version_label,
            summary_line,
            proposed_district_name: proposed_name,
            created_at: p.created_at,
        });
    }
    items.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then(a.id.as_str().cmp(b.id.as_str()))
    });
    items
}

pub fn classify_principle_diff(current: &[Principle], proposed: &Principle) -> DiffMark {
    match current.iter().find(|p| p.id == proposed.id) {
        None => DiffMark::Added,
        Some(prev) if prev == proposed => DiffMark::Unchanged,
        Some(_) => DiffMark::Modified,
    }
}

pub fn classify_stage_diff(current: &[StageDef], proposed: &StageDef) -> DiffMark {
    match current.iter().find(|s| s.id == proposed.id) {
        None => DiffMark::Added,
        Some(prev) if prev == proposed => DiffMark::Unchanged,
        Some(_) => DiffMark::Modified,
    }
}

pub fn removed_stage_ids(current: &[StageDef], proposed: &[StageDef]) -> Vec<String> {
    current
        .iter()
        .filter(|s| !proposed.iter().any(|p| p.id == s.id))
        .map(|s| format!("{} ({})", s.label, s.id))
        .collect()
}

pub fn classify_sub_stage_diff(current: &[SubStageDef], proposed: &SubStageDef) -> DiffMark {
    match current.iter().find(|s| s.id == proposed.id) {
        None => DiffMark::Added,
        Some(prev) if prev == proposed => DiffMark::Unchanged,
        Some(_) => DiffMark::Modified,
    }
}

pub fn removed_sub_stage_ids(current: &[SubStageDef], proposed: &[SubStageDef]) -> Vec<String> {
    current
        .iter()
        .filter(|s| !proposed.iter().any(|p| p.id == s.id))
        .map(|s| format!("{} ({})", s.label, s.id))
        .collect()
}

pub(super) fn render_proposals_section(
    st: &AppState,
    cx: &mut Context<AfterlifeApp>,
) -> Option<AnyElement> {
    let items = collect_pending_proposals(&st.engine);
    if items.is_empty() {
        return None;
    }
    let p = st.palette;
    let card_radius = p.radius_f32(&st.glass, 1.0);

    Some(
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
                p.accent,
                st.show_centerline,
                p.centerline,
                [SharedString::from(format!("Proposals: {}", items.len())).into_any_element()],
            ))
            .children(items.into_iter().map(|item| {
                let pid_click = item.id.clone();
                let is_sel = st.detail_open && st.selected_proposal_id.as_ref() == Some(&item.id);
                let card_bg = if is_sel { p.glass_selected } else { p.glass_card };
                let card_border = if is_sel { p.accent } else { p.border_rim };
                let hover_tint = with_alpha(p.accent_inactive, if is_sel { 0.0 } else { 0.14 });
                let grp = SharedString::from(format!("prop-grp-{}", item.id));
                let kind_badge = match item.kind {
                    ProposalKindTag::Philosophy => "Philosophy",
                    ProposalKindTag::Spec => "ProcedureSpec",
                    ProposalKindTag::Template => "Template",
                    ProposalKindTag::District => "District",
                };

                GlassSurface::card(&p, &st.glass)
                    .pad(SpacePx::S12)
                    .gap(SpacePx::S4)
                    .border(Some(card_border))
                    .bg(Some(card_bg))
                    .accent(EdgeAccent::Selected)
                    .build()
                    .overflow_hidden()
                    .id(SharedString::from(format!("prop-card-{}", item.id)))
                    .group(grp.clone())
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.update(cx, |st, _| {
                            st.open_proposal_detail(pid_click.clone());
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
                                p.accent,
                                st.show_centerline,
                                p.centerline,
                                [SharedString::from(format!(
                                    "{} · {}",
                                    item.scope_id, item.version_label
                                ))
                                .into_any_element()],
                            ))
                            .child(status_dot_label(
                                p.amber,
                                kind_badge,
                                p.text_tertiary,
                                st.show_centerline,
                                p.centerline,
                            )),
                    )
                    .child(
                        div()
                            .text_size(FontPx::F12.px())
                            .text_color(p.text_primary)
                            .child(SharedString::from(item.summary_line)),
                    )
                    .when_some(item.proposed_district_name, |d, rename| {
                        d.child(
                            div()
                                .text_size(FontPx::F11.px())
                                .text_color(p.amber)
                                .child(SharedString::from(format!("Rename → {rename}"))),
                        )
                    })
            }))
            .into_any_element(),
    )
}


#[cfg(test)]
mod tests {
    use afterlife_core::{
        DistrictId, EngineCommand, JobEvent, PhilosophyVersion, SpecVersion, StageId,
    };

    use super::*;

    #[test]
    fn pending_proposals_collect_diff_approve_and_surface_version_conflicts() {
        let mut st = AppState::new_ephemeral();
        let human = st.human_actor.clone();
        let dist_id = DistrictId::from("DIST-001");
        let mut stages = st.engine.find_district(&dist_id).unwrap().stages().to_vec();
        stages[1].label = "Design Whole V2".into();

        let p1 = st
            .engine_mut()
            .apply(
                &human,
                EngineCommand::ProposeSpecUpdate {
                    district_id: dist_id.clone(),
                    base_version: SpecVersion::new(1),
                    proposed_district_name: Some("DISTRICT 01 - Renamed".into()),
                    stages: stages.clone(),
                    changelog: "Rename design stage".into(),
                    rationale: "Improve clarity".into(),
                    source_feedback_ids: vec![],
                    at: 1_900_000_000_000,
                },
            )
            .unwrap();
        let pid1 = match &p1[0] {
            JobEvent::SpecProposed { proposal_id, .. } => proposal_id.clone(),
            _ => unreachable!(),
        };

        let p2 = st
            .engine_mut()
            .apply(
                &human,
                EngineCommand::ProposeSpecUpdate {
                    district_id: dist_id.clone(),
                    base_version: SpecVersion::new(1),
                    proposed_district_name: None,
                    stages,
                    changelog: "Competing v2".into(),
                    rationale: "Stale after p1 approval".into(),
                    source_feedback_ids: vec![],
                    at: 1_900_000_001_000,
                },
            )
            .unwrap();
        let pid2 = match &p2[0] {
            JobEvent::SpecProposed { proposal_id, .. } => proposal_id.clone(),
            _ => unreachable!(),
        };

        let items = collect_pending_proposals(&st.engine);
        assert_eq!(items.len(), 2);
        let cur_stages = st.engine.find_district(&dist_id).unwrap().stages();
        let prop1 = st
            .engine
            .spec_proposals
            .iter()
            .find(|p| p.id == pid1)
            .unwrap();
        assert_eq!(
            classify_stage_diff(cur_stages, &prop1.proposed.stages[1]),
            DiffMark::Modified
        );
        assert_eq!(
            classify_stage_diff(cur_stages, &prop1.proposed.stages[0]),
            DiffMark::Unchanged
        );
        assert!(removed_stage_ids(cur_stages, &prop1.proposed.stages).is_empty());

        st.open_proposal_detail(pid1.clone());
        assert_eq!(st.selected_proposal_id, Some(pid1.clone()));
        st.approve_proposal(pid1).unwrap();
        assert_eq!(
            st.engine.find_district(&dist_id).unwrap().name,
            "DISTRICT 01 - Renamed"
        );
        assert_eq!(
            st.engine.find_district(&dist_id).unwrap().active_spec_version,
            SpecVersion::new(2)
        );
        assert!(st.proposal_error.is_none());

        st.open_proposal_detail(pid2.clone());
        let err = st.approve_proposal(pid2).unwrap_err();
        assert!(err.contains("conflict") || err.contains("version"));
        assert!(st.proposal_error.is_some());

        let phil_ev = st
            .engine_mut()
            .apply(
                &human,
                EngineCommand::ProposePhilosophyUpdate {
                    district_id: dist_id.clone(),
                    base_version: PhilosophyVersion::new(1),
                    proposed_district_name: None,
                    summary: "Updated philosophy".into(),
                    principles: vec![Principle {
                        id: "P-NEW".into(),
                        title: "New Principle".into(),
                        statement: "Always verify.".into(),
                        rationale: "Safety.".into(),
                    }],
                    anti_patterns: vec![],
                    decision_heuristics: vec![],
                    rationale: "Test philosophy diff".into(),
                    source_feedback_ids: vec![],
                    at: 1_900_000_002_000,
                },
            )
            .unwrap();
        let phil_pid = match &phil_ev[0] {
            JobEvent::PhilosophyProposed { proposal_id, .. } => proposal_id.clone(),
            _ => unreachable!(),
        };
        let cur_pr = &st.engine.find_district(&dist_id).unwrap().active_philosophy().principles;
        let prop_pr = &st
            .engine
            .philosophy_proposals
            .iter()
            .find(|p| p.id == phil_pid)
            .unwrap()
            .proposed
            .principles[0];
        assert_eq!(classify_principle_diff(cur_pr, prop_pr), DiffMark::Added);
        assert_eq!(StageId::from("design").as_str(), "design");
    }

    #[test]
    fn template_proposals_and_builtin_sync_work_through_app_state() {
        let mut st = AppState::new_ephemeral();
        let human = st.human_actor.clone();
        let tid = afterlife_core::SubMachineTemplateId::from("tmpl-evidence");
        let mut stages = st.engine.templates[&tid].stages.clone();
        stages[0].procedure = "Updated trace procedure".into();

        let ev = st
            .engine_mut()
            .apply(
                &human,
                EngineCommand::ProposeTemplateUpdate {
                    template_id: tid.clone(),
                    name: "Evidence & Probe".into(),
                    description: "Updated via proposal".into(),
                    philosophy_notes: "Check sources first.".into(),
                    stages: stages.clone(),
                    rationale: "Improve template clarity".into(),
                    source_feedback_ids: vec![],
                    at: 1_900_000_010_000,
                },
            )
            .unwrap();
        let pid = match &ev[0] {
            JobEvent::TemplateProposed { proposal_id, .. } => proposal_id.clone(),
            _ => unreachable!(),
        };
        let items = collect_pending_proposals(&st.engine);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, ProposalKindTag::Template);
        let cur_stages = &st.engine.templates[&tid].stages;
        assert_eq!(classify_sub_stage_diff(cur_stages, &stages[0]), DiffMark::Modified);
        assert!(removed_sub_stage_ids(cur_stages, &stages).is_empty());

        st.open_proposal_detail(pid.clone());
        st.approve_proposal(pid).unwrap();
        assert_eq!(st.engine.templates[&tid].description, "Updated via proposal");

        let obs_id = afterlife_core::SubMachineTemplateId::from("tmpl-observe");
        st.engine_mut().templates.get_mut(&obs_id).unwrap().description =
            "対象経路の棚卸しから未観測区間の計測計装、再現可能な評価指標仕様の確定まで".into();
        assert_eq!(
            st.engine.outdated_builtin_template_ids(),
            vec![obs_id.clone()]
        );
        st.sync_builtin_templates(None).unwrap();
        assert!(st.engine.outdated_builtin_template_ids().is_empty());
        assert!(st.engine.templates[&obs_id].description.is_ascii());
    }

    #[test]
    fn district_lifecycle_proposals_collect_and_approve_through_app_state() {
        let mut st = AppState::new_ephemeral();
        let human = st.human_actor.clone();
        let dist1_stages = st.engine.districts[0].stages().to_vec();

        let create_ev = st
            .engine_mut()
            .apply(
                &human,
                EngineCommand::ProposeDistrictCreate {
                    district_id: None,
                    name: "DISTRICT 06 - UI Test".into(),
                    setup_by: None,
                    summary: "UI proposal test".into(),
                    principles: vec![Principle {
                        id: "P-UI-1".into(),
                        title: "Cohesive".into(),
                        statement: "Keep UI clean.".into(),
                        rationale: "Clarity.".into(),
                    }],
                    anti_patterns: vec![],
                    decision_heuristics: vec![],
                    stages: dist1_stages,
                    changelog: "v1".into(),
                    rationale: "Create via UI test".into(),
                    source_feedback_ids: vec![],
                    at: 1_900_000_020_000,
                },
            )
            .unwrap();
        let create_pid = match &create_ev[0] {
            JobEvent::DistrictProposalCreated { proposal_id, .. } => proposal_id.clone(),
            _ => unreachable!(),
        };
        let items = collect_pending_proposals(&st.engine);
        assert!(items.iter().any(|i| i.id == create_pid && i.kind == ProposalKindTag::District && i.version_label == "Create"));

        st.open_proposal_detail(create_pid.clone());
        st.approve_proposal(create_pid).unwrap();
        let d6 = afterlife_core::DistrictId::from("DIST-006");
        assert!(st.engine.active_districts().iter().any(|d| d.id == d6));

        let arch_ev = st
            .engine_mut()
            .apply(
                &human,
                EngineCommand::ProposeDistrictArchive {
                    district_id: d6.clone(),
                    rationale: "Archive via UI test".into(),
                    source_feedback_ids: vec![],
                    at: 1_900_000_021_000,
                },
            )
            .unwrap();
        let arch_pid = match &arch_ev[0] {
            JobEvent::DistrictProposalCreated { proposal_id, .. } => proposal_id.clone(),
            _ => unreachable!(),
        };
        let items = collect_pending_proposals(&st.engine);
        assert!(items.iter().any(|i| i.id == arch_pid && i.kind == ProposalKindTag::District && i.version_label == "Archive"));
        st.approve_proposal(arch_pid).unwrap();
        assert!(st.engine.active_districts().iter().all(|d| d.id != d6));

        let res_ev = st
            .engine_mut()
            .apply(
                &human,
                EngineCommand::ProposeDistrictRestore {
                    district_id: d6.clone(),
                    rationale: "Restore via UI test".into(),
                    source_feedback_ids: vec![],
                    at: 1_900_000_022_000,
                },
            )
            .unwrap();
        let res_pid = match &res_ev[0] {
            JobEvent::DistrictProposalCreated { proposal_id, .. } => proposal_id.clone(),
            _ => unreachable!(),
        };
        let items = collect_pending_proposals(&st.engine);
        assert!(items.iter().any(|i| i.id == res_pid && i.kind == ProposalKindTag::District && i.version_label == "Restore"));
        st.approve_proposal(res_pid).unwrap();
        assert!(st.engine.active_districts().iter().any(|d| d.id == d6));
    }
}
