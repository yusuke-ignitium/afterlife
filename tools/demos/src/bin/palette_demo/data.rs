use gpui::{Rgba, rgb};

#[derive(Clone, Copy)]
pub struct ColorRole {
    pub role: &'static str,
    pub sample_id: &'static str,
    pub sample_title: &'static str,
    pub sample_runner: &'static str,
    pub active: Rgba,
    pub inactive: Rgba,
    pub active_hex: &'static str,
    pub inactive_hex: &'static str,
}

#[derive(Clone, Copy)]
pub struct BackdropVariant {
    pub name: &'static str,
    pub bg_base: Rgba,
    pub wave_a: Rgba,
    pub wave_b: Rgba,
}

pub fn with_alpha(c: Rgba, a: f32) -> Rgba {
    Rgba {
        r: c.r,
        g: c.g,
        b: c.b,
        a,
    }
}

pub fn base_roles() -> [ColorRole; 8] {
    [
        ColorRole {
            role: "1. Neutral",
            sample_id: "JOB-040",
            sample_title: "Queued raw drop awaiting Runner claim",
            sample_runner: "Intake Daemon",
            active: rgb(0xf1f3f6),
            inactive: rgb(0x7d8594),
            active_hex: "#F1F3F6",
            inactive_hex: "#7D8594",
        },
        ColorRole {
            role: "2. Platinum",
            sample_id: "JOB-041",
            sample_title: "Selected focus & active navigation",
            sample_runner: "Active Focus",
            active: rgb(0xe2e8f0),
            inactive: rgb(0x4a5568),
            active_hex: "#E2E8F0",
            inactive_hex: "#4A5568",
        },
        ColorRole {
            role: "3. Ice Cyan",
            sample_id: "JOB-042",
            sample_title: "Live Runner execution & UDS stream",
            sample_runner: "Dots (Running)",
            active: rgb(0x72d6ec),
            inactive: rgb(0x2d5866),
            active_hex: "#72D6EC",
            inactive_hex: "#2D5866",
        },
        ColorRole {
            role: "4. Cobalt",
            sample_id: "JOB-043",
            sample_title: "PR & CI watch sub-statemachine",
            sample_runner: "CI Poller",
            active: rgb(0x7aa8f8),
            inactive: rgb(0x304670),
            active_hex: "#7AA8F8",
            inactive_hex: "#304670",
        },
        ColorRole {
            role: "5. Indigo",
            sample_id: "JOB-044",
            sample_title: "AI design & synthesis stage",
            sample_runner: "Claude Duet",
            active: rgb(0xa69bff),
            inactive: rgb(0x454175),
            active_hex: "#A69BFF",
            inactive_hex: "#454175",
        },
        ColorRole {
            role: "6. Mint",
            sample_id: "JOB-045",
            sample_title: "Formal verification & tests passed",
            sample_runner: "Z3 Verifier",
            active: rgb(0x74d6b2),
            inactive: rgb(0x2c594b),
            active_hex: "#74D6B2",
            inactive_hex: "#2C594B",
        },
        ColorRole {
            role: "7. Champagne",
            sample_id: "JOB-046",
            sample_title: "Waiting at Approval gate",
            sample_runner: "You",
            active: rgb(0xdfbe7a),
            inactive: rgb(0x685838),
            active_hex: "#DFBE7A",
            inactive_hex: "#685838",
        },
        ColorRole {
            role: "8. Coral",
            sample_id: "JOB-047",
            sample_title: "Rework requested or CI check failed",
            sample_runner: "Rework Alert",
            active: rgb(0xeb7a8a),
            inactive: rgb(0x6b343e),
            active_hex: "#EB7A8A",
            inactive_hex: "#6B343E",
        },
    ]
}

pub fn backdrops() -> [BackdropVariant; 3] {
    [
        BackdropVariant {
            name: "Silver Mono",
            bg_base: rgb(0x090a0d),
            wave_a: rgb(0x4a5568),
            wave_b: rgb(0x718096),
        },
        BackdropVariant {
            name: "Abyssal Slate",
            bg_base: rgb(0x07090e),
            wave_a: rgb(0x2d5866),
            wave_b: rgb(0x304670),
        },
        BackdropVariant {
            name: "Pure Obsidian",
            bg_base: rgb(0x070709),
            wave_a: rgb(0x3a3f4a),
            wave_b: rgb(0x525866),
        },
    ]
}
