#![expect(dead_code, reason = "TODO: crate is wip")]

use bytemuck as _;
use derive_setters as _;
#[cfg(feature = "text")]
use glyphon as _;
use wgpu as _;
#[cfg(feature = "winit")]
use winit as _;

pub mod color;
pub mod geometry;
pub mod layer;
pub mod primitive;
pub mod render;

pub use layer::Layer;
pub use render::{
    PreparedFrame, Primitive, PrimitiveRenderer, RenderPipelineCache, Renderer, RendererConfig,
    VertexWriter, Viewport,
};
