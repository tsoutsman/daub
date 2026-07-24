mod atlas;
mod renderer;

pub use atlas::{AtlasImage, ImageAtlas, ImageAtlasError};
pub use renderer::ImageRenderer;

use crate::{geometry::Rectangle, render::Primitive};

/// An atlas image stretched into an axis-aligned rectangle.
#[derive(Debug, Clone)]
pub struct Image {
    pub rectangle: Rectangle,
    pub image: AtlasImage,
}

impl Image {
    #[must_use]
    pub fn new(rectangle: Rectangle, image: AtlasImage) -> Self {
        Self { rectangle, image }
    }
}

impl Primitive for Image {
    type Renderer = ImageRenderer;
}
