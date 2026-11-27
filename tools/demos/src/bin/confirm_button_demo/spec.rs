use std::time::{Duration, Instant};

use afterlife_design::{Palette, with_alpha};
use gpui::Rgba;

pub const DETAIL_TOGGLE_WIDTH_PX: f32 = 72.0;
pub const ARM_TRANSITION_MS: u64 = 220;
pub const DISARM_TRANSITION_MS: u64 = 200;
pub const EXEC_FLASH_MS: u64 = 320;
pub const AUTO_DISARM_MS: u64 = 3_000;

pub fn ease_out_cubic(t: f32) -> f32 {
    let c = t.clamp(0.0, 1.0);
    1.0 - (1.0 - c).powi(3)
}

pub fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

pub fn lerp_rgba(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let u = t.clamp(0.0, 1.0);
    Rgba {
        r: a.r + (b.r - a.r) * u,
        g: a.g + (b.g - a.g) * u,
        b: a.b + (b.b - a.b) * u,
        a: a.a + (b.a - a.a) * u,
    }
}

pub fn lerp_glass_rgba(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let u = t.clamp(0.0, 1.0);
    let out_a = a.a + (b.a - a.a) * u;
    if out_a <= 1e-4 {
        return Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
    }
    let ra = a.r * a.a;
    let ga = a.g * a.a;
    let ba = a.b * a.a;
    let rb = b.r * b.a;
    let gb = b.g * b.a;
    let bb = b.b * b.a;
    Rgba {
        r: ((ra + (rb - ra) * u) / out_a).clamp(0.0, 1.0),
        g: ((ga + (gb - ga) * u) / out_a).clamp(0.0, 1.0),
        b: ((ba + (bb - ba) * u) / out_a).clamp(0.0, 1.0),
        a: out_a.clamp(0.0, 1.0),
    }
}

#[derive(Clone, Debug)]
pub struct ScalarTransition {
    from: f32,
    to: f32,
    started_at: Instant,
    duration: Duration,
}

impl ScalarTransition {
    pub fn settled(val: f32, now: Instant) -> Self {
        Self { from: val, to: val, started_at: now, duration: Duration::ZERO }
    }

    pub fn set_target(&mut self, target: f32, duration: Duration, now: Instant) {
        if (self.to - target).abs() <= 1e-4 {
            return;
        }
        self.from = self.value(now);
        self.to = target;
        self.started_at = now;
        self.duration = duration;
    }

    pub fn value(&self, now: Instant) -> f32 {
        if self.duration.is_zero() || (self.to - self.from).abs() <= 1e-4 {
            return self.to;
        }
        let raw = now.saturating_duration_since(self.started_at).as_secs_f32()
            / self.duration.as_secs_f32();
        if raw >= 1.0 {
            return self.to;
        }
        lerp_f32(self.from, self.to, ease_out_cubic(raw))
    }

