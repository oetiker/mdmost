//! Truecolor colours, attributes and styles.
//!
//! `mdless` deliberately defines its own [`Style`] type instead of using
//! `ratatui::style::Style`. Renderers must not depend on the TUI crate; the
//! conversion happens once, at the viewport edge.

use crate::error::ThemeError;

/// A 24-bit RGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

impl Color {
    /// Creates a colour from its channels.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Creates a colour from a packed `0xRRGGBB` literal.
    pub const fn hex(value: u32) -> Self {
        Self {
            r: ((value >> 16) & 0xff) as u8,
            g: ((value >> 8) & 0xff) as u8,
            b: (value & 0xff) as u8,
        }
    }

    /// Parses a `#rgb` or `#rrggbb` colour literal (the leading `#` is optional).
    ///
    /// # Errors
    ///
    /// Returns [`ThemeError::InvalidColor`] if the literal is malformed.
    pub fn parse(text: &str) -> Result<Self, ThemeError> {
        let raw = text.strip_prefix('#').unwrap_or(text);
        let invalid = || ThemeError::InvalidColor(text.to_string());
        let nibble = |c: char| c.to_digit(16).map(|d| d as u8).ok_or_else(invalid);
        let chars: Vec<char> = raw.chars().collect();
        match chars.len() {
            3 => Ok(Self::rgb(
                nibble(chars[0])? * 17,
                nibble(chars[1])? * 17,
                nibble(chars[2])? * 17,
            )),
            6 => Ok(Self::rgb(
                nibble(chars[0])? * 16 + nibble(chars[1])?,
                nibble(chars[2])? * 16 + nibble(chars[3])?,
                nibble(chars[4])? * 16 + nibble(chars[5])?,
            )),
            _ => Err(invalid()),
        }
    }

    /// Linearly blends `self` towards `other` by `t` in `0.0..=1.0`.
    ///
    /// Useful for deriving muted or highlighted variants from a palette colour.
    pub fn blend(self, other: Color, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
        Self::rgb(
            mix(self.r, other.r),
            mix(self.g, other.g),
            mix(self.b, other.b),
        )
    }

    /// Perceived relative luminance in `0.0..=1.0` (ITU-R BT.601 weights).
    pub fn luminance(self) -> f32 {
        (0.299 * f32::from(self.r) + 0.587 * f32::from(self.g) + 0.114 * f32::from(self.b)) / 255.0
    }
}

/// A set of terminal text attributes.
///
/// This is a small bit set rather than a `bitflags` dependency; the API is
/// intentionally tiny.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Attributes(u8);

impl Attributes {
    /// No attributes.
    pub const NONE: Self = Self(0);
    /// Bold / increased intensity.
    pub const BOLD: Self = Self(1 << 0);
    /// Dim / decreased intensity.
    pub const DIM: Self = Self(1 << 1);
    /// Italic.
    pub const ITALIC: Self = Self(1 << 2);
    /// Underline.
    pub const UNDERLINE: Self = Self(1 << 3);
    /// Strikethrough.
    pub const STRIKETHROUGH: Self = Self(1 << 4);
    /// Reverse video (swap foreground and background).
    pub const REVERSE: Self = Self(1 << 5);

    /// Returns `true` if no attribute is set.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns `true` if every attribute in `other` is set in `self`.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Returns the union of both attribute sets.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Returns `self` without the attributes in `other`.
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Returns the raw bit set. Only useful for adapters.
    pub const fn bits(self) -> u8 {
        self.0
    }
}

impl std::ops::BitOr for Attributes {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

/// A terminal cell style: optional truecolor foreground and background plus attributes.
///
/// `None` for a colour means "inherit whatever is underneath" — that is what makes
/// [`Style::patch`] useful for overlays such as search highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Style {
    /// Foreground colour, or `None` to inherit.
    pub fg: Option<Color>,
    /// Background colour, or `None` to inherit.
    pub bg: Option<Color>,
    /// Text attributes.
    pub attrs: Attributes,
}

