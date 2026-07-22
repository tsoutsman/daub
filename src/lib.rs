#![expect(dead_code, reason = "TODO: crate is wip")]

use bytemuck as _;
use derive_setters as _;
#[cfg(feature = "text")]
use glyphon as _;
use wgpu as _;

pub mod color;
pub mod geometry;
pub mod primitive;
pub mod render;
pub mod scene;
#[cfg(feature = "winit")]
pub mod winit;

pub use render::{
    PreparedFrame, Primitive, PrimitiveRenderer, RenderPipelineCache, Renderer, RendererConfig,
    VertexWriter, Viewport,
};
pub use scene::Scene;
