use gpui::{Rgba, rgb};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorRole {
    Neutral,
    Platinum,
    IceCyan,
    Cobalt,
    Indigo,
    Mint,
    Champagne,
    Coral,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorState {
    Active,
    Inactive,
}

impl ColorRole {
    pub const ALL: [ColorRole; 8] = [
        Self::Neutral,
        Self::Platinum,
        Self::IceCyan,
        Self::Cobalt,
        Self::Indigo,
        Self::Mint,
        Self::Champagne,
        Self::Coral,
    ];

    pub fn rgba(self, state: ColorState) -> Rgba {
        match (self, state) {
            (Self::Neutral, ColorState::Active) => rgb(0xf1f3f6),
            (Self::Neutral, ColorState::Inactive) => rgb(0x7d8594),
            (Self::Platinum, ColorState::Active) => rgb(0xe2e8f0),
            (Self::Platinum, ColorState::Inactive) => rgb(0x4a5568),
            (Self::IceCyan, ColorState::Active) => rgb(0x72d6ec),
            (Self::IceCyan, ColorState::Inactive) => rgb(0x2d5866),
            (Self::Cobalt, ColorState::Active) => rgb(0x7aa8f8),
            (Self::Cobalt, ColorState::Inactive) => rgb(0x304670),
            (Self::Indigo, ColorState::Active) => rgb(0xa69bff),
            (Self::Indigo, ColorState::Inactive) => rgb(0x454175),
            (Self::Mint, ColorState::Active) => rgb(0x74d6b2),
            (Self::Mint, ColorState::Inactive) => rgb(0x2c594b),
            (Self::Champagne, ColorState::Active) => rgb(0xdfbe7a),
            (Self::Champagne, ColorState::Inactive) => rgb(0x685838),
            (Self::Coral, ColorState::Active) => rgb(0xeb7a8a),
            (Self::Coral, ColorState::Inactive) => rgb(0x6b343e),
        }
    }
}

pub fn with_alpha(c: Rgba, a: f32) -> Rgba {
    Rgba {
        r: c.r,
        g: c.g,
        b: c.b,
        a: a.clamp(0.0, 1.0),
    }
}
