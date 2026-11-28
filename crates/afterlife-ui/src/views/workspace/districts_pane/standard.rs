use std::collections::HashMap;

use afterlife_core::{
    District, Job, JobLocation, StageDef, SubMachineTemplate, SubMachineTemplateId,
};
use afterlife_design::{
    BORDER_PX, EdgeAccent, FontPx, GlassSurface, RadiusPx, RowHeightPx, SpacePx, UiStyle,
    single_line, with_alpha,
};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    Stateful, StatefulInteractiveElement, Styled, Div, div, prelude::FluentBuilder, px,
};

use crate::{
    app::{
        motion::{MotionSnapshot, lerp_glass_rgba, lerp_rgba},
        state::AppState,
    },
    components::{
        controls::{card_hover_sheen, status_dot_label},
        inline_text::linkified_text,
    },
    views::root::AfterlifeApp,
};

const CARD_TOGGLE_WIDTH_PX: f32 = 66.0;

#[derive(Clone, Copy)]
struct StandardStageLayout {
    card_pad: SpacePx,
    section_gap: SpacePx,
    header_gap: SpacePx,
    title_row: RowHeightPx,
    meta_row: RowHeightPx,
    sub_row: RowHeightPx,
    sub_gap: SpacePx,
    wireframe_header_pb: SpacePx,
}

const STAGE_LAYOUT: StandardStageLayout = StandardStageLayout {
    card_pad: SpacePx::S12,
    section_gap: SpacePx::S12,
    header_gap: SpacePx::S4,
    title_row: RowHeightPx::H24,
    meta_row: RowHeightPx::H22,
    sub_row: RowHeightPx::H22,
    sub_gap: SpacePx::S4,
    wireframe_header_pb: SpacePx::S8,
};

impl StandardStageLayout {
    fn empty_stage_height_px(
        self,
        stage: &StageDef,
        sub_stage_count: usize,
        style: UiStyle,
    ) -> f32 {
        let card_border = GlassSurface::border_width_px(style, true) * 2.0;
        let card_pad = self.card_pad.val() * 2.0;
        let title_line = self.title_row.val().max(self.meta_row.val());
        let sub_meta_line = if stage.supports_sub_machine() {
            self.header_gap.val() + self.meta_row.val()
        } else {
            0.0
        };
        let header_divider = if style == UiStyle::Wireframe {
            self.wireframe_header_pb.val() + BORDER_PX
        } else {
            0.0
        };
        let sub_list = if sub_stage_count > 0 {
            self.section_gap.val()
                + self.sub_row.val() * sub_stage_count as f32
                + self.sub_gap.val() * sub_stage_count.saturating_sub(1) as f32
        } else {
            0.0
        };
        card_border + card_pad + title_line + sub_meta_line + header_divider + sub_list
    }

    fn district_base_height_px(
        self,
        district: &District,
        templates: &HashMap<SubMachineTemplateId, SubMachineTemplate>,
        style: UiStyle,
    ) -> f32 {
        district
            .stages()
            .iter()
            .map(|stage| {
                let sub_count = stage
                    .sub_template_id
                    .as_ref()
                    .and_then(|tid| templates.get(tid))
                    .map_or(0, |t| t.stages.len());
                self.empty_stage_height_px(stage, sub_count, style)
            })
            .fold(0.0_f32, f32::max)
    }
}

fn pause_glyph(color: gpui::Rgba) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(2.0))
        .child(div().w(px(2.0)).h(px(7.0)).rounded(px(1.0)).bg(color))
        .child(div().w(px(2.0)).h(px(7.0)).rounded(px(1.0)).bg(color))
        .into_any_element()
}

