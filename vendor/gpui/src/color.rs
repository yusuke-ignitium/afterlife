use anyhow::{Context as _, bail};
use schemars::{JsonSchema, json_schema};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};
use std::borrow::Cow;
use std::{
    fmt::{self, Display, Formatter},
    hash::{Hash, Hasher},
};

/// Convert an RGB hex color code number to a color type
pub fn rgb(hex: u32) -> Rgba {
    let [_, r, g, b] = hex.to_be_bytes().map(|b| (b as f32) / 255.0);
    Rgba { r, g, b, a: 1.0 }
}

/// Convert an RGBA hex color code number to [`Rgba`]
pub fn rgba(hex: u32) -> Rgba {
    let [r, g, b, a] = hex.to_be_bytes().map(|b| (b as f32) / 255.0);
    Rgba { r, g, b, a }
}

/// Swap from RGBA with premultiplied alpha to BGRA
pub(crate) fn swap_rgba_pa_to_bgra(color: &mut [u8]) {
    color.swap(0, 2);
    if color[3] > 0 {
        let a = color[3] as f32 / 255.;
        color[0] = (color[0] as f32 / a) as u8;
        color[1] = (color[1] as f32 / a) as u8;
        color[2] = (color[2] as f32 / a) as u8;
    }
}

/// An RGBA color
#[derive(PartialEq, Clone, Copy, Default)]
#[repr(C)]
pub struct Rgba {
    /// The red component of the color, in the range 0.0 to 1.0
    pub r: f32,
    /// The green component of the color, in the range 0.0 to 1.0
    pub g: f32,
    /// The blue component of the color, in the range 0.0 to 1.0
    pub b: f32,
    /// The alpha component of the color, in the range 0.0 to 1.0
    pub a: f32,
}

impl fmt::Debug for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "rgba({:#010x})", u32::from(*self))
    }
}

impl Rgba {
    /// Create a new [`Rgba`] color by blending this and another color together
    pub fn blend(&self, other: Rgba) -> Self {
        if other.a >= 1.0 {
            other
        } else if other.a <= 0.0 {
            *self
        } else {
            Rgba {
                r: (self.r * (1.0 - other.a)) + (other.r * other.a),
                g: (self.g * (1.0 - other.a)) + (other.g * other.a),
                b: (self.b * (1.0 - other.a)) + (other.b * other.a),
                a: self.a,
            }
        }
    }
}

impl From<Rgba> for u32 {
    fn from(rgba: Rgba) -> Self {
        let r = (rgba.r * 255.0) as u32;
        let g = (rgba.g * 255.0) as u32;
        let b = (rgba.b * 255.0) as u32;
        let a = (rgba.a * 255.0) as u32;
        (r << 24) | (g << 16) | (b << 8) | a
    }
}

struct RgbaVisitor;

impl Visitor<'_> for RgbaVisitor {
    type Value = Rgba;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string in the format #rrggbb or #rrggbbaa")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Rgba, E> {
        Rgba::try_from(value).map_err(E::custom)
    }
}

impl JsonSchema for Rgba {
    fn schema_name() -> Cow<'static, str> {
        "Rgba".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        json_schema!({
            "type": "string",
            "pattern": "^#([0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$"
        })
    }
}

impl<'de> Deserialize<'de> for Rgba {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(RgbaVisitor)
    }
}

impl Serialize for Rgba {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let r = (self.r * 255.0).round() as u8;
        let g = (self.g * 255.0).round() as u8;
        let b = (self.b * 255.0).round() as u8;
        let a = (self.a * 255.0).round() as u8;

        let s = format!("#{r:02x}{g:02x}{b:02x}{a:02x}");
        serializer.serialize_str(&s)
    }
}

impl From<Hsla> for Rgba {
    fn from(color: Hsla) -> Self {
        let h = color.h;
        let s = color.s;
        let l = color.l;

        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
        let m = l - c / 2.0;
        let cm = c + m;
        let xm = x + m;

        let (r, g, b) = match (h * 6.0).floor() as i32 {
            0 | 6 => (cm, xm, m),
            1 => (xm, cm, m),
            2 => (m, cm, xm),
            3 => (m, xm, cm),
            4 => (xm, m, cm),
            _ => (cm, m, xm),
        };

        Rgba {
            r: r.clamp(0., 1.),
            g: g.clamp(0., 1.),
            b: b.clamp(0., 1.),
            a: color.a,
        }
    }
}

