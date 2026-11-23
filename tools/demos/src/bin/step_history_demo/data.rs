pub struct DemoField {
    pub label: &'static str,
    pub value: &'static str,
}

pub struct DemoStep {
    pub stage_id: &'static str,
    pub sub_label: Option<&'static str>,
    pub runner: &'static str,
    pub time_label: &'static str,
    pub artifact: Option<&'static str>,
    pub fields: &'static [DemoField],
}

pub const STAGE_GROUPS: &[(&str, &str)] = &[
    ("discover", "Discover & Probe"),
    ("design", "Design Whole"),
    ("baseline_guard", "Baseline & Guard"),
    ("build", "Build & Hygiene"),
    ("verify", "Verify & Diff"),
];

pub const DEMO_STEPS: &[DemoStep] = &[
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
pub enum SheetHeightMode {
    Compact,
    Expanded,
}

impl SheetHeightMode {
    pub fn height_px(self) -> f32 {
        match self {
            Self::Compact => 420.0,
            Self::Expanded => 620.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Compact => "Sheet: 420px",
            Self::Expanded => "Sheet: 620px",
        }
    }
}