    pub fn is_active(&self, now: Instant) -> bool {
        !self.duration.is_zero()
            && (self.to - self.from).abs() > 1e-4
            && now.saturating_duration_since(self.started_at) < self.duration
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmVariant {
    B1SolidInversion,
    B2LuminousGlass,
    B3GlassBarCountdown,
}

impl ConfirmVariant {
    pub fn index(self) -> usize {
        match self {
            Self::B1SolidInversion => 0,
            Self::B2LuminousGlass => 1,
            Self::B3GlassBarCountdown => 2,
        }
    }

    pub fn id_prefix(self) -> &'static str {
        match self {
            Self::B1SolidInversion => "var-b1",
            Self::B2LuminousGlass => "var-b2",
            Self::B3GlassBarCountdown => "var-b3",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::B1SolidInversion => "B1 · Solid Accent Inversion",
            Self::B2LuminousGlass => "B2 · Luminous Glass Charge",
            Self::B3GlassBarCountdown => "B3 · Glass Tint + Bottom Gauge",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Self::B1SolidInversion => "Same label · smooth fill into solid accent + dark ink",
            Self::B2LuminousGlass => "Same label · smooth translucent glow + soft semantic rim",
            Self::B3GlassBarCountdown => "Same label · borderless tint boost + 2px decay bar",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionTone {
    Approve,
    Rework,
    Archive,
}

impl ActionTone {
    pub fn label(self) -> &'static str {
        match self {
            Self::Approve => "Approve",
            Self::Rework => "Rework",
            Self::Archive => "Archive",
        }
    }

    pub fn accent(self, p: Palette) -> Rgba {
        match self {
            Self::Approve => p.accent,
            Self::Rework => p.amber,
            Self::Archive => p.coral,
        }
    }

    pub fn idle_colors(self, p: Palette) -> (Rgba, Rgba) {
        match self {
            Self::Approve => (p.active_bg, p.accent),
            Self::Rework => (p.btn_bg, p.text_secondary),
            Self::Archive => (Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }, p.text_tertiary),
        }
    }

    pub fn armed_colors(self, p: Palette, variant: ConfirmVariant) -> (Rgba, Rgba) {
        match (variant, self) {
            (ConfirmVariant::B1SolidInversion, Self::Approve) => (p.accent, p.window_backdrop),
            (ConfirmVariant::B1SolidInversion, Self::Rework) => (p.amber, p.window_backdrop),
            (ConfirmVariant::B1SolidInversion, Self::Archive) => (p.coral, p.window_backdrop),
            (ConfirmVariant::B2LuminousGlass, Self::Approve) => {
                (with_alpha(p.accent_inactive, 0.88), p.text_primary)
            }
            (ConfirmVariant::B2LuminousGlass, Self::Rework) => {
                (with_alpha(p.amber_inactive, 0.78), p.amber)
            }
            (ConfirmVariant::B2LuminousGlass, Self::Archive) => {
                (with_alpha(p.coral_inactive, 0.72), p.coral)
            }
            (ConfirmVariant::B3GlassBarCountdown, Self::Approve) => {
                (with_alpha(p.accent, 0.30), p.text_primary)
            }
            (ConfirmVariant::B3GlassBarCountdown, Self::Rework) => {
                (with_alpha(p.amber, 0.25), p.amber)
            }
            (ConfirmVariant::B3GlassBarCountdown, Self::Archive) => {
                (with_alpha(p.coral, 0.24), p.coral)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DemoContextKind {
    ApprovalGate,
    WorkStage,
    Queued,
    Proposal,
}

impl DemoContextKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::ApprovalGate => "ApprovalGate (4)",
            Self::WorkStage => "Work Stage (3)",
            Self::Queued => "Queued (1)",
            Self::Proposal => "Proposal (1)",
        }
    }

    pub fn header_title(self) -> &'static str {
        if self == Self::Proposal { "Proposal Detail" } else { "Job Detail" }
    }

    pub fn item_id(self) -> &'static str {
        match self {
            Self::ApprovalGate => "JOB-056",
            Self::WorkStage => "JOB-058",
            Self::Queued => "JOB-053",
            Self::Proposal => "PROP-SPEC-0002",
        }
    }

    pub fn title_text(self) -> &'static str {
        match self {
            Self::ApprovalGate => "文字変更なし・横ズレなし・色と光だけの滑らかな2段階確認",
            Self::WorkStage => "Work Stage: Rework / Archive は2回押し、Stop は1クリック即実行",
            Self::Queued => "Queue Job: Ghost Archive ボタンの2段階色変化",
            Self::Proposal => "Governance Proposal: Approve の2段階色変化",
        }
    }

    pub fn stage_label(self, stopped: bool) -> String {
        let base = match self {
            Self::ApprovalGate => "Human Gate",
            Self::WorkStage => "Discover & Probe (trace_sources)",
            Self::Queued => "Jobs (Queued)",
            Self::Proposal => "DIST-002 · ProcedureSpec · S v1 → v2",
        };
        if stopped && matches!(self, Self::ApprovalGate | Self::WorkStage) {
            format!("{base} · Stopped")
        } else {
            base.to_string()
        }
    }
}