pub(super) fn render_standard_district(
    panel: Stateful<Div>,
    st: &AppState,
    motion: &MotionSnapshot,
    district: &District,
    district_jobs_count: usize,
    stage_w: f32,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    let card_radius = p.radius_f32(&st.glass, 1.0);
    let stage_min_h = STAGE_LAYOUT.district_base_height_px(district, &st.engine.templates, st.style);
    let version_badge = format!(
        "Philosophy v{} · Spec v{}",
        district.active_philosophy_version, district.active_spec_version
    );

    panel
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(SpacePx::S12.px())
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(SpacePx::S12.px())
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
                            [SharedString::from(district.name.clone()).into_any_element()],
                        ))
                        .child(status_dot_label(
                            p.purple_inactive,
                            version_badge,
                            p.text_tertiary,
                            st.show_centerline,
                            p.centerline,
                        )),
                )
                .child(status_dot_label(
                    p.accent,
                    format!("{} active", district_jobs_count),
                    p.text_secondary,
                    st.show_centerline,
                    p.centerline,
                )),
        )
        .child(
            div()
                .id(SharedString::from(format!("stages-scroll-{}", district.id)))
                .flex()
                .flex_row()
                .items_start()
                .gap(SpacePx::S12.px())
                .w_full()
                .min_w(px(0.0))
                .overflow_x_scroll()
                .restrict_scroll_to_axis()
                .children(district.stages().iter().enumerate().map(|(idx, stage)| {
                    let stage_jobs: Vec<&Job> = st
                        .engine
                        .jobs
                        .iter()
                        .filter(|j| j.is_in_stage(&district.id, &stage.id))
                        .collect();

                    let sub_tmpl = stage
                        .sub_template_id
                        .as_ref()
                        .and_then(|tid| st.engine.templates.get(tid));
                    let visible_sub_stages = sub_tmpl
                        .map(|t| t.stages.as_slice())
                        .filter(|stages| !stages.is_empty());

                    let is_gate = stage.is_gate();
                    let is_terminal = stage.is_terminal();

                    GlassSurface::card(&p, &st.glass)
                        .pad(STAGE_LAYOUT.card_pad)
                        .gap(STAGE_LAYOUT.section_gap)
                        .radius_factor(1.25)
                        .border(Some(if is_gate {
                            p.amber
                        } else {
                            p.border_rim
                        }))
                        .bg(Some(if is_gate {
                            p.glass_gate
                        } else {
                            p.stage_base
                        }))
                        .accent(if is_gate {
                            EdgeAccent::Gate
                        } else {
                            EdgeAccent::Neutral
                        })
                        .build()
                        .w(px(stage_w))
                        .min_w(px(stage_w))
                        .max_w(px(stage_w))
                        .min_h(px(stage_min_h))
                        .flex_shrink_0()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(STAGE_LAYOUT.header_gap.px())
                                .when(st.style == UiStyle::Wireframe, |d| {
                                    d.pb(STAGE_LAYOUT.wireframe_header_pb.px())
                                        .border_b(px(BORDER_PX))
                                        .border_color(p.border_subtle)
                                })
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .gap(SpacePx::S8.px())
                                        .child(single_line(
                                            STAGE_LAYOUT.title_row,
                                            SpacePx::S0,
                                            SpacePx::S4,
                                            FontPx::F13,
                                            p.radius(RadiusPx::R2),
                                            None,
                                            None,
                                            if is_gate {
                                                p.amber
                                            } else if is_terminal {
                                                p.green
                                            } else {
                                                p.text_primary
                                            },
                                            st.show_centerline,
                                            p.centerline,
                                            [SharedString::from(format!(
                                                "{}. {}",
                                                idx + 1,
                                                stage.label
                                            ))
                                            .into_any_element()],
                                        ))
                                        .child(single_line(
                                            STAGE_LAYOUT.meta_row,
                                            SpacePx::S0,
                                            SpacePx::S4,
                                            FontPx::F11,
                                            p.radius(RadiusPx::R2),
                                            None,
                                            None,
                                            p.text_tertiary,
                                            st.show_centerline,
                                            p.centerline,
                                            [SharedString::from(stage.runner.clone())
                                                .into_any_element()],
                                        )),
                                )
                                .when(stage.supports_sub_machine(), |d| {
                                    let tmpl_label = match sub_tmpl {
                                        Some(t) => format!("Sub-SM: {}", t.name),
                                        None => "Sub-SM: None".into(),
                                    };
                                    let sub_fg = if sub_tmpl.is_some() {
                                        p.purple
                                    } else {
                                        p.text_tertiary
                                    };
                                    d.child(single_line(
                                        STAGE_LAYOUT.meta_row,
                                        SpacePx::S0,
                                        SpacePx::S4,
                                        FontPx::F11,
                                        RadiusPx::R0,
                                        None,
                                        None,
                                        sub_fg,
                                        st.show_centerline,
                                        p.centerline,
                                        [
                                            div()
                                                .w(px(6.0))
                                                .h(px(6.0))
                                                .rounded(px(3.0))
                                                .bg(sub_fg)
                                                .into_any_element(),
                                            SharedString::from(tmpl_label).into_any_element(),
                                        ],
                                    ))
                                }),
                        )
                        .when_some(visible_sub_stages, |d, sub_stages| {
                            d.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .pl(SpacePx::S8.px())
                                    .border_l(px(1.0))
                                    .border_color(with_alpha(p.purple_inactive, 0.55))
                                    .gap(STAGE_LAYOUT.sub_gap.px())
                                    .children(sub_stages.iter().map(|sub| {
                                        let active_here = stage_jobs.iter().any(|j| {
                                            matches!(
                                                &j.location,
                                                JobLocation::InDistrict { sub_stage_id: Some(sid), .. }
                                                if sid == &sub.id
                                            )
                                        });
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .justify_between()
                                            .gap(SpacePx::S8.px())
                                            .child(single_line(
                                                STAGE_LAYOUT.sub_row,
                                                SpacePx::S0,
                                                SpacePx::S4,
                                                FontPx::F11,
                                                RadiusPx::R0,
                                                None,
                                                None,
                                                if active_here {
                                                    p.purple
                                                } else {
                                                    p.text_secondary
                                                },
                                                st.show_centerline,
                                                p.centerline,
                                                [
                                                    div()
                                                        .w(px(6.0))
                                                        .h(px(6.0))
                                                        .rounded(px(3.0))
                                                        .bg(if active_here {
                                                            p.purple
                                                        } else {
                                                            p.text_tertiary
                                                        })
                                                        .into_any_element(),
                                                    SharedString::from(sub.label.clone())
                                                        .into_any_element(),
                                                ],
                                            ))
                                            .child(single_line(
                                                STAGE_LAYOUT.sub_row,
                                                SpacePx::S0,
                                                SpacePx::S4,
                                                FontPx::F11,
                                                RadiusPx::R0,
                                                None,
                                                None,
                                                p.text_tertiary,
                                                st.show_centerline,
                                                p.centerline,
                                                [SharedString::from(sub.runner.clone())
                                                    .into_any_element()],
                                            ))
                                    })),
                            )
                        })
                        .children(stage_jobs.into_iter().map(|job| {
                            render_standard_job_card(
                                st,
                                motion,
                                job,
                                is_gate,
                                is_terminal,
                                card_radius,
                                cx,
                            )
                        }))
                })),
        )
        .into_any_element()
}

