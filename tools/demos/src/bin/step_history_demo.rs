use std::collections::HashSet;
use std::sync::Arc;

use afterlife_design::{
    FadePx, FontPx, GlassSettings, GlassSurface, Palette, RadiusPx, RowHeightPx, ScrollFadeExt,
    SpacePx, UiStyle, single_line, with_alpha,
};
use gpui::{
    AnyElement, App, AppContext, Application, Bounds, Context, Image, ImageFormat, ImageSource,
    InteractiveElement, IntoElement, ObjectFit, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, StyledImage, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, div, img, liquid_glass_styled,
    particle_wave_with, point, prelude::FluentBuilder, px, size,
};

const APP_TITLE_LOGO_PNG: &[u8] =
    include_bytes!("../../../../crates/afterlife-ui/assets/afterlife-logo.png");

struct DemoField {
    label: &'static str,
    value: &'static str,
}

struct DemoStep {
    stage_id: &'static str,
    sub_label: Option<&'static str>,
    runner: &'static str,
    time_label: &'static str,
    artifact: Option<&'static str>,
    fields: &'static [DemoField],
}

const STAGE_GROUPS: &[(&str, &str)] = &[
    ("discover", "Discover & Probe"),
    ("design", "Design Whole"),
    ("baseline_guard", "Baseline & Guard"),
    ("build", "Build & Hygiene"),
    ("verify", "Verify & Diff"),
];

