mod renderer;

use derive_setters::Setters;
pub use renderer::QuadRenderer;

use crate::{
    color::Color,
    geometry::{LayoutValue, Rectangle},
    render::Primitive,
};

/// The inside border of a [`Quad`].
#[cfg_attr(feature = "ocaml", derive(ocaml::FromValue, ocaml::ToValue))]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    pub color: Color,
    pub width: LayoutValue,
}

impl Border {
    pub const NONE: Self = Self {
        color: Color::TRANSPARENT,
        width: LayoutValue::ZERO,
    };

    #[must_use]
    pub const fn new(color: Color, width: LayoutValue) -> Self {
        Self { color, width }
    }
}

impl Default for Border {
    fn default() -> Self {
        Self::NONE
    }
}

/// Corner radii ordered by their named rectangle corners.
#[cfg_attr(feature = "ocaml", derive(ocaml::FromValue, ocaml::ToValue))]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CornerRadii {
    pub top_left: LayoutValue,
    pub top_right: LayoutValue,
    pub bottom_right: LayoutValue,
    pub bottom_left: LayoutValue,
}

impl CornerRadii {
    pub const ZERO: Self = Self::uniform(LayoutValue::ZERO);

    #[must_use]
    pub const fn uniform(radius: LayoutValue) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }

    #[must_use]
    pub const fn new(
        top_left: LayoutValue,
        top_right: LayoutValue,
        bottom_right: LayoutValue,
        bottom_left: LayoutValue,
    ) -> Self {
        Self {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        }
    }
}

impl Default for CornerRadii {
    fn default() -> Self {
        Self::ZERO
    }
}

/// A filled, optionally bordered axis-aligned rectangle.
#[cfg_attr(feature = "ocaml", derive(ocaml::FromValue, ocaml::ToValue))]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Setters)]
pub struct Quad {
    pub rectangle: Rectangle,
    pub color: Color,
    pub border: Border,
    pub corner_radii: CornerRadii,
}

impl Quad {
    #[must_use]
    pub const fn new(rectangle: Rectangle, color: Color) -> Self {
        Self {
            rectangle,
            color,
            border: Border::NONE,
            corner_radii: CornerRadii::ZERO,
        }
    }
}

impl Default for Quad {
    fn default() -> Self {
        Self::new(Rectangle::default(), Color::TRANSPARENT)
    }
}

impl Primitive for Quad {
    type Renderer = QuadRenderer;
}

#[cfg(test)]
mod tests {
    use super::{Border, CornerRadii, Quad};
    use crate::{
        color::Color,
        geometry::{LayoutValue, Rectangle},
    };

    #[test]
    fn new_quad_has_no_border_or_rounding() {
        let rectangle = Rectangle::default();
        let quad = Quad::new(rectangle, Color::WHITE);

        assert_eq!(quad.rectangle, rectangle);
        assert_eq!(quad.color, Color::WHITE);
        assert_eq!(quad.border, Border::NONE);
        assert_eq!(quad.corner_radii, CornerRadii::ZERO);
    }

    #[test]
    fn builder_sets_appearance() {
        let border = Border::new(Color::BLACK, LayoutValue::pixels(2.0));
        let radius = LayoutValue::pixels(6.0);
        let quad = Quad::new(Rectangle::default(), Color::WHITE)
            .border(border)
            .corner_radii(CornerRadii::uniform(radius));

        assert_eq!(quad.border, border);
        assert_eq!(quad.corner_radii, CornerRadii::uniform(radius));
    }
}
