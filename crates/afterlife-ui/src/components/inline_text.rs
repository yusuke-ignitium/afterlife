use std::ops::Range;

use afterlife_core::{is_openable_url, parse_inline_links};
use afterlife_design::with_alpha;
use gpui::{
    AnyElement, ElementId, HighlightStyle, InteractiveElement, InteractiveText, IntoElement,
    ParentElement, Rgba, SharedString, StatefulInteractiveElement, Styled, StyledText,
    UnderlineStyle, div, px,
};

pub fn linkified_text(
    id: impl Into<ElementId>,
    text: &str,
    link_color: Rgba,
) -> AnyElement {
    let parsed = parse_inline_links(text);
    if parsed.links.is_empty() {
        return SharedString::from(parsed.display_text).into_any_element();
    }

    let underline = UnderlineStyle {
        thickness: px(1.0),
        color: Some(with_alpha(link_color, 0.45).into()),
        wavy: false,
    };
    let highlights: Vec<(Range<usize>, HighlightStyle)> = parsed
        .links
        .iter()
        .map(|link| {
            (
                link.range.clone(),
                HighlightStyle {
                    color: Some(link_color.into()),
                    underline: Some(underline),
                    ..Default::default()
                },
            )
        })
        .collect();
    let ranges: Vec<Range<usize>> = parsed.links.iter().map(|l| l.range.clone()).collect();
    let urls: Vec<String> = parsed.links.iter().map(|l| l.url.clone()).collect();

    InteractiveText::new(
        id,
        StyledText::new(parsed.display_text).with_highlights(highlights),
    )
    .on_click(ranges, move |ix, _window, cx| {
        if let Some(url) = urls.get(ix) {
            cx.stop_propagation();
            cx.open_url(url);
        }
    })
    .into_any_element()
}

pub struct LinkRowStyle {
    pub dot_color: Rgba,
    pub link_color: Rgba,
    pub plain_color: Rgba,
    pub show_centerline: bool,
    pub centerline: Rgba,
}

pub fn clickable_link_row(
    id: impl Into<ElementId>,
    label: String,
    uri: String,
    style: LinkRowStyle,
) -> AnyElement {
    let LinkRowStyle {
        dot_color,
        link_color,
        plain_color,
        show_centerline,
        centerline,
    } = style;
    if is_openable_url(&uri) {
        let open_uri = uri;
        div()
            .id(id)
            .flex()
            .flex_row()
            .items_center()
            .gap(afterlife_design::SpacePx::S8.px())
            .cursor_pointer()
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                cx.open_url(&open_uri);
            })
            .child(
                div()
                    .w(px(6.0))
                    .h(px(6.0))
                    .rounded(px(3.0))
                    .bg(dot_color),
            )
            .child(
                div()
                    .text_size(afterlife_design::FontPx::F11.px())
                    .text_color(link_color)
                    .hover(|s| s.underline())
                    .child(SharedString::from(label)),
            )
            .into_any_element()
    } else {
        crate::components::controls::status_dot_label(
            dot_color,
            label,
            plain_color,
            show_centerline,
            centerline,
        )
        .into_any_element()
    }
}