const DEMO_STEPS: &[DemoStep] = &[
    DemoStep {
        stage_id: "discover",
        sub_label: Some("Trace Sources"),
        runner: "Claude Code",
        time_label: "21:02",
        artifact: None,
        fields: &[
            DemoField {
                label: "一次ソース",
                value: "engine/mod.rs, policy.rs, mcp.rs, left_pane.rs の全遷移経路を精査。",
            },
            DemoField {
                label: "事実と制約",
                value: "Sub-stage 遷移が未検証で通過しており、ExecutionContext に過去ログが未同梱。",
            },
        ],
    },
    DemoStep {
        stage_id: "design",
        sub_label: None,
        runner: "Claude Code",
        time_label: "21:08",
        artifact: None,
        fields: &[
            DemoField {
                label: "採用設計",
                value: "AdvanceJob を Agent 専用化し、ApprovalGate は ApproveJob (Human 専用) へ分離。",
            },
            DemoField {
                label: "見送った案",
                value: "spec_summary 追記は上書き破壊が起きるため棄却。別コマンド分離案は負荷が高いため棄却。",
            },
            DemoField {
                label: "不変条件",
                value: "新フィールドはすべて serde(default) で既存 DB 互換を維持。",
            },
        ],
    },
    DemoStep {
        stage_id: "baseline_guard",
        sub_label: Some("Capture Baseline"),
        runner: "Verifier",
        time_label: "21:14",
        artifact: Some("baseline-metrics.json"),
        fields: &[
            DemoField {
                label: "Before 実測",
                value: "全 7 クレート 63/63 pass、アイドル再描画 0fps、MCP schema sync 通過。",
            },
            DemoField {
                label: "再現条件",
                value: "同一ワークスペース構成で全クレートの自動テストおよびスキーマ一致検証を実行。",
            },
        ],
    },
    DemoStep {
        stage_id: "build",
        sub_label: Some("Apply Change"),
        runner: "Codex",
        time_label: "21:23",
        artifact: None,
        fields: &[
            DemoField {
                label: "変更方針",
                value: "policy::require_step_record と require_agent_execution をエンジン中核へ配置。",
            },
            DemoField {
                label: "変更内容",
                value: "StepRecord / StepFieldDef 追加、ApproveJob 分離、ExecutionContext 自動引き継ぎ。",
            },
        ],
    },
    DemoStep {
        stage_id: "verify",
        sub_label: Some("Before/After Diff"),
        runner: "Verifier",
        time_label: "21:29",
        artifact: None,
        fields: &[
            DemoField {
                label: "前後比較",
                value: "Before 63/63 → After 64/64 pass。警告ゼロ・コメントゼロ・スキーマ同期すべて維持。",
            },
            DemoField {
                label: "退行確認",
                value: "既存 E2E・並行クレーム・永続化後方互換テストがすべて通過。",
            },
        ],
    },
    DemoStep {
        stage_id: "verify",
        sub_label: Some("Bundle Evidence"),
        runner: "Claude Code",
        time_label: "21:31",
        artifact: Some("step-contract-report.md"),
        fields: &[
            DemoField {
                label: "Why 要約",
                value: "ステップ単位の判断・実行・証跡を構造的に強制し、AI 実行と人間承認の分離を徹底。",
            },
            DemoField {
                label: "確認ポイント",
                value: "baseline-metrics.json と step-contract-report.md を添付。",
            },
        ],
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum SheetHeightMode {
    Compact,
    Expanded,
}

impl SheetHeightMode {
    fn height_px(self) -> f32 {
        match self {
            Self::Compact => 420.0,
            Self::Expanded => 620.0,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Compact => "Sheet: 420px",
            Self::Expanded => "Sheet: 620px",
        }
    }
}

struct StepHistoryDemoApp {
    logo: Arc<Image>,
    glass: GlassSettings,
    palette: Palette,
    sheet_mode: SheetHeightMode,
    a_collapsed: HashSet<&'static str>,
    b_expanded: HashSet<&'static str>,
    c_collapsed: HashSet<&'static str>,
}

impl StepHistoryDemoApp {
    fn new() -> Self {
        let glass = GlassSettings::default();
        let palette = Palette::from_style(UiStyle::LiquidGlass, &glass);
        let logo = Arc::new(Image::from_bytes(
            ImageFormat::Png,
            APP_TITLE_LOGO_PNG.to_vec(),
        ));
        let mut b_expanded = HashSet::new();
        b_expanded.insert("baseline_guard");
        b_expanded.insert("verify");
        Self {
            logo,
            glass,
            palette,
            sheet_mode: SheetHeightMode::Expanded,
            a_collapsed: HashSet::new(),
            b_expanded,
            c_collapsed: HashSet::new(),
        }
    }

    fn pill_glass(&self, tint: gpui::Rgba, accent: f32) -> gpui::Background {
        liquid_glass_styled(
            tint,
            self.glass.disp_scale,
            self.glass.reach_px(),
            self.glass.chroma,
            self.glass.highlight_alpha,
            self.glass.rim_width,
            self.glass.blur_std_dev,
            7,
            accent,
        )
    }

    fn quiet_btn(&self, id: String, label: &'static str, primary: bool) -> AnyElement {
        let p = self.palette;
        let bg = if primary {
            with_alpha(p.accent, 0.24)
        } else {
            with_alpha(p.text_primary, 0.06)
        };
        let fg = if primary {
            p.text_primary
        } else {
            p.text_secondary
        };
        div()
            .id(SharedString::from(id))
            .cursor_pointer()
            .px(SpacePx::S12.px())
            .h(RowHeightPx::H28.px())
            .flex()
            .items_center()
            .rounded(p.radius_f32(&self.glass, 0.7))
            .bg(bg)
            .text_size(FontPx::F12.px())
            .text_color(fg)
            .child(label)
            .into_any_element()
    }

    fn artifact_row(&self, label: &str) -> AnyElement {
        let p = self.palette;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(SpacePx::S8.px())
            .pt(px(2.0))
            .child(
                div()
                    .w(px(4.0))
                    .h(px(4.0))
                    .rounded(px(2.0))
                    .bg(with_alpha(p.accent, 0.75)),
            )
            .child(
                div()
                    .text_size(FontPx::F11.px())
                    .text_color(p.accent)
                    .child(label.to_string()),
            )
            .into_any_element()
    }

    fn render_native_detail_header(&self) -> AnyElement {
        let p = self.palette;
        div()
            .flex()
            .flex_col()
            .gap(SpacePx::S8.px())
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
                        false,
                        p.centerline,
                        [SharedString::from("Job Detail").into_any_element()],
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(SpacePx::S8.px())
                            .child(single_line(
                                RowHeightPx::H22,
                                SpacePx::S0,
                                SpacePx::S4,
                                FontPx::F11,
                                RadiusPx::R0,
                                None,
                                None,
                                p.text_tertiary,
                                false,
                                p.centerline,
                                [SharedString::from("JOB-014").into_any_element()],
                            ))
                            .child(single_line(
                                RowHeightPx::H22,
                                SpacePx::S0,
                                SpacePx::S4,
                                FontPx::F11,
                                RadiusPx::R0,
                                None,
                                None,
                                p.text_tertiary,
                                false,
                                p.centerline,
                                [SharedString::from("Close").into_any_element()],
                            )),
                    ),
            )
            .child(
                div()
                    .text_size(FontPx::F13.px())
                    .text_color(p.text_primary)
                    .child("ステップごとの判断・実行ログと必須証跡契約を導入"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
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
                                RadiusPx::R0,
                                None,
                                None,
                                p.text_tertiary,
                                false,
                                p.centerline,
                                [SharedString::from("State").into_any_element()],
                            ))
                            .child(single_line(
                                RowHeightPx::H22,
                                SpacePx::S0,
                                SpacePx::S4,
                                FontPx::F11,
                                RadiusPx::R0,
                                None,
                                None,
                                p.amber,
                                false,
                                p.centerline,
                                [SharedString::from("Human Gate").into_any_element()],
                            )),
                    )
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
                                RadiusPx::R0,
                                None,
                                None,
                                p.text_tertiary,
                                false,
                                p.centerline,
                                [SharedString::from("District").into_any_element()],
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
                                false,
                                p.centerline,
                                [SharedString::from("Product Delivery").into_any_element()],
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_pattern_frame(
        &self,
        title: &'static str,
        subtitle: &'static str,
        scroll_id: &'static str,
        footer_prefix: &'static str,
        body: AnyElement,
    ) -> AnyElement {
        let p = self.palette;
        let sheet_h = self.sheet_mode.height_px();

        div()
            .flex()
            .flex_col()
            .w(px(368.0))
            .min_w(px(368.0))
            .h_full()
            .min_h(px(0.0))
            .gap(SpacePx::S8.px())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .px(SpacePx::S4.px())
                    .child(
                        div()
                            .text_size(FontPx::F13.px())
                            .text_color(p.text_primary)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(FontPx::F11.px())
                            .text_color(p.text_tertiary)
                            .child(subtitle),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .rounded(p.radius_f32(&self.glass, 1.4))
                    .bg(p.panel_base)
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .p(SpacePx::S12.px())
                            .gap(SpacePx::S8.px())
                            .opacity(0.38)
                            .child(
                                div()
                                    .text_size(FontPx::F12.px())
                                    .text_color(p.amber)
                                    .child("Approvals: 1"),
                            )
                            .child(
                                div()
                                    .p(SpacePx::S8.px())
                                    .rounded(p.radius_f32(&self.glass, 0.9))
                                    .bg(with_alpha(p.amber, 0.08))
                                    .text_size(FontPx::F11.px())
                                    .text_color(p.text_secondary)
                                    .child("JOB-014 · Waiting at Human Gate"),
                            ),
                    )
                    .child(
                        GlassSurface::sheet(&p, &self.glass)
                            .border(Some(p.border_rim))
                            .build()
                            .absolute()
                            .left(SpacePx::S4.px())
                            .right(SpacePx::S4.px())
                            .bottom(SpacePx::S4.px())
                            .h(px(sheet_h))
                            .px(SpacePx::S12.px())
                            .pt(SpacePx::S8.px())
                            .pb(SpacePx::S12.px())
                            .flex()
                            .flex_col()
                            .gap(SpacePx::S8.px())
                            .overflow_hidden()
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .justify_center()
                                    .child(
                                        div()
                                            .w(px(32.0))
                                            .h(px(2.0))
                                            .rounded(px(1.0))
                                            .bg(with_alpha(p.text_primary, 0.18)),
                                    ),
                            )
                            .child(
                                div()
                                    .id(scroll_id)
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h(px(0.0))
                                    .overflow_y_scroll()
                                    .restrict_scroll_to_axis()
                                    .overflow_fade_bottom(FadePx::F16)
                                    .gap(SpacePx::S12.px())
                                    .pb(SpacePx::S12.px())
                                    .child(self.render_native_detail_header())
                                    .child(body),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(SpacePx::S8.px())
                                    .pt(SpacePx::S4.px())
                                    .child(self.quiet_btn(
                                        format!("{footer_prefix}-approve"),
                                        "Approve",
                                        true,
                                    ))
                                    .child(self.quiet_btn(
                                        format!("{footer_prefix}-rework"),
                                        "Rework",
                                        false,
                                    ))
                                    .child(self.quiet_btn(
                                        format!("{footer_prefix}-archive"),
                                        "Archive",
                                        false,
                                    )),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_3a_hairline(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let body = div()
            .flex()
            .flex_col()
            .gap(SpacePx::S12.px())
            .child(single_line(
                RowHeightPx::H22,
                SpacePx::S0,
                SpacePx::S4,
                FontPx::F11,
                RadiusPx::R0,
                None,
                None,
                p.text_tertiary,
                false,
                p.centerline,
                [SharedString::from("Steps").into_any_element()],
            ))
            .children(STAGE_GROUPS.iter().map(|&(stage_id, stage_label)| {
                let steps: Vec<&DemoStep> = DEMO_STEPS
                    .iter()
                    .filter(|s| s.stage_id == stage_id)
                    .collect();
                let collapsed = self.a_collapsed.contains(stage_id);
                let has_art = steps.iter().any(|s| s.artifact.is_some());

                div()
                    .flex()
                    .flex_col()
                    .gap(SpacePx::S4.px())
                    .child(
                        div()
                            .id(SharedString::from(format!("a-stage-{stage_id}")))
                            .cursor_pointer()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.a_collapsed.remove(stage_id) {
                                    this.a_collapsed.insert(stage_id);
                                }
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(SpacePx::S8.px())
                                    .child(
                                        div()
                                            .w(px(5.0))
                                            .h(px(5.0))
                                            .rounded(px(3.0))
                                            .bg(if has_art {
                                                with_alpha(p.accent, 0.8)
                                            } else {
                                                with_alpha(p.text_tertiary, 0.6)
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_size(FontPx::F12.px())
                                            .text_color(p.text_primary)
                                            .child(stage_label),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(FontPx::F11.px())
                                    .text_color(with_alpha(p.text_tertiary, 0.7))
                                    .child(if collapsed { "▸" } else { "▾" }),
                            ),
                    )
                    .when(!collapsed, |d| {
                        d.child(
                            div()
                                .ml(px(2.0))
                                .pl(SpacePx::S12.px())
                                .border_l_1()
                                .border_color(with_alpha(p.text_primary, 0.08))
                                .flex()
                                .flex_col()
                                .gap(SpacePx::S8.px())
                                .children(steps.into_iter().map(|step| {
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(SpacePx::S4.px())
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .justify_between()
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F11.px())
                                                        .text_color(p.text_secondary)
                                                        .child(step.sub_label.unwrap_or(step.runner)),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F11.px())
                                                        .text_color(with_alpha(p.text_tertiary, 0.75))
                                                        .child(if step.sub_label.is_some() {
                                                            format!("{} · {}", step.runner, step.time_label)
                                                        } else {
                                                            step.time_label.to_string()
                                                        }),
                                                ),
                                        )
                                        .children(step.fields.iter().map(|f| {
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(1.0))
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F11.px())
                                                        .text_color(with_alpha(p.text_tertiary, 0.8))
                                                        .child(f.label),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F12.px())
                                                        .text_color(p.text_secondary)
                                                        .child(f.value),
                                                )
                                        }))
                                        .when_some(step.artifact, |col, art| {
                                            col.child(self.artifact_row(art))
                                        })
                                })),
                        )
                    })
            }))
            .into_any_element();

        self.render_pattern_frame(
            "3A. Hairline Whisper",
            "no cards, no pills · 8% hairline rail + native typography",
            "scroll-3a",
            "p3a",
            body,
        )
    }

    fn render_3b_pure_typo(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let body = div()
            .flex()
            .flex_col()
            .gap(SpacePx::S8.px())
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
                        RadiusPx::R0,
                        None,
                        None,
                        p.text_tertiary,
                        false,
                        p.centerline,
                        [SharedString::from("Steps").into_any_element()],
                    ))
                    .child(
                        div()
                            .id("b-toggle-all")
                            .cursor_pointer()
                            .text_size(FontPx::F11.px())
                            .text_color(p.text_tertiary)
                            .hover(move |s| s.text_color(p.text_primary))
                            .child(if self.b_expanded.len() == STAGE_GROUPS.len() {
                                "Fold"
                            } else {
                                "All"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.b_expanded.len() == STAGE_GROUPS.len() {
                                    this.b_expanded.clear();
                                    this.b_expanded.insert("verify");
                                } else {
                                    for &(sid, _) in STAGE_GROUPS {
                                        this.b_expanded.insert(sid);
                                    }
                                }
                                cx.notify();
                            })),
                    ),
            )
            .children(STAGE_GROUPS.iter().map(|&(stage_id, stage_label)| {
                let steps: Vec<&DemoStep> = DEMO_STEPS
                    .iter()
                    .filter(|s| s.stage_id == stage_id)
                    .collect();
                let open = self.b_expanded.contains(stage_id);
                let has_art = steps.iter().any(|s| s.artifact.is_some());
                let preview = steps
                    .last()
                    .and_then(|s| s.fields.first())
                    .map(|f| f.value)
                    .unwrap_or("");

                div()
                    .flex()
                    .flex_col()
                    .gap(SpacePx::S4.px())
                    .child(
                        div()
                            .id(SharedString::from(format!("b-stage-{stage_id}")))
                            .cursor_pointer()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.b_expanded.remove(stage_id) {
                                    this.b_expanded.insert(stage_id);
                                }
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap(SpacePx::S8.px())
                                            .child(
                                                div()
                                                    .w(px(4.0))
                                                    .h(px(4.0))
                                                    .rounded(px(2.0))
                                                    .bg(if open || has_art {
                                                        with_alpha(p.accent, 0.8)
                                                    } else {
                                                        with_alpha(p.text_tertiary, 0.5)
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .text_size(FontPx::F12.px())
                                                    .text_color(if open {
                                                        p.text_primary
                                                    } else {
                                                        p.text_secondary
                                                    })
                                                    .child(stage_label),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(FontPx::F11.px())
                                            .text_color(with_alpha(p.text_tertiary, 0.7))
                                            .child(steps.last().map(|s| s.time_label).unwrap_or("")),
                                    ),
                            )
                            .when(!open, |d| {
                                d.child(
                                    div()
                                        .pl(SpacePx::S12.px())
                                        .text_size(FontPx::F11.px())
                                        .text_color(p.text_tertiary)
                                        .truncate()
                                        .child(preview),
                                )
                            }),
                    )
                    .when(open, |d| {
                        d.child(
                            div()
                                .pl(SpacePx::S12.px())
                                .pb(SpacePx::S4.px())
                                .flex()
                                .flex_col()
                                .gap(SpacePx::S8.px())
                                .children(steps.into_iter().map(|step| {
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(SpacePx::S4.px())
                                        .when_some(step.sub_label, |col, sub| {
                                            col.child(
                                                div()
                                                    .text_size(FontPx::F11.px())
                                                    .text_color(p.text_tertiary)
                                                    .child(format!("{sub} · {}", step.runner)),
                                            )
                                        })
                                        .children(step.fields.iter().map(|f| {
                                            div()
                                                .text_size(FontPx::F12.px())
                                                .text_color(p.text_secondary)
                                                .child(format!("{} — {}", f.label, f.value))
                                        }))
                                        .when_some(step.artifact, |col, art| {
                                            col.child(self.artifact_row(art))
                                        })
                                })),
                        )
                    })
            }))
            .into_any_element();

        self.render_pattern_frame(
            "3B. Pure Typography",
            "zero lines, zero boxes · indent + quiet key—value prose",
            "scroll-3b",
            "p3b",
            body,
        )
    }

    fn render_3c_quiet_wash(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let body = div()
            .flex()
            .flex_col()
            .gap(SpacePx::S8.px())
            .child(single_line(
                RowHeightPx::H22,
                SpacePx::S0,
                SpacePx::S4,
                FontPx::F11,
                RadiusPx::R0,
                None,
                None,
                p.text_tertiary,
                false,
                p.centerline,
                [SharedString::from("Steps").into_any_element()],
            ))
            .children(STAGE_GROUPS.iter().map(|&(stage_id, stage_label)| {
                let steps: Vec<&DemoStep> = DEMO_STEPS
                    .iter()
                    .filter(|s| s.stage_id == stage_id)
                    .collect();
                let collapsed = self.c_collapsed.contains(stage_id);
                let has_art = steps.iter().any(|s| s.artifact.is_some());

                div()
                    .flex()
                    .flex_col()
                    .gap(SpacePx::S4.px())
                    .child(
                        div()
                            .id(SharedString::from(format!("c-stage-{stage_id}")))
                            .cursor_pointer()
                            .px(SpacePx::S4.px())
                            .py(px(2.0))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.c_collapsed.remove(stage_id) {
                                    this.c_collapsed.insert(stage_id);
                                }
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(SpacePx::S8.px())
                                    .child(
                                        div()
                                            .text_size(FontPx::F12.px())
                                            .text_color(p.text_primary)
                                            .child(stage_label),
                                    )
                                    .when(has_art, |row| {
                                        row.child(
                                            div()
                                                .w(px(4.0))
                                                .h(px(4.0))
                                                .rounded(px(2.0))
                                                .bg(p.accent),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(FontPx::F11.px())
                                    .text_color(with_alpha(p.text_tertiary, 0.7))
                                    .child(if collapsed {
                                        format!("{}", steps.len())
                                    } else {
                                        steps.last().map(|s| s.time_label).unwrap_or("").to_string()
                                    }),
                            ),
                    )
                    .when(!collapsed, |d| {
                        d.child(
                            div()
                                .px(SpacePx::S8.px())
                                .py(SpacePx::S8.px())
                                .rounded(p.radius_f32(&self.glass, 0.65))
                                .bg(with_alpha(p.text_primary, 0.028))
                                .flex()
                                .flex_col()
                                .gap(SpacePx::S8.px())
                                .children(steps.into_iter().map(|step| {
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(SpacePx::S4.px())
                                        .when_some(step.sub_label, |col, sub| {
                                            col.child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .justify_between()
                                                    .child(
                                                        div()
                                                            .text_size(FontPx::F11.px())
                                                            .text_color(p.text_secondary)
                                                            .child(sub),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(FontPx::F11.px())
                                                            .text_color(with_alpha(p.text_tertiary, 0.7))
                                                            .child(step.runner),
                                                    ),
                                            )
                                        })
                                        .children(step.fields.iter().map(|f| {
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(1.0))
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F11.px())
                                                        .text_color(with_alpha(p.text_tertiary, 0.78))
                                                        .child(f.label),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(FontPx::F12.px())
                                                        .text_color(p.text_secondary)
                                                        .child(f.value),
                                                )
                                        }))
                                        .when_some(step.artifact, |col, art| {
                                            col.child(self.artifact_row(art))
                                        })
                                })),
                        )
                    })
            }))
            .into_any_element();

        self.render_pattern_frame(
            "3C. Quiet Wash",
            "borderless 2.8% glass wash per stage · dot only for evidence",
            "scroll-3c",
            "p3c",
            body,
        )
    }
}