impl TryFrom<&'_ str> for Rgba {
    type Error = anyhow::Error;

    fn try_from(value: &'_ str) -> Result<Self, Self::Error> {
        const RGB: usize = "rgb".len();
        const RGBA: usize = "rgba".len();
        const RRGGBB: usize = "rrggbb".len();
        const RRGGBBAA: usize = "rrggbbaa".len();

        const EXPECTED_FORMATS: &str = "Expected #rgb, #rgba, #rrggbb, or #rrggbbaa";
        const INVALID_UNICODE: &str = "invalid unicode characters in color";

        let Some(("", hex)) = value.trim().split_once('#') else {
            bail!("invalid RGBA hex color: '{value}'. {EXPECTED_FORMATS}");
        };

        let (r, g, b, a) = match hex.len() {
            RGB | RGBA => {
                let r = u8::from_str_radix(
                    hex.get(0..1).with_context(|| {
                        format!("{INVALID_UNICODE}: r component of #rgb/#rgba for value: '{value}'")
                    })?,
                    16,
                )?;
                let g = u8::from_str_radix(
                    hex.get(1..2).with_context(|| {
                        format!("{INVALID_UNICODE}: g component of #rgb/#rgba for value: '{value}'")
                    })?,
                    16,
                )?;
                let b = u8::from_str_radix(
                    hex.get(2..3).with_context(|| {
                        format!("{INVALID_UNICODE}: b component of #rgb/#rgba for value: '{value}'")
                    })?,
                    16,
                )?;
                let a = if hex.len() == RGBA {
                    u8::from_str_radix(
                        hex.get(3..4).with_context(|| {
                            format!("{INVALID_UNICODE}: a component of #rgba for value: '{value}'")
                        })?,
                        16,
                    )?
                } else {
                    0xf
                };

                /// Duplicates a given hex digit.
                /// E.g., `0xf` -> `0xff`.
                const fn duplicate(value: u8) -> u8 {
                    (value << 4) | value
                }

                (duplicate(r), duplicate(g), duplicate(b), duplicate(a))
            }
            RRGGBB | RRGGBBAA => {
                let r = u8::from_str_radix(
                    hex.get(0..2).with_context(|| {
                        format!(
                            "{}: r component of #rrggbb/#rrggbbaa for value: '{}'",
                            INVALID_UNICODE, value
                        )
                    })?,
                    16,
                )?;
                let g = u8::from_str_radix(
                    hex.get(2..4).with_context(|| {
                        format!(
                            "{INVALID_UNICODE}: g component of #rrggbb/#rrggbbaa for value: '{value}'"
                        )
                    })?,
                    16,
                )?;
                let b = u8::from_str_radix(
                    hex.get(4..6).with_context(|| {
                        format!(
                            "{INVALID_UNICODE}: b component of #rrggbb/#rrggbbaa for value: '{value}'"
                        )
                    })?,
                    16,
                )?;
                let a = if hex.len() == RRGGBBAA {
                    u8::from_str_radix(
                        hex.get(6..8).with_context(|| {
                            format!(
                                "{INVALID_UNICODE}: a component of #rrggbbaa for value: '{value}'"
                            )
                        })?,
                        16,
                    )?
                } else {
                    0xff
                };
                (r, g, b, a)
            }
            _ => bail!("invalid RGBA hex color: '{value}'. {EXPECTED_FORMATS}"),
        };

        Ok(Rgba {
            r: r as f32 / 255.,
            g: g as f32 / 255.,
            b: b as f32 / 255.,
            a: a as f32 / 255.,
        })
    }
}

/// An HSLA color
#[derive(Default, Copy, Clone, Debug)]
#[repr(C)]
pub struct Hsla {
    /// Hue, in a range from 0 to 1
    pub h: f32,

    /// Saturation, in a range from 0 to 1
    pub s: f32,

    /// Lightness, in a range from 0 to 1
    pub l: f32,

    /// Alpha, in a range from 0 to 1
    pub a: f32,
}

impl PartialEq for Hsla {
    fn eq(&self, other: &Self) -> bool {
        self.h
            .total_cmp(&other.h)
            .then(self.s.total_cmp(&other.s))
            .then(self.l.total_cmp(&other.l).then(self.a.total_cmp(&other.a)))
            .is_eq()
    }
}

impl PartialOrd for Hsla {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Hsla {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.h
            .total_cmp(&other.h)
            .then(self.s.total_cmp(&other.s))
            .then(self.l.total_cmp(&other.l).then(self.a.total_cmp(&other.a)))
    }
}

impl Eq for Hsla {}

impl Hash for Hsla {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u32(u32::from_be_bytes(self.h.to_be_bytes()));
        state.write_u32(u32::from_be_bytes(self.s.to_be_bytes()));
        state.write_u32(u32::from_be_bytes(self.l.to_be_bytes()));
        state.write_u32(u32::from_be_bytes(self.a.to_be_bytes()));
    }
}

impl Display for Hsla {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "hsla({:.2}, {:.2}%, {:.2}%, {:.2})",
            self.h * 360.,
            self.s * 100.,
            self.l * 100.,
            self.a
        )
    }
}

/// Construct an [`Hsla`] object from plain values
pub fn hsla(h: f32, s: f32, l: f32, a: f32) -> Hsla {
    Hsla {
        h: h.clamp(0., 1.),
        s: s.clamp(0., 1.),
        l: l.clamp(0., 1.),
        a: a.clamp(0., 1.),
    }
}

