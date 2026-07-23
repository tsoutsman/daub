#![expect(dead_code, reason = "TODO: crate is wip")]

use bytemuck as _;
use derive_setters as _;
use wgpu as _;

pub mod color;
pub mod geometry;
pub mod primitive;
pub mod render;
pub mod scene;
#[cfg(feature = "winit")]
pub mod winit;

pub use render::{
    Error, PreparedFrame, Primitive, PrimitiveRenderer, PrimitiveRendererError,
    RenderPipelineCache, RenderStage, Renderer, RendererConfig, ResolvedRectangle, VertexWriter,
    Viewport,
};
pub use scene::{Layer, Scene};
