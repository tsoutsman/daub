mod renderer;

use derive_setters::Setters;
pub use renderer::TextRenderer;

use crate::{color::Color, geometry::Rectangle, render::Primitive};

/// A font family used to select faces for text shaping.
#[cfg_attr(feature = "ocaml", derive(ocaml::FromValue, ocaml::ToValue))]
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub enum FontFamily {
    Name(String),
    Serif,
    #[default]
    SansSerif,
    Cursive,
    Fantasy,
    Monospace,
}

impl FontFamily {
    fn as_glyphon(&self) -> glyphon::Family<'_> {
        match self {
            Self::Name(name) => glyphon::Family::Name(name),
            Self::Serif => glyphon::Family::Serif,
            Self::SansSerif => glyphon::Family::SansSerif,
            Self::Cursive => glyphon::Family::Cursive,
            Self::Fantasy => glyphon::Family::Fantasy,
            Self::Monospace => glyphon::Family::Monospace,
        }
    }
}

/// The numeric weight of a font face.
#[cfg_attr(feature = "ocaml", derive(ocaml::FromValue, ocaml::ToValue))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontWeight(pub u16);

impl FontWeight {
    pub const THIN: Self = Self(100);
    pub const EXTRA_LIGHT: Self = Self(200);
    pub const LIGHT: Self = Self(300);
    pub const NORMAL: Self = Self(400);
    pub const MEDIUM: Self = Self(500);
    pub const SEMIBOLD: Self = Self(600);
    pub const BOLD: Self = Self(700);
    pub const EXTRA_BOLD: Self = Self(800);
    pub const BLACK: Self = Self(900);

    fn to_glyphon(self) -> glyphon::Weight {
        glyphon::Weight(self.0)
    }
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// The width class of a font face.
#[cfg_attr(feature = "ocaml", derive(ocaml::FromValue, ocaml::ToValue))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontStretch {
    UltraCondensed,
    ExtraCondensed,
    Condensed,
    SemiCondensed,
    #[default]
    Normal,
    SemiExpanded,
    Expanded,
    ExtraExpanded,
    UltraExpanded,
}

impl FontStretch {
    fn to_glyphon(self) -> glyphon::Stretch {
        match self {
            Self::UltraCondensed => glyphon::Stretch::UltraCondensed,
            Self::ExtraCondensed => glyphon::Stretch::ExtraCondensed,
            Self::Condensed => glyphon::Stretch::Condensed,
            Self::SemiCondensed => glyphon::Stretch::SemiCondensed,
            Self::Normal => glyphon::Stretch::Normal,
            Self::SemiExpanded => glyphon::Stretch::SemiExpanded,
            Self::Expanded => glyphon::Stretch::Expanded,
            Self::ExtraExpanded => glyphon::Stretch::ExtraExpanded,
            Self::UltraExpanded => glyphon::Stretch::UltraExpanded,
        }
    }
}

/// The slant style of a font face.
#[cfg_attr(feature = "ocaml", derive(ocaml::FromValue, ocaml::ToValue))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

impl FontStyle {
    fn to_glyphon(self) -> glyphon::Style {
        match self {
            Self::Normal => glyphon::Style::Normal,
            Self::Italic => glyphon::Style::Italic,
            Self::Oblique => glyphon::Style::Oblique,
        }
    }
}

/// The wrapping behavior of a text buffer.
#[cfg_attr(feature = "ocaml", derive(ocaml::FromValue, ocaml::ToValue))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextWrap {
    None,
    Glyph,
    Word,
    #[default]
    WordOrGlyph,
}

impl TextWrap {
    fn to_glyphon(self) -> glyphon::Wrap {
        match self {
            Self::None => glyphon::Wrap::None,
            Self::Glyph => glyphon::Wrap::Glyph,
            Self::Word => glyphon::Wrap::Word,
            Self::WordOrGlyph => glyphon::Wrap::WordOrGlyph,
        }
    }
}

/// The shaping strategy used for a text buffer.
#[cfg_attr(feature = "ocaml", derive(ocaml::FromValue, ocaml::ToValue))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shaping {
    Basic,
    #[default]
    Advanced,
}

impl Shaping {
    fn to_glyphon(self) -> glyphon::Shaping {
        match self {
            Self::Basic => glyphon::Shaping::Basic,
            Self::Advanced => glyphon::Shaping::Advanced,
        }
    }
}

/// A single-style block of shaped text constrained to a rectangle.
#[cfg_attr(feature = "ocaml", derive(ocaml::FromValue, ocaml::ToValue))]
#[derive(Debug, Clone, PartialEq, Setters)]
pub struct Text {
    pub rectangle: Rectangle,
    pub content: String,
    pub color: Color,
    /// Font size in logical pixels.
    pub font_size: f32,
    /// Line height in logical pixels.
    pub line_height: f32,
    pub family: FontFamily,
    pub weight: FontWeight,
    pub stretch: FontStretch,
    pub style: FontStyle,
    pub wrap: TextWrap,
    pub shaping: Shaping,
}

impl Text {
    #[must_use]
    pub fn new(rectangle: Rectangle, content: impl Into<String>, color: Color) -> Self {
        Self {
            rectangle,
            content: content.into(),
            color,
            ..Self::default()
        }
    }
}

impl Default for Text {
    fn default() -> Self {
        Self {
            rectangle: Rectangle::default(),
            content: String::new(),
            color: Color::BLACK,
            font_size: 16.0,
            line_height: 20.0,
            family: FontFamily::SansSerif,
            weight: FontWeight::NORMAL,
            stretch: FontStretch::Normal,
            style: FontStyle::Normal,
            wrap: TextWrap::WordOrGlyph,
            shaping: Shaping::Advanced,
        }
    }
}

impl Primitive for Text {
    type Renderer = TextRenderer;
}

#[cfg(test)]
mod tests {
    use super::{FontFamily, FontWeight, Shaping, Text, TextWrap};
    use crate::{color::Color, geometry::Rectangle};

    #[test]
    fn text_defaults_are_suitable_for_ui_copy() {
        let text = Text::default();

        assert_eq!(text.color, Color::BLACK);
        assert_close(text.font_size, 16.0);
        assert_close(text.line_height, 20.0);
        assert_eq!(text.family, FontFamily::SansSerif);
        assert_eq!(text.weight, FontWeight::NORMAL);
        assert_eq!(text.wrap, TextWrap::WordOrGlyph);
        assert_eq!(text.shaping, Shaping::Advanced);
    }

    #[test]
    fn constructor_and_setters_preserve_content_and_style() {
        let rectangle = Rectangle::default();
        let text = Text::new(rectangle, "Hello", Color::WHITE)
            .font_size(24.0)
            .line_height(30.0);

        assert_eq!(text.rectangle, rectangle);
        assert_eq!(text.content, "Hello");
        assert_eq!(text.color, Color::WHITE);
        assert_close(text.font_size, 24.0);
        assert_close(text.line_height, 30.0);
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < f32::EPSILON);
    }
}