/// Pure black in [`Hsla`]
pub const fn black() -> Hsla {
    Hsla {
        h: 0.,
        s: 0.,
        l: 0.,
        a: 1.,
    }
}

/// Transparent black in [`Hsla`]
pub const fn transparent_black() -> Hsla {
    Hsla {
        h: 0.,
        s: 0.,
        l: 0.,
        a: 0.,
    }
}

/// Transparent white in [`Hsla`]
pub const fn transparent_white() -> Hsla {
    Hsla {
        h: 0.,
        s: 0.,
        l: 1.,
        a: 0.,
    }
}

/// Opaque grey in [`Hsla`], values will be clamped to the range [0, 1]
pub fn opaque_grey(lightness: f32, opacity: f32) -> Hsla {
    Hsla {
        h: 0.,
        s: 0.,
        l: lightness.clamp(0., 1.),
        a: opacity.clamp(0., 1.),
    }
}

/// Pure white in [`Hsla`]
pub const fn white() -> Hsla {
    Hsla {
        h: 0.,
        s: 0.,
        l: 1.,
        a: 1.,
    }
}

/// The color red in [`Hsla`]
pub const fn red() -> Hsla {
    Hsla {
        h: 0.,
        s: 1.,
        l: 0.5,
        a: 1.,
    }
}

/// The color blue in [`Hsla`]
pub const fn blue() -> Hsla {
    Hsla {
        h: 0.6666666667,
        s: 1.,
        l: 0.5,
        a: 1.,
    }
}

/// The color green in [`Hsla`]
pub const fn green() -> Hsla {
    Hsla {
        h: 0.3333333333,
        s: 1.,
        l: 0.25,
        a: 1.,
    }
}

/// The color yellow in [`Hsla`]
pub const fn yellow() -> Hsla {
    Hsla {
        h: 0.1666666667,
        s: 1.,
        l: 0.5,
        a: 1.,
    }
}

impl Hsla {
    /// Converts this HSLA color to an RGBA color.
    pub fn to_rgb(self) -> Rgba {
        self.into()
    }

    /// The color red
    pub const fn red() -> Self {
        red()
    }

    /// The color green
    pub const fn green() -> Self {
        green()
    }

    /// The color blue
    pub const fn blue() -> Self {
        blue()
    }

    /// The color black
    pub const fn black() -> Self {
        black()
    }

    /// The color white
    pub const fn white() -> Self {
        white()
    }

    /// The color transparent black
    pub const fn transparent_black() -> Self {
        transparent_black()
    }

    /// Returns true if the HSLA color is fully transparent, false otherwise.
    pub fn is_transparent(&self) -> bool {
        self.a == 0.0
    }

    /// Returns true if the HSLA color is fully opaque, false otherwise.
    pub fn is_opaque(&self) -> bool {
        self.a == 1.0
    }

    /// Blends `other` on top of `self` based on `other`'s alpha value. The resulting color is a combination of `self`'s and `other`'s colors.
    ///
    /// If `other`'s alpha value is 1.0 or greater, `other` color is fully opaque, thus `other` is returned as the output color.
    /// If `other`'s alpha value is 0.0 or less, `other` color is fully transparent, thus `self` is returned as the output color.
    /// Else, the output color is calculated as a blend of `self` and `other` based on their weighted alpha values.
    ///
    /// Assumptions:
    /// - Alpha values are contained in the range [0, 1], with 1 as fully opaque and 0 as fully transparent.
    /// - The relative contributions of `self` and `other` is based on `self`'s alpha value (`self.a`) and `other`'s  alpha value (`other.a`), `self` contributing `self.a * (1.0 - other.a)` and `other` contributing its own alpha value.
    /// - RGB color components are contained in the range [0, 1].
    /// - If `self` and `other` colors are out of the valid range, the blend operation's output and behavior is undefined.
    pub fn blend(self, other: Hsla) -> Hsla {
        let alpha = other.a;

        if alpha >= 1.0 {
            other
        } else if alpha <= 0.0 {
            self
        } else {
            let converted_self = Rgba::from(self);
            let converted_other = Rgba::from(other);
            let blended_rgb = converted_self.blend(converted_other);
            Hsla::from(blended_rgb)
        }
    }

    /// Returns a new HSLA color with the same hue, and lightness, but with no saturation.
    pub fn grayscale(&self) -> Self {
        Hsla {
            h: self.h,
            s: 0.,
            l: self.l,
            a: self.a,
        }
    }

    /// Fade out the color by a given factor. This factor should be between 0.0 and 1.0.
    /// Where 0.0 will leave the color unchanged, and 1.0 will completely fade out the color.
    pub fn fade_out(&mut self, factor: f32) {
        self.a *= 1.0 - factor.clamp(0., 1.);
    }

