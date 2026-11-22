use afterlife_design::{
    FadePx, FontPx, GlassSurface, RadiusPx, RowHeightPx, ScrollFadeExt, SpacePx, single_line,
    with_alpha,
};
use gpui::{
    AnyElement, ClipboardItem, Context, InteractiveElement, IntoElement, MouseButton,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};

use crate::{
    app::state::{AppState, ArtifactPreview, ArtifactPreviewBody},
    views::root::AfterlifeApp,
};

const PREVIEW_WIDTH_PX: f32 = 620.0;

fn format_byte_size(bytes: u64) -> String {
    if bytes < 1_024 {
        format!("{bytes} B")
    } else if bytes < 1_024 * 1_024 {
        format!("{:.1} KiB", bytes as f64 / 1_024.0)
    } else {
        format!("{:.2} MiB", bytes as f64 / (1_024.0 * 1_024.0))
    }
}

pub fn render_artifact_preview(
    st: &AppState,
    preview: &ArtifactPreview,
    window_height: f32,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    let max_body_h = (window_height - 210.0).clamp(180.0, 560.0);

    let mut meta_parts = vec![preview.kind_label.to_string()];
    if let Some(sz) = preview.size_bytes {
        meta_parts.push(format_byte_size(sz));
    }
    if let Some(ref sha) = preview.sha256 {
        let short = &sha[..sha.len().min(12)];
        meta_parts.push(format!("sha256:{short}"));
    }
    let meta_line = meta_parts.join(" · ");

    let copy_text = match &preview.body {
        ArtifactPreviewBody::Text { text, .. } => Some(text.clone()),
        _ => None,
    };

    GlassSurface::sheet(&p, &st.glass)
        .border(Some(p.border_rim))
        .build()
        .id("artifact-preview-sheet")
        .w(px(PREVIEW_WIDTH_PX))
        .overflow_hidden()
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |_, _, cx| {
            cx.stop_propagation();
        })
        .child(
            div()
                .flex()
                .flex_col()
                .gap(SpacePx::S12.px())
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_start()
                        .justify_between()
                        .gap(SpacePx::S12.px())
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .min_w(px(0.0))
                                .flex_1()
                                .child(
                                    div()
                                        .text_size(FontPx::F14.px())
                                        .text_color(p.text_primary)
                                        .child(SharedString::from(preview.label.clone())),
                                )
                                .child(
                                    div()
                                        .text_size(FontPx::F11.px())
                                        .text_color(with_alpha(p.text_tertiary, 0.82))
                                        .child(SharedString::from(meta_line)),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(SpacePx::S12.px())
                                .when_some(copy_text, |row, text_to_copy| {
                                    row.child(
                                        single_line(
                                            RowHeightPx::H22,
                                            SpacePx::S4,
                                            SpacePx::S0,
                                            FontPx::F11,
                                            RadiusPx::R0,
                                            None,
                                            None,
                                            p.accent,
                                            st.show_centerline,
                                            p.centerline,
                                            [SharedString::from("Copy").into_any_element()],
                                        )
                                        .id("artifact-preview-copy")
                                        .cursor_pointer()
                                        .hover(move |s| s.text_color(p.text_primary))
                                        .on_click(move |_, _, cx| {
                                            cx.stop_propagation();
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                text_to_copy.clone(),
                                            ));
                                        }),
                                    )
                                })
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
                                    .id("artifact-preview-close")
                                    .cursor_pointer()
                                    .hover(move |s| s.text_color(p.text_primary))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.state.update(cx, |st, _| {
                                            st.close_artifact_preview();
                                        });
                                        cx.stop_propagation();
                                        cx.notify();
                                    })),
                                ),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(1.0))
                        .bg(with_alpha(p.text_primary, 0.08)),
                )
                .child(
                    div()
                        .id("artifact-preview-body-scroll")
                        .max_h(px(max_body_h))
                        .overflow_y_scroll()
                        .restrict_scroll_to_axis()
                        .overflow_fade_bottom(FadePx::F16)
                        .px(SpacePx::S12.px())
                        .py(SpacePx::S12.px())
                        .rounded(p.radius_f32(&st.glass, 0.65))
                        .bg(with_alpha(p.text_primary, 0.028))
                        .child(match &preview.body {
                            ArtifactPreviewBody::Text { text, truncated } => div()
                                .flex()
                                .flex_col()
                                .gap(SpacePx::S8.px())
                                .child(
                                    div()
                                        .text_size(FontPx::F12.px())
                                        .text_color(p.text_secondary)
                                        .child(SharedString::from(text.clone())),
                                )
                                .when(*truncated, |col| {
                                    col.child(
                                        div()
                                            .text_size(FontPx::F11.px())
                                            .text_color(p.amber)
                                            .child(SharedString::from(
                                                "Preview truncated at 64 KiB.",
                                            )),
                                    )
                                })
                                .into_any_element(),
                            ArtifactPreviewBody::Binary { byte_len } => div()
                                .text_size(FontPx::F12.px())
                                .text_color(p.text_tertiary)
                                .child(SharedString::from(format!(
                                    "Binary blob payload ({}) stored and SHA-256 verified.",
                                    format_byte_size(*byte_len as u64)
                                )))
                                .into_any_element(),
                            ArtifactPreviewBody::Unavailable(msg) => div()
                                .text_size(FontPx::F12.px())
                                .text_color(p.coral)
                                .child(SharedString::from(msg.clone()))
                                .into_any_element(),
                        }),
                ),
        )
        .into_any_element()
}