impl Style {
    /// A style that changes nothing.
    pub const NONE: Self = Self {
        fg: None,
        bg: None,
        attrs: Attributes::NONE,
    };

    /// Creates an empty style. Equivalent to [`Style::NONE`].
    pub const fn new() -> Self {
        Self::NONE
    }

    /// Returns `self` with the given foreground colour.
    pub const fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    /// Returns `self` with the given background colour.
    pub const fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    /// Returns `self` with the given attributes added.
    pub const fn with(mut self, attrs: Attributes) -> Self {
        self.attrs = self.attrs.union(attrs);
        self
    }

    /// Returns `self` with the given attributes removed.
    pub const fn without(mut self, attrs: Attributes) -> Self {
        self.attrs = self.attrs.difference(attrs);
        self
    }

    /// Returns `self` marked bold.
    pub const fn bold(self) -> Self {
        self.with(Attributes::BOLD)
    }

    /// Returns `self` marked dim.
    pub const fn dim(self) -> Self {
        self.with(Attributes::DIM)
    }

    /// Returns `self` marked italic.
    pub const fn italic(self) -> Self {
        self.with(Attributes::ITALIC)
    }

    /// Returns `self` marked underlined.
    pub const fn underline(self) -> Self {
        self.with(Attributes::UNDERLINE)
    }

    /// Returns `self` marked struck through.
    pub const fn strikethrough(self) -> Self {
        self.with(Attributes::STRIKETHROUGH)
    }

    /// Returns `self` in reverse video.
    pub const fn reverse(self) -> Self {
        self.with(Attributes::REVERSE)
    }

    /// Overlays `over` on top of `self`.
    ///
    /// Colours set in `over` win; colours left as `None` keep the value from `self`.
    /// Attributes are unioned. This is the operation used to apply an overlay such as
    /// a search highlight to already-styled cells.
    pub const fn patch(self, over: Style) -> Self {
        Self {
            fg: match over.fg {
                Some(c) => Some(c),
                None => self.fg,
            },
            bg: match over.bg {
                Some(c) => Some(c),
                None => self.bg,
            },
            attrs: self.attrs.union(over.attrs),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_short_and_long_hex() {
        assert_eq!(
            Color::parse("#fff").expect("valid"),
            Color::rgb(255, 255, 255)
        );
        assert_eq!(
            Color::parse("012345").expect("valid"),
            Color::rgb(1, 0x23, 0x45)
        );
        assert!(Color::parse("#12").is_err());
        assert!(Color::parse("#gggggg").is_err());
    }

    #[test]
    fn hex_literal_matches_channels() {
        assert_eq!(Color::hex(0x1a2b3c), Color::rgb(0x1a, 0x2b, 0x3c));
    }

    #[test]
    fn blend_endpoints_are_exact() {
        let a = Color::hex(0x000000);
        let b = Color::hex(0xffffff);
        assert_eq!(a.blend(b, 0.0), a);
        assert_eq!(a.blend(b, 1.0), b);
        assert_eq!(a.blend(b, 0.5), Color::rgb(128, 128, 128));
    }

    #[test]
    fn patch_prefers_overlay_colors_and_unions_attributes() {
        let base = Style::new().fg(Color::hex(0x111111)).bold();
        let over = Style::new().bg(Color::hex(0x222222)).italic();
        let merged = base.patch(over);
        assert_eq!(merged.fg, Some(Color::hex(0x111111)));
        assert_eq!(merged.bg, Some(Color::hex(0x222222)));
        assert!(merged.attrs.contains(Attributes::BOLD));
        assert!(merged.attrs.contains(Attributes::ITALIC));
    }

    #[test]
    fn attributes_set_operations() {
        let a = Attributes::BOLD | Attributes::DIM;
        assert!(a.contains(Attributes::BOLD));
        assert!(!a.difference(Attributes::BOLD).contains(Attributes::BOLD));
        assert!(Attributes::NONE.is_empty());
    }
}