    /// Multiplies the alpha value of the color by a given factor
    /// and returns a new HSLA color.
    ///
    /// Useful for transforming colors with dynamic opacity,
    /// like a color from an external source.
    ///
    /// Example:
    /// ```
    /// let color = gpui::red();
    /// let faded_color = color.opacity(0.5);
    /// assert_eq!(faded_color.a, 0.5);
    /// ```
    ///
    /// This will return a red color with half the opacity.
    ///
    /// Example:
    /// ```
    /// use gpui::hsla;
    /// let color = hsla(0.7, 1.0, 0.5, 0.7); // A saturated blue
    /// let faded_color = color.opacity(0.16);
    /// assert!((faded_color.a - 0.112).abs() < 1e-6);
    /// ```
    ///
    /// This will return a blue color with around ~10% opacity,
    /// suitable for an element's hover or selected state.
    ///
    pub fn opacity(&self, factor: f32) -> Self {
        Hsla {
            h: self.h,
            s: self.s,
            l: self.l,
            a: self.a * factor.clamp(0., 1.),
        }
    }

    /// Returns a new HSLA color with the same hue, saturation,
    /// and lightness, but with a new alpha value.
    ///
    /// Example:
    /// ```
    /// let color = gpui::red();
    /// let red_color = color.alpha(0.25);
    /// assert_eq!(red_color.a, 0.25);
    /// ```
    ///
    /// This will return a red color with half the opacity.
    ///
    /// Example:
    /// ```
    /// use gpui::hsla;
    /// let color = hsla(0.7, 1.0, 0.5, 0.7); // A saturated blue
    /// let faded_color = color.alpha(0.25);
    /// assert_eq!(faded_color.a, 0.25);
    /// ```
    ///
    /// This will return a blue color with 25% opacity.
    pub fn alpha(&self, a: f32) -> Self {
        Hsla {
            h: self.h,
            s: self.s,
            l: self.l,
            a: a.clamp(0., 1.),
        }
    }
}

impl From<Rgba> for Hsla {
    fn from(color: Rgba) -> Self {
        let r = color.r;
        let g = color.g;
        let b = color.b;

        let max = r.max(g.max(b));
        let min = r.min(g.min(b));
        let delta = max - min;

        let l = (max + min) / 2.0;
        let s = if l == 0.0 || l == 1.0 {
            0.0
        } else if l < 0.5 {
            delta / (2.0 * l)
        } else {
            delta / (2.0 - 2.0 * l)
        };

        let h = if delta == 0.0 {
            0.0
        } else if max == r {
            ((g - b) / delta).rem_euclid(6.0) / 6.0
        } else if max == g {
            ((b - r) / delta + 2.0) / 6.0
        } else {
            ((r - g) / delta + 4.0) / 6.0
        };

        Hsla {
            h,
            s,
            l,
            a: color.a,
        }
    }
}

impl JsonSchema for Hsla {
    fn schema_name() -> Cow<'static, str> {
        Rgba::schema_name()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        Rgba::json_schema(generator)
    }
}

impl Serialize for Hsla {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Rgba::from(*self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Hsla {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Rgba::deserialize(deserializer)?.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[repr(C)]
pub(crate) enum BackgroundTag {
    Solid = 0,
    LinearGradient = 1,
    PatternSlash = 2,
    LiquidGlass = 3,
    ParticleWave = 4,
}

/// A color space for color interpolation.
///
/// References:
/// - <https://developer.mozilla.org/en-US/docs/Web/CSS/color-interpolation-method>
/// - <https://www.w3.org/TR/css-color-4/#typedef-color-space>
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[repr(C)]
pub enum ColorSpace {
    #[default]
    /// The sRGB color space.
    Srgb = 0,
    /// The Oklab color space.
    Oklab = 1,
}

impl Display for ColorSpace {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ColorSpace::Srgb => write!(f, "sRGB"),
            ColorSpace::Oklab => write!(f, "Oklab"),
        }
    }
}

/// A background color, which can be either a solid color or a linear gradient.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[repr(C)]
pub struct Background {
    pub(crate) tag: BackgroundTag,
    pub(crate) color_space: ColorSpace,
    pub(crate) solid: Hsla,
    pub(crate) gradient_angle_or_pattern_height: f32,
    pub(crate) colors: [LinearColorStop; 2],
    /// Padding for alignment for repr(C) layout.
    pad: u32,
}

impl std::fmt::Debug for Background {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.tag {
            BackgroundTag::Solid => write!(f, "Solid({:?})", self.solid),
            BackgroundTag::LinearGradient => {
                write!(
                    f,
                    "LinearGradient({}, {:?}, {:?})",
                    self.gradient_angle_or_pattern_height, self.colors[0], self.colors[1]
                )
            }
            BackgroundTag::PatternSlash => {
                write!(
                    f,
                    "PatternSlash({:?}, {})",
                    self.solid, self.gradient_angle_or_pattern_height
                )
            }
            BackgroundTag::LiquidGlass => {
                write!(f, "LiquidGlass({:?})", self.solid)
            }
            BackgroundTag::ParticleWave => {
                write!(f, "ParticleWave({:?})", self.solid)
            }
        }
    }
}

