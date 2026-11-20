use afterlife_core::JobId;
use afterlife_design::{
    EdgeAccent, FontPx, GlassSettings, GlassSurface, Palette, RadiusPx, RowHeightPx, SpacePx,
    single_line,
};
use gpui::{Context, IntoElement, ParentElement, Render, SharedString, Styled, Window, div, px};

#[derive(Clone)]
pub struct DraggedJob {
    pub job_id: JobId,
    pub title: String,
}

pub struct DragPreview {
    pub job_id: JobId,
    pub title: String,
    pub palette: Palette,
    pub glass: GlassSettings,
}

impl Render for DragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        GlassSurface::card(&p, &self.glass)
            .pad(SpacePx::S8)
            .gap(SpacePx::S4)
            .radius_factor(1.2)
            .border(Some(p.accent))
            .bg(Some(p.glass_selected))
            .accent(EdgeAccent::Selected)
            .build()
            .w(px(260.0))
            .child(single_line(
                RowHeightPx::H22,
                SpacePx::S0,
                SpacePx::S4,
                FontPx::F11,
                p.radius(RadiusPx::R4),
                None,
                None,
                p.accent,
                false,
                p.centerline,
                [SharedString::from(self.job_id.to_string()).into_any_element()],
            ))
            .child(
                div()
                    .text_size(FontPx::F13.px())
                    .text_color(p.text_primary)
                    .child(self.title.clone()),
            )
    }
}
