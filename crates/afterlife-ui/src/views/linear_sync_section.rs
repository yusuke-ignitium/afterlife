use afterlife_design::{FontPx, RadiusPx, RowHeightPx, SpacePx, with_alpha};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};

use crate::{
    app::{
        linear_actions::{
            disconnect_linear, export_skills_now, read_linear_status, trigger_linear_sync_async,
        },
        state::AppState,
    },
    views::root::AfterlifeApp,
};

fn render_action_pill(
    st: &AppState,
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&mut AfterlifeApp, &mut Context<AfterlifeApp>) + 'static,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .h(RowHeightPx::H24.px())
        .px(SpacePx::S8.px())
        .rounded(p.radius(RadiusPx::R4).px())
        .border_1()
        .border_color(with_alpha(p.text_primary, 0.14))
        .bg(with_alpha(p.text_primary, 0.06))
        .hover(move |s| {
            s.bg(with_alpha(p.text_primary, 0.14))
                .border_color(with_alpha(p.text_primary, 0.24))
        })
        .cursor_pointer()
        .text_size(FontPx::F11.px())
        .text_color(p.text_primary)
        .on_click(cx.listener(move |this, _, _, cx| {
            on_click(this, cx);
            cx.stop_propagation();
            cx.notify();
        }))
        .child(label)
        .into_any_element()
}

pub fn render_linear_and_skills_section(
    st: &AppState,
    cx: &mut Context<AfterlifeApp>,
) -> AnyElement {
    let p = st.palette;
    let status = read_linear_status(st);
    let connected = status.as_ref().is_some_and(|s| s.connected);

    let summary_text = match &status {
        Some(s) if s.connected => {
            let who = s
                .viewer_name
                .as_deref()
                .or(s.viewer_email.as_deref())
                .unwrap_or("Linear");
            format!(
                "Connected as {who} · {} started issues linked",
                s.linked_issues.len()
            )
        }
        Some(s) => format!(
            "Not connected · Callback: {}",
            s.redirect_uri
        ),
        None => "Offline mode (ephemeral workspace)".to_string(),
    };

    let error_line = status.as_ref().and_then(|s| s.last_error.clone());

    let mut action_row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(SpacePx::S8.px())
        .child(render_action_pill(
            st,
            "btn-export-district-skills",
            "Export Skills",
            |this, cx| {
                let st = this.state.read(cx);
                match export_skills_now(st) {
                    Ok(msg) => eprintln!("[afterlife-ui] {msg}"),
                    Err(err) => eprintln!("[afterlife-ui] skill export failed: {err}"),
                }
            },
            cx,
        ));

    if connected {
        action_row = action_row
            .child(render_action_pill(
                st,
                "btn-linear-sync-now",
                "Sync Linear",
                |this, cx| {
                    trigger_linear_sync_async(this.state.read(cx));
                },
                cx,
            ))
            .child(render_action_pill(
                st,
                "btn-linear-disconnect",
                "Disconnect",
                |this, cx| {
                    disconnect_linear(this.state.read(cx));
                },
                cx,
            ));
    }

    let mut section = div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(SpacePx::S4.px())
        .pt(SpacePx::S8.px())
        .child(
            div()
                .text_size(FontPx::F11.px())
                .text_color(p.text_tertiary)
                .child("Linear Sync & District Skills"),
        )
        .child(
            div()
                .text_size(FontPx::F11.px())
                .text_color(p.text_secondary)
                .child(SharedString::from(summary_text)),
        )
        .child(action_row);

    if !connected && status.is_some() {
        section = section.child(
            div()
                .text_size(FontPx::F11.px())
                .font_family("SF Mono")
                .text_color(p.text_tertiary)
                .child("CLI: afterlife linear connect --client-id <ID> --open"),
        );
    }

    if let Some(err) = error_line {
        section = section.child(
            div()
                .pt(px(2.0))
                .text_size(FontPx::F11.px())
                .text_color(p.text_tertiary)
                .child(SharedString::from(format!("Last error: {err}"))),
        );
    }

    section.into_any_element()
}