impl Eq for Background {}
impl Default for Background {
    fn default() -> Self {
        Self {
            tag: BackgroundTag::Solid,
            solid: Hsla::default(),
            color_space: ColorSpace::default(),
            gradient_angle_or_pattern_height: 0.0,
            colors: [LinearColorStop::default(), LinearColorStop::default()],
            pad: 0,
        }
    }
}

/// Creates a hash pattern background
pub fn pattern_slash(color: Hsla, width: f32, interval: f32) -> Background {
    let width_scaled = (width * 255.0) as u32;
    let interval_scaled = (interval * 255.0) as u32;
    let height = ((width_scaled * 0xFFFF) + interval_scaled) as f32;

    Background {
        tag: BackgroundTag::PatternSlash,
        solid: color,
        gradient_angle_or_pattern_height: height,
        ..Default::default()
    }
}

/// Creates a solid background color.
pub fn solid_background(color: impl Into<Hsla>) -> Background {
    Background {
        solid: color.into(),
        ..Default::default()
    }
}

/// Creates a Liquid Glass background with Voxel World Agents (overworld-agents) parameters.
pub fn liquid_glass(
    tint: impl Into<Hsla>,
    disp_scale: f32,
    refract_reach: f32,
    chroma: f32,
    highlight_alpha: f32,
    rim_width: f32,
    blur_px: f32,
) -> Background {
    liquid_glass_styled(
        tint,
        disp_scale,
        refract_reach,
        chroma,
        highlight_alpha,
        rim_width,
        blur_px,
        0,
        0.0,
    )
}

/// Creates a Liquid Glass background with an edge rendering style:
/// - `edge_mode`: 0 = legacy, 1 = Plan A (optical fresnel rim), 2 = Plan B (top-fade hairline), 3 = Plan C (borderless)
/// - `edge_tint_kind`: 0.0 = neutral glass white, 0.5 = accent blue, 1.0 = gate amber
pub fn liquid_glass_styled(
    tint: impl Into<Hsla>,
    disp_scale: f32,
    refract_reach: f32,
    chroma: f32,
    highlight_alpha: f32,
    rim_width: f32,
    blur_px: f32,
    edge_mode: u8,
    edge_tint_kind: f32,
) -> Background {
    Background {
        tag: BackgroundTag::LiquidGlass,
        solid: tint.into(),
        gradient_angle_or_pattern_height: disp_scale,
        colors: [
            LinearColorStop {
                color: Hsla {
                    h: 0.0,
                    s: chroma.clamp(0.0, 1.0),
                    l: 1.0,
                    a: highlight_alpha,
                },
                percentage: refract_reach,
            },
            LinearColorStop {
                color: Hsla {
                    h: (edge_mode as f32) * 0.25,
                    s: edge_tint_kind.clamp(0.0, 1.0),
                    l: 1.0,
                    a: rim_width,
                },
                percentage: blur_px,
            },
        ],
        ..Default::default()
    }
}

/// Tuning parameters for the mirror-ball particle wave backdrop and optional demo image layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleWaveTuning {
    /// Whether the backdrop image dissolve layer is enabled (false skips all image GPU work).
    pub image_enabled: bool,
    /// Whether live Apple Music artwork takes priority over custom backdrop images while playing.
    pub apple_music_artwork_enabled: bool,
    /// Whether live YouTube video mirroring is enabled when Apple Music artwork is not active.
    pub youtube_mirror_enabled: bool,
    /// Mirror-ball tile size in device pixels (clamped to 2.0..=24.0).
    pub tile_size_px: f32,
    /// Duration in seconds that each backdrop image stays intact before disintegrating (0.5..=600.0).
    pub image_hold_secs: f32,
    /// Image reflection brightness multiplier (0.0..=2.0).
    pub image_brightness: f32,
    /// Soft-knee peak luminance ceiling for the image layer (0.0..=1.0).
    pub image_peak_luminance: f32,
    /// Image saturation multiplier (0.0..=1.5).
    pub image_saturation: f32,
}

impl Default for ParticleWaveTuning {
    fn default() -> Self {
        Self {
            image_enabled: false,
            apple_music_artwork_enabled: true,
            youtube_mirror_enabled: true,
            tile_size_px: 6.0,
            image_hold_secs: 10.0,
            image_brightness: 0.40,
            image_peak_luminance: 0.55,
            image_saturation: 0.64,
        }
    }
}

fn finite_clamp(val: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if val.is_finite() {
        val.clamp(min, max)
    } else {
        fallback
    }
}