impl Render for StepHistoryDemoApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;

        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(particle_wave_with(
                p.window_backdrop,
                p.wave_a,
                p.wave_b,
                self.glass.bg_brightness,
            ))
            .text_color(p.text_primary)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(px(46.0))
                    .pl(px(86.0))
                    .pr(SpacePx::S16.px())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(SpacePx::S16.px())
                            .child(
                                img(ImageSource::Image(self.logo.clone()))
                                    .h(px(22.0))
                                    .w(px(86.0))
                                    .object_fit(ObjectFit::Contain),
                            )
                            .child(
                                div()
                                    .text_size(FontPx::F12.px())
                                    .text_color(p.text_secondary)
                                    .child("Stage-Grouped Minimal Refinements (3A / 3B / 3C)"),
                            ),
                    )
                    .child(
                        div()
                            .id("toggle-sheet-height")
                            .cursor_pointer()
                            .px(SpacePx::S12.px())
                            .h(RowHeightPx::H24.px())
                            .flex()
                            .items_center()
                            .rounded(p.radius_f32(&self.glass, 0.8))
                            .bg(self.pill_glass(p.glass_selected, 0.45))
                            .text_size(FontPx::F12.px())
                            .text_color(p.text_primary)
                            .child(self.sheet_mode.label())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sheet_mode = match this.sheet_mode {
                                    SheetHeightMode::Compact => SheetHeightMode::Expanded,
                                    SheetHeightMode::Expanded => SheetHeightMode::Compact,
                                };
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_center()
                    .flex_1()
                    .min_h(px(0.0))
                    .p(SpacePx::S16.px())
                    .pt(SpacePx::S8.px())
                    .gap(SpacePx::S16.px())
                    .child(self.render_3a_hairline(cx))
                    .child(self.render_3b_pure_typo(cx))
                    .child(self.render_3c_quiet_wash(cx)),
            )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.activate(true);
        let bounds = Bounds::centered(None, size(px(1200.0), px(790.0)), cx);
        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_background: WindowBackgroundAppearance::Opaque,
                titlebar: Some(TitlebarOptions {
                    title: Some("Afterlife · Step History Minimal Demo (3A / 3B / 3C)".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(13.0), px(13.0))),
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| StepHistoryDemoApp::new()),
        );
    });
}
