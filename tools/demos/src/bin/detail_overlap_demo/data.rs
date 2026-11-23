#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OverlapPattern {
    WhisperFrost,
    FloatingAmbient,
    ClearRefractVeil,
    DockedMatte,
}

impl OverlapPattern {
    pub const ALL: [Self; 4] = [
        Self::WhisperFrost,
        Self::FloatingAmbient,
        Self::ClearRefractVeil,
        Self::DockedMatte,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::WhisperFrost => "1. Whisper Frost",
            Self::FloatingAmbient => "2. Floating Ambient",
            Self::ClearRefractVeil => "3. Clear Glass + Veil",
            Self::DockedMatte => "4. Docked Matte",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Self::WhisperFrost => "flush overlap · 11px frost · soft 2-tier upward shadow",
            Self::FloatingAmbient => "4px inset card · 14px frost · wide ambient lift",
            Self::ClearRefractVeil => "5px light frost · subtle refraction + 16px shadow veil",
            Self::DockedMatte => "bottom-docked · 16px deep matte · tight crisp shadow",
        }
    }

    pub fn id_prefix(self) -> &'static str {
        match self {
            Self::WhisperFrost => "p1",
            Self::FloatingAmbient => "p2",
            Self::ClearRefractVeil => "p3",
            Self::DockedMatte => "p4",
        }
    }
}

pub struct SampleJob {
    pub id: &'static str,
    pub ticket: Option<&'static str>,
    pub title: &'static str,
    pub spec: &'static str,
}

pub const SAMPLE_JOBS: &[SampleJob] = &[
    SampleJob {
        id: "JOB-041",
        ticket: Some("YKZ-104"),
        title: "UI の px トークンと縦中央揃えの決定論的ガードを適用",
        spec: "Guard SpacePx / FontPx / RowHeightPx / FadePx across all desktop panes.",
    },
    SampleJob {
        id: "JOB-042",
        ticket: Some("YKZ-108"),
        title: "特許の件に「AI マッチング」とは別の、人材に寄り添う提案だと伝わる呼称をつける",
        spec: "Explore candidate naming grounded in human-centered curation rather than algorithmic matching.",
    },
    SampleJob {
        id: "JOB-043",
        ticket: Some("ENG-219"),
        title: "SQLite WAL 競合ロックとリース期限切れの自動検証スイートを拡充",
        spec: "Add concurrent 16-worker claim contention and zombie fencing token rejection checks.",
    },
    SampleJob {
        id: "JOB-044",
        ticket: Some("YKZ-112"),
        title: "OAuth 2.1 Broker のスコープ分離とトークン失効パスを監査",
        spec: "Verify human approval routes stay fail-closed under agent-scoped tokens.",
    },
    SampleJob {
        id: "JOB-045",
        ticket: None,
        title: "ローカル MCP 接続のレスポンス p95 とメモリ RSS を定点計測",
        spec: "Capture steady-state telemetry under 30Hz EventBus bursts with zero idle frames.",
    },
    SampleJob {
        id: "JOB-046",
        ticket: Some("OPS-087"),
        title: "District 02 のリサーチ手順書 v3 へフィードバックを統合",
        spec: "Incorporate rework feedback into Philosophy v2 and ProcedureSpec v3 draft.",
    },
    SampleJob {
        id: "JOB-047",
        ticket: Some("YKZ-121"),
        title: "マルチモニタ環境でのウィンドウ復元と最小サイズクランプを検証",
        spec: "Ensure disconnected display fallback restores safely inside primary visibleFrame.",
    },
];