impl ParticleWaveTuning {
    /// Returns a copy with all fields clamped to safe rendering bounds.
    pub fn clamped(self) -> Self {
        let def = Self::default();
        Self {
            image_enabled: self.image_enabled,
            apple_music_artwork_enabled: self.apple_music_artwork_enabled,
            youtube_mirror_enabled: self.youtube_mirror_enabled,
            tile_size_px: finite_clamp(self.tile_size_px, 2.0, 24.0, def.tile_size_px),
            image_hold_secs: finite_clamp(self.image_hold_secs, 0.5, 600.0, def.image_hold_secs),
            image_brightness: finite_clamp(self.image_brightness, 0.0, 2.0, def.image_brightness),
            image_peak_luminance: finite_clamp(
                self.image_peak_luminance,
                0.0,
                1.0,
                def.image_peak_luminance,
            ),
            image_saturation: finite_clamp(self.image_saturation, 0.0, 1.5, def.image_saturation),
        }
    }
}

fn particle_wave_tuning_cell() -> &'static std::sync::Mutex<ParticleWaveTuning> {
    static CELL: std::sync::OnceLock<std::sync::Mutex<ParticleWaveTuning>> =
        std::sync::OnceLock::new();
    CELL.get_or_init(|| std::sync::Mutex::new(ParticleWaveTuning::default()))
}

/// Updates the shared particle wave tuning read by the GPU renderer on each frame.
pub fn set_particle_wave_tuning(tuning: ParticleWaveTuning) {
    if let Ok(mut guard) = particle_wave_tuning_cell().lock() {
        *guard = tuning.clamped();
    }
}

/// Returns the current particle wave tuning.
pub fn particle_wave_tuning() -> ParticleWaveTuning {
    particle_wave_tuning_cell()
        .lock()
        .map(|g| *g)
        .unwrap_or_default()
}

struct OverrideImageState {
    generation: u64,
    path: Option<String>,
}

static OVERRIDE_IMAGE_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn particle_wave_override_cell() -> &'static std::sync::Mutex<OverrideImageState> {
    static CELL: std::sync::OnceLock<std::sync::Mutex<OverrideImageState>> =
        std::sync::OnceLock::new();
    CELL.get_or_init(|| {
        std::sync::Mutex::new(OverrideImageState {
            generation: 0,
            path: None,
        })
    })
}

/// Sets or clears a live priority override backdrop image (e.g. currently playing Apple Music artwork).
pub fn set_particle_wave_override_image(path: Option<String>) {
    let normalized = path.and_then(|p| {
        let trimmed = p.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    if let Ok(mut guard) = particle_wave_override_cell().lock() {
        if guard.path == normalized {
            return;
        }
        guard.generation = guard.generation.wrapping_add(1).max(1);
        guard.path = normalized;
        OVERRIDE_IMAGE_GEN.store(guard.generation, std::sync::atomic::Ordering::Release);
    }
}

/// Returns the lock-free generation counter for the live override backdrop image.
pub fn particle_wave_override_image_gen() -> u64 {
    OVERRIDE_IMAGE_GEN.load(std::sync::atomic::Ordering::Acquire)
}

/// Returns the current `(generation, Option<path>)` for the live override backdrop image.
pub fn particle_wave_override_image() -> (u64, Option<String>) {
    particle_wave_override_cell()
        .lock()
        .map(|g| (g.generation, g.path.clone()))
        .unwrap_or((0, None))
}

/// Returns whether a live priority override backdrop image (e.g. Apple Music artwork) is currently set.
pub fn particle_wave_override_image_active() -> bool {
    particle_wave_override_cell()
        .lock()
        .map(|g| g.path.is_some())
        .unwrap_or(false)
}

/// A live RGBA8 video frame streamed in-memory for the mirror-ball backdrop.
#[derive(Clone, Debug)]
pub struct ParticleWaveLiveVideoFrame {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Tightly packed RGBA8 pixel data (`width * height * 4` bytes).
    pub rgba: std::sync::Arc<Vec<u8>>,
    /// Timestamp when this frame was received.
    pub updated_at: std::time::Instant,
}

struct LiveVideoState {
    generation: u64,
    frame: Option<ParticleWaveLiveVideoFrame>,
}

static LIVE_VIDEO_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn particle_wave_live_video_cell() -> &'static std::sync::Mutex<LiveVideoState> {
    static CELL: std::sync::OnceLock<std::sync::Mutex<LiveVideoState>> =
        std::sync::OnceLock::new();
    CELL.get_or_init(|| {
        std::sync::Mutex::new(LiveVideoState {
            generation: 0,
            frame: None,
        })
    })
}

/// Updates the shared in-memory live video frame for the mirror-ball backdrop.
pub fn set_particle_wave_live_video_frame(width: u32, height: u32, rgba: Vec<u8>) {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4));
    if width == 0 || height == 0 || expected != Some(rgba.len()) {
        return;
    }
    if let Ok(mut guard) = particle_wave_live_video_cell().lock() {
        guard.generation = guard.generation.wrapping_add(1).max(1);
        guard.frame = Some(ParticleWaveLiveVideoFrame {
            width,
            height,
            rgba: std::sync::Arc::new(rgba),
            updated_at: std::time::Instant::now(),
        });
        LIVE_VIDEO_GEN.store(guard.generation, std::sync::atomic::Ordering::Release);
    }
}