fn render_standard_job_card(
    st: &AppState,
    motion: &MotionSnapshot,
    job: &Job,
    is_gate: bool,
    is_terminal: bool,
    card_radius: gpui::Pixels,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    let is_sel =
        st.detail_open && st.selected_proposal_id.is_none() && st.selected_job_id == job.id;
    let sel_t = motion.selection_weight(&job.id, is_sel);
    let is_stopped = job.stopped;
    let can_toggle = matches!(job.location, JobLocation::InDistrict { .. }) && !is_terminal;
    let target_bg = if is_gate {
        p.glass_gate
    } else {
        p.glass_selected
    };
    let base_border = if is_gate { p.amber } else { p.border_rim };
    let card_bg = if is_stopped {
        with_alpha(p.coral_inactive, if is_sel { 0.20 } else { 0.12 })
    } else {
        lerp_glass_rgba(p.glass_card, target_bg, sel_t)
    };
    let card_border = if is_stopped {
        with_alpha(p.coral, if is_sel { 0.72 } else { 0.28 })
    } else {
        lerp_rgba(base_border, p.accent, sel_t)
    };
    let id_color = if is_stopped {
        p.coral
    } else if is_gate {
        p.amber
    } else {
        lerp_rgba(p.text_secondary, p.accent, sel_t)
    };
    let title_color = if is_stopped {
        p.text_secondary
    } else {
        p.text_primary
    };
    let hover_tint = with_alpha(
        if is_stopped {
            p.coral_inactive
        } else if is_gate {
            p.amber_inactive
        } else {
            p.accent_inactive
        },
        0.14 * (1.0 - sel_t),
    );
    let grp = SharedString::from(format!("stage-job-grp-{}", job.id));
    let jid = job.id.clone();
    let jid_toggle = job.id.clone();
    let sub_tag = match &job.location {
        JobLocation::InDistrict {
            sub_stage_id: Some(sid),
            ..
        } => format!(" ({})", sid),
        _ => String::new(),
    };
    let toggle_btn = can_toggle.then(|| {
        let grp_hover = grp.clone();
        div()
            .id(SharedString::from(format!("stage-job-toggle-{}", job.id)))
            .cursor_pointer()
            .w(px(CARD_TOGGLE_WIDTH_PX))
            .h(RowHeightPx::H22.px())
            .rounded(p.radius_f32(&st.glass, 0.5))
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .gap(SpacePx::S4.px())
            .flex_shrink_0()
            .when(!is_stopped, |d| {
                d.opacity(0.0)
                    .group_hover(grp_hover, |s| s.opacity(1.0))
                    .hover(move |s| s.bg(with_alpha(p.coral_inactive, 0.28)))
                    .child(pause_glyph(with_alpha(p.text_secondary, 0.85)))
                    .child(
                        div()
                            .text_size(FontPx::F11.px())
                            .text_color(p.text_secondary)
                            .child("Stop"),
                    )
            })
            .when(is_stopped, |d| {
                d.hover(move |s| s.bg(with_alpha(p.accent_inactive, 0.28)))
                    .child(div().w(px(5.0)).h(px(5.0)).rounded(px(2.5)).bg(p.coral))
                    .child(
                        div()
                            .text_size(FontPx::F11.px())
                            .text_color(p.coral)
                            .child("Resume"),
                    )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.state.update(cx, |st, _| {
                    if is_stopped {
                        let _ = st.resume_job(jid_toggle.clone());
                    } else {
                        let _ = st.stop_job(jid_toggle.clone());
                    }
                });
                cx.notify();
            }))
    });

    GlassSurface::card(&p, &st.glass)
        .pad(SpacePx::S8)
        .gap(SpacePx::S4)
        .border(Some(card_border))
        .bg(Some(card_bg))
        .accent(if is_stopped || is_gate {
            EdgeAccent::Gate
        } else {
            EdgeAccent::Selected
        })
        .build()
        .overflow_hidden()
        .id(SharedString::from(format!("stage-job-{}", job.id)))
        .group(grp.clone())
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.state.update(cx, |st, _| {
                st.open_job_detail(jid.clone());
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
                .gap(SpacePx::S8.px())
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
                    [SharedString::from(format!("{}{}", job.id, sub_tag)).into_any_element()],
                ))
                .when_some(toggle_btn, |d, btn| d.child(btn)),
        )
        .child(
            div()
                .text_size(FontPx::F13.px())
                .text_color(title_color)
                .child(linkified_text(
                    SharedString::from(format!("s-title-{}", job.id)),
                    &job.title,
                    p.accent,
                )),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use afterlife_core::StageKind;

    use super::*;

    #[test]
    fn stage_base_height_derives_from_actual_district_stage_structure() {
        let mut st = AppState::new_ephemeral();
        let mut templates = st.engine.templates.clone();
        let sample_tmpl = templates.values().next().unwrap().clone();
        let empty_id: SubMachineTemplateId = "tmpl-empty".into();
        let mut empty_tmpl = sample_tmpl.clone();
        empty_tmpl.id = empty_id.clone();
        empty_tmpl.stages.clear();
        templates.insert(empty_id.clone(), empty_tmpl);

        let three_id: SubMachineTemplateId = "tmpl-three".into();
        let mut three_tmpl = sample_tmpl;
        three_tmpl.id = three_id.clone();
        while three_tmpl.stages.len() < 3 {
            let extra = three_tmpl.stages[0].clone();
            three_tmpl.stages.push(extra);
        }
        three_tmpl.stages.truncate(3);
        templates.insert(three_id.clone(), three_tmpl);

        let district = &mut st.engine_mut().districts[0];
        let base_stage = district.stages()[0].clone();
        let make_stage = |kind: StageKind, sub_template_id: Option<SubMachineTemplateId>| {
            let mut s = base_stage.clone();
            s.kind = kind;
            s.sub_template_id = sub_template_id;
            s
        };

        district.active_spec_mut().stages = vec![
            make_stage(StageKind::Work, Some(three_id)),
            make_stage(StageKind::Work, None),
            make_stage(StageKind::Work, Some(empty_id.clone())),
            make_stage(StageKind::ApprovalGate, None),
        ];

        let glass_h =
            STAGE_LAYOUT.district_base_height_px(district, &templates, UiStyle::LiquidGlass);
        assert_eq!(glass_h, 160.0);

        let wireframe_h =
            STAGE_LAYOUT.district_base_height_px(district, &templates, UiStyle::Wireframe);
        let wireframe_delta = GlassSurface::border_width_px(UiStyle::Wireframe, true) * 2.0
            + STAGE_LAYOUT.wireframe_header_pb.val()
            + BORDER_PX;
        assert_eq!(wireframe_h, glass_h + wireframe_delta);

        district.active_spec_mut().stages = vec![
            make_stage(StageKind::Work, None),
            make_stage(StageKind::Work, Some(empty_id)),
        ];
        assert_eq!(
            STAGE_LAYOUT.district_base_height_px(district, &templates, UiStyle::LiquidGlass),
            74.0
        );

        district.active_spec_mut().stages = vec![
            make_stage(StageKind::ApprovalGate, None),
            make_stage(StageKind::Terminal, None),
        ];
        assert_eq!(
            STAGE_LAYOUT.district_base_height_px(district, &templates, UiStyle::LiquidGlass),
            48.0
        );
    }
}
