#[derive(Clone, Copy)]
pub struct DemoPr {
    pub repo: &'static str,
    pub number: u32,
    pub uri: &'static str,
}

impl DemoPr {
    pub fn display_label(self) -> String {
        format!("{} #{}", self.repo, self.number)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PrScenario {
    None,
    Single,
    Multiple,
    LongNames,
}

impl PrScenario {
    pub const ALL: [Self; 4] = [Self::None, Self::Single, Self::Multiple, Self::LongNames];

    pub fn id(self) -> &'static str {
        match self {
            Self::None => "sc-none",
            Self::Single => "sc-single",
            Self::Multiple => "sc-multi",
            Self::LongNames => "sc-long",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "0 PR",
            Self::Single => "1 PR",
            Self::Multiple => "3 PRs",
            Self::LongNames => "Long Repo",
        }
    }

    pub fn job_id(self) -> &'static str {
        match self {
            Self::None => "JOB-015",
            Self::Single => "JOB-016",
            Self::Multiple => "JOB-018",
            Self::LongNames => "JOB-019",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::None => "全テスト完走までのトータル待ち時間の計測と最速化（ビルド・リンク・実行時間）",
            Self::Single => "Districts ボードに Compact Mode（P3 フラットステージ・横幅揃え）を実装",
            Self::Multiple => "クロスリポジトリ OAuth 2.1 トークン失効とクライアント SDK 同期対応",
            Self::LongNames => "外部デザインシステム・クラウド制御プレーン間のトークン自動検証パイプライン構築",
        }
    }

    pub fn state_label(self) -> &'static str {
        match self {
            Self::None => "Shipped (Done)",
            Self::Single => "Approve",
            Self::Multiple => "Approve",
            Self::LongNames => "Verify (bundle_evidence)",
        }
    }

    pub fn runner_label(self) -> &'static str {
        match self {
            Self::None => "Unclaimed",
            Self::Single => "Unclaimed",
            Self::Multiple => "Unclaimed",
            Self::LongNames => "Antigravity",
        }
    }

    pub fn prs(self) -> &'static [DemoPr] {
        match self {
            Self::None => &[],
            Self::Single => &[DemoPr {
                repo: "afterlife",
                number: 1,
                uri: "https://github.com/example/afterlife/pull/1",
            }],
            Self::Multiple => &[
                DemoPr {
                    repo: "afterlife",
                    number: 14,
                    uri: "https://github.com/example/afterlife/pull/14",
                },
                DemoPr {
                    repo: "afterlife-cloud",
                    number: 42,
                    uri: "https://github.com/example/afterlife-cloud/pull/42",
                },
                DemoPr {
                    repo: "afterlife-sdk",
                    number: 7,
                    uri: "https://github.com/example/afterlife-sdk/pull/7",
                },
            ],
            Self::LongNames => &[
                DemoPr {
                    repo: "afterlife-control-plane-service",
                    number: 1284,
                    uri: "https://github.com/example/afterlife-control-plane-service/pull/1284",
                },
                DemoPr {
                    repo: "afterlife-design-system-tokens",
                    number: 309,
                    uri: "https://github.com/example/afterlife-design-system-tokens/pull/309",
                },
            ],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PrPattern {
    AQuietRightStack,
    BHoverPillWithArrow,
    CDotInlineList,
}

impl PrPattern {
    pub const ALL: [Self; 3] = [
        Self::AQuietRightStack,
        Self::BHoverPillWithArrow,
        Self::CDotInlineList,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::AQuietRightStack => "A · Quiet Right Stack",
            Self::BHoverPillWithArrow => "B · Hover Pill + Arrow",
            Self::CDotInlineList => "C · Dot Prefix Stack",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Self::AQuietRightStack => "Pure meta_row alignment · right-aligned {repo} #{n} links",
            Self::BHoverPillWithArrow => "Right-aligned hover tint pill + subtle ↗ on hover",
            Self::CDotInlineList => "Accent dot + {repo} #{n} right-aligned rows",
        }
    }

    pub fn prefix(self) -> &'static str {
        match self {
            Self::AQuietRightStack => "pa",
            Self::BHoverPillWithArrow => "pb",
            Self::CDotInlineList => "pc",
        }
    }
}