/// Decodes an encoded image payload (JPEG/WebP/PNG) and updates the shared live video frame.
pub fn set_particle_wave_live_video_encoded(
    bytes: &[u8],
    min_dim: u32,
    max_width: u32,
    max_height: u32,
) -> Result<(u32, u32), String> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| format!("failed to decode live video frame: {e}"))?;
    let w = img.width();
    let h = img.height();
    if !(min_dim..=max_width).contains(&w) || !(min_dim..=max_height).contains(&h) {
        return Err(format!(
            "decoded frame {w}x{h} out of allowed bounds {min_dim}..={max_width}x{min_dim}..={max_height}"
        ));
    }
    let rgba = img.to_rgba8().into_raw();
    set_particle_wave_live_video_frame(w, h, rgba);
    Ok((w, h))
}

/// Clears any active live video backdrop frame immediately.
pub fn clear_particle_wave_live_video_frame() {
    if let Ok(mut guard) = particle_wave_live_video_cell().lock() {
        if guard.frame.is_none() {
            return;
        }
        guard.generation = guard.generation.wrapping_add(1).max(1);
        guard.frame = None;
        LIVE_VIDEO_GEN.store(guard.generation, std::sync::atomic::Ordering::Release);
    }
}

/// Returns the lock-free generation counter for the live video backdrop frame.
pub fn particle_wave_live_video_gen() -> u64 {
    LIVE_VIDEO_GEN.load(std::sync::atomic::Ordering::Acquire)
}

/// Returns the current `(generation, Option<ParticleWaveLiveVideoFrame>)` for live video mirroring.
pub fn particle_wave_live_video_frame() -> (u64, Option<ParticleWaveLiveVideoFrame>) {
    particle_wave_live_video_cell()
        .lock()
        .map(|g| (g.generation, g.frame.clone()))
        .unwrap_or((0, None))
}

static PARTICLE_WAVE_GRID_DIMS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Records the active particle-wave tile grid dimensions `(grid_w, grid_h)` from the renderer.
pub fn set_particle_wave_grid_dimensions(grid_w: u32, grid_h: u32) {
    let packed = ((grid_w as u64) << 32) | (grid_h as u64);
    if PARTICLE_WAVE_GRID_DIMS.load(std::sync::atomic::Ordering::Relaxed) != packed {
        PARTICLE_WAVE_GRID_DIMS.store(packed, std::sync::atomic::Ordering::Release);
    }
}

/// Returns the latest rendered particle-wave tile grid dimensions `(grid_w, grid_h)`.
pub fn particle_wave_grid_dimensions() -> (u32, u32) {
    let packed = PARTICLE_WAVE_GRID_DIMS.load(std::sync::atomic::Ordering::Acquire);
    ((packed >> 32) as u32, (packed & 0xFFFF_FFFF) as u32)
}

/// Creates a drifting, gently waving fine particle field background rendered on GPU.
pub fn particle_wave(base_color: impl Into<Hsla>) -> Background {
    Background {
        tag: BackgroundTag::ParticleWave,
        solid: base_color.into(),
        gradient_angle_or_pattern_height: 1.0,
        ..Default::default()
    }
}

/// Creates a mirror-ball wave background with custom wave tint colors and brightness multiplier.
pub fn particle_wave_with(
    base_color: impl Into<Hsla>,
    wave_primary: impl Into<Hsla>,
    wave_secondary: impl Into<Hsla>,
    brightness: f32,
) -> Background {
    let mut c0 = wave_primary.into();
    c0.a = 1.0;
    let mut c1 = wave_secondary.into();
    c1.a = 1.0;
    Background {
        tag: BackgroundTag::ParticleWave,
        solid: base_color.into(),
        gradient_angle_or_pattern_height: brightness.clamp(0.0, 2.5),
        colors: [
            LinearColorStop {
                color: c0,
                percentage: 0.0,
            },
            LinearColorStop {
                color: c1,
                percentage: 1.0,
            },
        ],
        ..Default::default()
    }
}

/// Creates a LinearGradient background color.
///
/// The gradient line's angle of direction. A value of `0.` is equivalent to top; increasing values rotate clockwise from there.
///
/// The `angle` is in degrees value in the range 0.0 to 360.0.
///
/// <https://developer.mozilla.org/en-US/docs/Web/CSS/gradient/linear-gradient>
pub fn linear_gradient(
    angle: f32,
    from: impl Into<LinearColorStop>,
    to: impl Into<LinearColorStop>,
) -> Background {
    Background {
        tag: BackgroundTag::LinearGradient,
        gradient_angle_or_pattern_height: angle,
        colors: [from.into(), to.into()],
        ..Default::default()
    }
}

