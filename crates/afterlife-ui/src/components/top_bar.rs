use afterlife_design::{GlassSurface, RowHeightPx, SpacePx, UiStyle};
use gpui::{
    AnyElement, ClickEvent, Context, ImageSource, InteractiveElement, IntoElement, ObjectFit,
    ParentElement, StatefulInteractiveElement, Styled, StyledImage, div, img, px,
};

use crate::{
    app::state::{AppState, TabId},
    components::controls::nav_item,
    views::root::AfterlifeApp,
};

fn render_app_title(st: &AppState) -> AnyElement {
    div()
        .id("app-title")
        .flex()
        .items_center()
        .h(RowHeightPx::H36.px())
        .child(
            img(ImageSource::Image(st.app_title_logo.clone()))
                .w(px(140.0))
                .h(px(36.0))
                .object_fit(ObjectFit::Contain),
        )
        .into_any_element()
}

pub fn render_top_bar(st: &AppState, cx: &mut Context<AfterlifeApp>) -> AnyElement {
    let p = st.palette;

    GlassSurface::panel(&p, &st.glass)
        .pad(SpacePx::S0)
        .gap(SpacePx::S8)
        .border(None)
        .bg(None)
        .build()
        .id("top-bar-header")
        .py(SpacePx::S4.px())
        .w_full()
        .on_click(|ev: &ClickEvent, window, _| {
            if ev.click_count() == 2 {
                window.zoom_window();
            }
        })
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .w_full()
                .gap(SpacePx::S12.px())
                .child(render_app_title(st))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(SpacePx::S8.px())
                        .child(nav_item(
                            st,
                            "tab-ws",
                            "Workspace",
                            SpacePx::S12,
                            st.active_tab == TabId::Workspace,
                            |st, _, _| st.active_tab = TabId::Workspace,
                            cx,
                        ))
                        .child(nav_item(
                            st,
                            "tab-tmpl",
                            "Sub-SM Templates",
                            SpacePx::S12,
                            st.active_tab == TabId::Templates,
                            |st, _, _| st.active_tab = TabId::Templates,
                            cx,
                        ))
                        .child(nav_item(
                            st,
                            "tab-guard",
                            "Px & Align Guard",
                            SpacePx::S12,
                            st.active_tab == TabId::Guard,
                            |st, _, _| st.active_tab = TabId::Guard,
                            cx,
                        ))
                        .child(nav_item(
                            st,
                            "toggle-style",
                            match st.style {
                                UiStyle::LiquidGlass => "Liquid Glass",
                                UiStyle::Wireframe => "Wire 0px",
                            },
                            SpacePx::S8,
                            st.style == UiStyle::LiquidGlass,
                            |st, _, _| st.toggle_style(),
                            cx,
                        ))
                        .child(nav_item(
                            st,
                            "toggle-glass-cfg",
                            "Appearance Config",
                            SpacePx::S8,
                            st.settings_open,
                            |st, _, _| st.settings_open = !st.settings_open,
                            cx,
                        ))
                        .child(nav_item(
                            st,
                            "toggle-centerline",
                            "Centerline",
                            SpacePx::S8,
                            st.show_centerline,
                            |st, _, _| st.show_centerline = !st.show_centerline,
                            cx,
                        )),
                ),
        )
        .into_any_element()
}
