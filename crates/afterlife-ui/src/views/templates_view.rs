use afterlife_core::{SubStageDef, TemplateProposal};
use afterlife_design::{
    EdgeAccent, FontPx, GlassSurface, RadiusPx, RowHeightPx, SpacePx, single_line, single_line_ex,
    with_alpha,
};
use gpui::{
    AnyElement, Context, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::FluentBuilder, px,
};

use crate::{
    app::state::AppState,
    components::controls::chip_btn,
    views::{
        root::AfterlifeApp,
        workspace::proposals_list::{
            DiffMark, classify_sub_stage_diff, removed_sub_stage_ids,
        },
    },
};

pub fn render_template_proposal_body(st: &AppState, prop: &TemplateProposal) -> AnyElement {
    let p = st.palette;
    let current = st.engine.templates.get(&prop.template_id);
    let current_stages: &[SubStageDef] = current.map(|t| t.stages.as_slice()).unwrap_or(&[]);
    let removed = removed_sub_stage_ids(current_stages, &prop.proposed.stages);

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
                        .child("Template Name & Description"),
                )
                .child(
                    div()
                        .text_size(FontPx::F12.px())
                        .text_color(p.text_primary)
                        .child(SharedString::from(format!(
                            "{} ({})",
                            prop.proposed.name, prop.template_id
                        ))),
                )
                .child(
                    div()
                        .text_size(FontPx::F11.px())
                        .text_color(p.text_secondary)
                        .child(SharedString::from(prop.proposed.description.clone())),
                ),
        )
        .when(!prop.proposed.philosophy_notes.is_empty(), |d| {
            d.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(FontPx::F11.px())
                            .text_color(p.text_tertiary)
                            .child("Philosophy Notes"),
                    )
                    .child(
                        div()
                            .text_size(FontPx::F11.px())
                            .text_color(p.text_secondary)
                            .child(SharedString::from(prop.proposed.philosophy_notes.clone())),
                    ),
            )
        })
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
                            "Sub-Stages ({})",
                            prop.proposed.stages.len()
                        ))),
                )
                .children(prop.proposed.stages.iter().map(|stage| {
                    let mark = classify_sub_stage_diff(current_stages, stage);
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
                                    "[{}] {} ({}) · {}",
                                    mark.prefix(),
                                    stage.label,
                                    stage.id,
                                    stage.runner
                                ))),
                        )
                        .child(
                            div()
                                .text_size(FontPx::F11.px())
                                .text_color(p.text_secondary)
                                .child(SharedString::from(stage.procedure.clone())),
                        )
                }))
                .children(removed.into_iter().map(|rem| {
                    div()
                        .px(SpacePx::S8.px())
                        .py(SpacePx::S4.px())
                        .rounded(p.radius_f32(&st.glass, 0.55))
                        .bg(with_alpha(p.coral, 0.12))
                        .text_size(FontPx::F11.px())
                        .text_color(p.coral)
                        .child(SharedString::from(format!("[-] Removed sub-stage: {rem}")))
                })),
        )
        .into_any_element()
}

pub fn render_templates_tab(st: &AppState, cx: &mut Context<AfterlifeApp>) -> AnyElement {
    let p = st.palette;
    let outdated_count = st.engine.outdated_builtin_template_ids().len();
    GlassSurface::panel(&p, &st.glass)
        .build()
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
                    [SharedString::from("Sub-StateMachine Templates").into_any_element()],
                ))
                .when(outdated_count > 0, |d| {
                    let label = format!("Sync {outdated_count} Built-ins → EN");
                    d.child(chip_btn(
                        st,
                        "sync-builtin-templates-btn",
                        &label,
                        RowHeightPx::H28,
                        SpacePx::S12,
                        FontPx::F12,
                        true,
                        |st, _, _| {
                            let _ = st.sync_builtin_templates(None);
                        },
                        cx,
                    ))
                }),
        )
        .children(st.engine.template_order.iter().filter_map(|id| {
            let tmpl = st.engine.templates.get(id)?;
            Some(
                GlassSurface::card(&p, &st.glass)
                    .pad(SpacePx::S12)
                    .gap(SpacePx::S8)
                    .radius_factor(1.2)
                    .border(Some(p.border_rim))
                    .bg(Some(p.stage_base))
                    .accent(EdgeAccent::Neutral)
                    .build()
                    .child(single_line(
                        RowHeightPx::H24,
                        SpacePx::S0,
                        SpacePx::S8,
                        FontPx::F13,
                        p.radius(RadiusPx::R4),
                        None,
                        None,
                        p.purple,
                        st.show_centerline,
                        p.centerline,
                        [SharedString::from(format!("{} ({})", tmpl.name, tmpl.id))
                            .into_any_element()],
                    ))
                    .child(
                        div()
                            .text_size(FontPx::F12.px())
                            .text_color(p.text_secondary)
                            .child(tmpl.description.clone()),
                    )
                    .child(div().flex().flex_row().gap(SpacePx::S8.px()).children(
                        tmpl.stages.iter().enumerate().map(|(i, stage)| {
                            single_line_ex(
                                RowHeightPx::H28,
                                SpacePx::S8,
                                SpacePx::S4,
                                FontPx::F12,
                                p.radius_f32(&st.glass, 0.8),
                                Some(p.border_rim),
                                Some(p.glass_card),
                                st.glass_opt(),
                                0.0,
                                p.text_primary,
                                st.show_centerline,
                                p.centerline,
                                [SharedString::from(format!(
                                    "{}. {} ({})",
                                    i + 1,
                                    stage.label,
                                    stage.runner
                                ))
                                .into_any_element()],
                            )
                        }),
                    )),
            )
        }))
        .into_any_element()
}