/// A color stop in a linear gradient.
///
/// <https://developer.mozilla.org/en-US/docs/Web/CSS/gradient/linear-gradient#linear-color-stop>
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[repr(C)]
pub struct LinearColorStop {
    /// The color of the color stop.
    pub color: Hsla,
    /// The percentage of the gradient, in the range 0.0 to 1.0.
    pub percentage: f32,
}

/// Creates a new linear color stop.
///
/// The percentage of the gradient, in the range 0.0 to 1.0.
pub fn linear_color_stop(color: impl Into<Hsla>, percentage: f32) -> LinearColorStop {
    LinearColorStop {
        color: color.into(),
        percentage,
    }
}

impl LinearColorStop {
    /// Returns a new color stop with the same color, but with a modified alpha value.
    pub fn opacity(&self, factor: f32) -> Self {
        Self {
            percentage: self.percentage,
            color: self.color.opacity(factor),
        }
    }
}

impl Background {
    /// Use specified color space for color interpolation.
    ///
    /// <https://developer.mozilla.org/en-US/docs/Web/CSS/color-interpolation-method>
    pub fn color_space(mut self, color_space: ColorSpace) -> Self {
        self.color_space = color_space;
        self
    }

    /// Returns a new background color with the same hue, saturation, and lightness, but with a modified alpha value.
    pub fn opacity(&self, factor: f32) -> Self {
        let mut background = *self;
        background.solid = background.solid.opacity(factor);
        background.colors = [
            self.colors[0].opacity(factor),
            self.colors[1].opacity(factor),
        ];
        background
    }

    /// Returns whether the background color is transparent.
    pub fn is_transparent(&self) -> bool {
        match self.tag {
            BackgroundTag::Solid => self.solid.is_transparent(),
            BackgroundTag::LinearGradient => self.colors.iter().all(|c| c.color.is_transparent()),
            BackgroundTag::PatternSlash => self.solid.is_transparent(),
            BackgroundTag::LiquidGlass => false,
            BackgroundTag::ParticleWave => false,
        }
    }
}

impl From<Hsla> for Background {
    fn from(value: Hsla) -> Self {
        Background {
            tag: BackgroundTag::Solid,
            solid: value,
            ..Default::default()
        }
    }
}
impl From<Rgba> for Background {
    fn from(value: Rgba) -> Self {
        Background {
            tag: BackgroundTag::Solid,
            solid: Hsla::from(value),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_deserialize_three_value_hex_to_rgba() {
        let actual: Rgba = serde_json::from_value(json!("#f09")).unwrap();

        assert_eq!(actual, rgba(0xff0099ff))
    }

    #[test]
    fn test_deserialize_four_value_hex_to_rgba() {
        let actual: Rgba = serde_json::from_value(json!("#f09f")).unwrap();

        assert_eq!(actual, rgba(0xff0099ff))
    }

    #[test]
    fn test_deserialize_six_value_hex_to_rgba() {
        let actual: Rgba = serde_json::from_value(json!("#ff0099")).unwrap();

        assert_eq!(actual, rgba(0xff0099ff))
    }

    #[test]
    fn test_deserialize_eight_value_hex_to_rgba() {
        let actual: Rgba = serde_json::from_value(json!("#ff0099ff")).unwrap();

        assert_eq!(actual, rgba(0xff0099ff))
    }

    #[test]
    fn test_deserialize_eight_value_hex_with_padding_to_rgba() {
        let actual: Rgba = serde_json::from_value(json!(" #f5f5f5ff   ")).unwrap();

        assert_eq!(actual, rgba(0xf5f5f5ff))
    }

    #[test]
    fn test_deserialize_eight_value_hex_with_mixed_case_to_rgba() {
        let actual: Rgba = serde_json::from_value(json!("#DeAdbEeF")).unwrap();

        assert_eq!(actual, rgba(0xdeadbeef))
    }

    #[test]
    fn test_background_solid() {
        let color = Hsla::from(rgba(0xff0099ff));
        let mut background = Background::from(color);
        assert_eq!(background.tag, BackgroundTag::Solid);
        assert_eq!(background.solid, color);

        assert_eq!(background.opacity(0.5).solid, color.opacity(0.5));
        assert!(!background.is_transparent());
        background.solid = hsla(0.0, 0.0, 0.0, 0.0);
        assert!(background.is_transparent());
    }

    #[test]
    fn test_background_linear_gradient() {
        let from = linear_color_stop(rgba(0xff0099ff), 0.0);
        let to = linear_color_stop(rgba(0x00ff99ff), 1.0);
        let background = linear_gradient(90.0, from, to);
        assert_eq!(background.tag, BackgroundTag::LinearGradient);
        assert_eq!(background.colors[0], from);
        assert_eq!(background.colors[1], to);

        assert_eq!(background.opacity(0.5).colors[0], from.opacity(0.5));
        assert_eq!(background.opacity(0.5).colors[1], to.opacity(0.5));
        assert!(!background.is_transparent());
        assert!(background.opacity(0.0).is_transparent());
    }
}
