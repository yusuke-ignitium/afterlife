use afterlife_design::{
    EdgeAccent, FontPx, GlassSurface, RadiusPx, RowHeightPx, SpacePx, single_line, single_line_ex,
};
use gpui::{AnyElement, IntoElement, ParentElement, SharedString, Styled, div};

use crate::app::state::AppState;

pub fn render_templates_tab(st: &AppState) -> AnyElement {
    let p = st.palette;
    GlassSurface::panel(&p, &st.glass)
        .build()
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
