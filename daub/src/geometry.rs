#[cfg_attr(feature = "ocaml", derive(ocaml::FromValue, ocaml::ToValue))]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum LayoutValue {
    Relative(f64),
    LogicalPixels(f64),
    PhysicalPixels(f64),
}

impl LayoutValue {
    pub const ZERO: Self = Self::LogicalPixels(0.0);

    #[must_use]
    pub const fn relative(value: f64) -> Self {
        Self::Relative(value)
    }

    #[must_use]
    pub const fn pixels(value: f64) -> Self {
        Self::LogicalPixels(value)
    }

    #[must_use]
    pub const fn physical_pixels(value: f64) -> Self {
        Self::PhysicalPixels(value)
    }
}

impl Default for LayoutValue {
    fn default() -> Self {
        Self::ZERO
    }
}

impl From<LayoutValue> for f64 {
    fn from(value: LayoutValue) -> Self {
        match value {
            LayoutValue::Relative(v)
            | LayoutValue::LogicalPixels(v)
            | LayoutValue::PhysicalPixels(v) => v,
        }
    }
}

#[cfg_attr(feature = "ocaml", derive(ocaml::FromValue, ocaml::ToValue))]
#[derive(Debug, Default, Clone, Copy, PartialEq, PartialOrd)]
pub struct Point {
    pub x: LayoutValue,
    pub y: LayoutValue,
}

impl Point {
    #[must_use]
    pub const fn new(x: LayoutValue, y: LayoutValue) -> Self {
        Self { x, y }
    }
}

#[cfg_attr(feature = "ocaml", derive(ocaml::FromValue, ocaml::ToValue))]
#[derive(Debug, Default, Clone, Copy, PartialEq, PartialOrd)]
pub struct Size {
    pub width: LayoutValue,
    pub height: LayoutValue,
}

impl Size {
    #[must_use]
    pub const fn new(width: LayoutValue, height: LayoutValue) -> Self {
        Self { width, height }
    }
}

/// The point within a rectangle represented by its position.
///
/// `(0, 0)` is the top-left corner and `(1, 1)` is the bottom-right corner.
#[cfg_attr(feature = "ocaml", derive(ocaml::FromValue, ocaml::ToValue))]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Anchor {
    pub x: f64,
    pub y: f64,
}

impl Anchor {
    pub const TOP_LEFT: Self = Self::new(0.0, 0.0);
    pub const TOP: Self = Self::new(0.5, 0.0);
    pub const TOP_RIGHT: Self = Self::new(1.0, 0.0);
    pub const LEFT: Self = Self::new(0.0, 0.5);
    pub const CENTER: Self = Self::new(0.5, 0.5);
    pub const RIGHT: Self = Self::new(1.0, 0.5);
    pub const BOTTOM_LEFT: Self = Self::new(0.0, 1.0);
    pub const BOTTOM: Self = Self::new(0.5, 1.0);
    pub const BOTTOM_RIGHT: Self = Self::new(1.0, 1.0);

    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

impl Default for Anchor {
    fn default() -> Self {
        Self::TOP_LEFT
    }
}

/// An axis-aligned rectangle described by an anchored position and a size.
#[cfg_attr(feature = "ocaml", derive(ocaml::FromValue, ocaml::ToValue))]
#[derive(Debug, Default, Clone, Copy, PartialEq, PartialOrd)]
pub struct Rectangle {
    pub position: Point,
    pub size: Size,
    pub anchor: Anchor,
}

impl Rectangle {
    #[must_use]
    pub const fn new(position: Point, size: Size) -> Self {
        Self::from_anchor(position, size, Anchor::TOP_LEFT)
    }

    #[must_use]
    pub const fn from_center(center: Point, size: Size) -> Self {
        Self::from_anchor(center, size, Anchor::CENTER)
    }

    #[must_use]
    pub const fn from_anchor(position: Point, size: Size, anchor: Anchor) -> Self {
        Self {
            position,
            size,
            anchor,
        }
    }
}
