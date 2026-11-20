use gpui::{Pixels, px};

pub const BORDER_PX: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpacePx {
    S0,
    S4,
    S8,
    S12,
    S16,
    S24,
    S32,
}

impl SpacePx {
    pub const fn val(self) -> f32 {
        match self {
            Self::S0 => 0.0,
            Self::S4 => 4.0,
            Self::S8 => 8.0,
            Self::S12 => 12.0,
            Self::S16 => 16.0,
            Self::S24 => 24.0,
            Self::S32 => 32.0,
        }
    }

    pub fn px(self) -> Pixels {
        px(self.val())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontPx {
    F11,
    F12,
    F13,
    F14,
    F16,
    F20,
}

impl FontPx {
    pub const fn val(self) -> f32 {
        match self {
            Self::F11 => 11.0,
            Self::F12 => 12.0,
            Self::F13 => 13.0,
            Self::F14 => 14.0,
            Self::F16 => 16.0,
            Self::F20 => 20.0,
        }
    }

    pub fn px(self) -> Pixels {
        px(self.val())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowHeightPx {
    H22,
    H24,
    H28,
    H32,
    H36,
}

impl RowHeightPx {
    pub const fn val(self) -> f32 {
        match self {
            Self::H22 => 22.0,
            Self::H24 => 24.0,
            Self::H28 => 28.0,
            Self::H32 => 32.0,
            Self::H36 => 36.0,
        }
    }

    pub fn px(self) -> Pixels {
        px(self.val())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadiusPx {
    R0,
    R2,
    R4,
    R6,
    R8,
}

impl RadiusPx {
    pub const fn val(self) -> f32 {
        match self {
            Self::R0 => 0.0,
            Self::R2 => 2.0,
            Self::R4 => 4.0,
            Self::R6 => 6.0,
            Self::R8 => 8.0,
        }
    }

    pub fn px(self) -> Pixels {
        px(self.val())
    }
}
