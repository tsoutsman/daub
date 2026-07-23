use std::{
    any::{Any, TypeId},
    collections::HashMap,
    error, fmt,
    ops::Range,
};

use crate::{
    geometry::{LayoutValue, Rectangle},
    scene::{ErasedBatch, Scene},
};

/// A value that can be submitted to a [`crate::scene::Layer`].
pub trait Primitive: 'static {
    type Renderer: PrimitiveRenderer<Primitive = Self>;
}

/// The error type returned by a concrete primitive renderer.
pub type PrimitiveRendererError = Box<dyn error::Error + Send + Sync + 'static>;

/// A result returned by the rendering API.
pub type Result<T> = ::std::result::Result<T, Error>;

/// The operation a primitive renderer was performing when it failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderStage {
    Prepare,
    Render,
}

/// An error produced while preparing or rendering a primitive batch.
#[derive(Debug)]
pub struct Error {
    stage: RenderStage,
    renderer: &'static str,
    source: PrimitiveRendererError,
}

impl Error {
    fn new<R>(stage: RenderStage, source: PrimitiveRendererError) -> Self
    where
        R: PrimitiveRenderer,
    {
        Self {
            stage,
            renderer: std::any::type_name::<R>(),
            source,
        }
    }

    #[must_use]
    pub const fn stage(&self) -> RenderStage {
        self.stage
    }

    #[must_use]
    pub const fn renderer(&self) -> &'static str {
        self.renderer
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "primitive renderer {} failed during {:?}: {}",
            self.renderer, self.stage, self.source
        )
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

/// Configuration that affects render-pipeline compatibility.
///
/// Create a new [`Renderer`] if either value changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RendererConfig {
    pub target_format: wgpu::TextureFormat,
    pub sample_count: u32,
}

impl RendererConfig {
    #[must_use]
    pub const fn new(target_format: wgpu::TextureFormat) -> Self {
        Self {
            target_format,
            sample_count: 1,
        }
    }
}

/// The dimensions used to resolve relative and pixel-based geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub physical_width: u32,
    pub physical_height: u32,
    pub scale_factor: f64,
}

impl Viewport {
    #[must_use]
    pub const fn new(physical_width: u32, physical_height: u32, scale_factor: f64) -> Self {
        Self {
            physical_width,
            physical_height,
            scale_factor,
        }
    }

    /// Resolves a layout value into physical pixels.
    ///
    /// `relative_to` is the physical-pixel extent represented by a relative
    /// value of `1.0`.
    #[must_use]
    pub fn resolve(self, value: LayoutValue, relative_to: f64) -> f64 {
        match value {
            LayoutValue::Relative(value) => value * relative_to,
            LayoutValue::LogicalPixels(value) => value * self.scale_factor,
            LayoutValue::PhysicalPixels(value) => value,
        }
    }

    /// Resolves a horizontal layout value into physical pixels.
    #[must_use]
    pub fn resolve_x(self, value: LayoutValue) -> f64 {
        self.resolve(value, f64::from(self.physical_width))
    }

    /// Resolves a vertical layout value into physical pixels.
    #[must_use]
    pub fn resolve_y(self, value: LayoutValue) -> f64 {
        self.resolve(value, f64::from(self.physical_height))
    }

    /// Converts a position in physical pixels to normalized device coordinates.
    ///
    /// Pixel positions use a top-left origin with Y increasing downwards. The
    /// returned NDC position uses WGPU's Y-up convention.
    #[must_use]
    pub fn to_ndc_position(self, x: f64, y: f64) -> [f64; 2] {
        let x = ndc_position_axis(x, f64::from(self.physical_width), false);
        let y = ndc_position_axis(y, f64::from(self.physical_height), true);
        [x, y]
    }

    /// Converts a size in physical pixels to a normalized device coordinate
    /// size.
    #[must_use]
    pub fn to_ndc_size(self, width: f64, height: f64) -> [f64; 2] {
        let width = ndc_size_axis(width, f64::from(self.physical_width));
        let height = ndc_size_axis(height, f64::from(self.physical_height));
        [width, height]
    }

    /// Resolves an anchored rectangle into physical-pixel coordinates.
    ///
    /// Returns `None` when the rectangle contains a non-finite position, size,
    /// or anchor component. Negative dimensions are clamped to zero.
    #[must_use]
    pub fn resolve_rectangle(self, rectangle: Rectangle) -> Option<ResolvedRectangle> {
        let width = self.resolve_x(rectangle.size.width);
        let height = self.resolve_y(rectangle.size.height);
        let position_x = self.resolve_x(rectangle.position.x);
        let position_y = self.resolve_y(rectangle.position.y);

        if ![
            width,
            height,
            position_x,
            position_y,
            rectangle.anchor.x,
            rectangle.anchor.y,
        ]
        .into_iter()
        .all(f64::is_finite)
        {
            return None;
        }

        let width = width.max(0.0);
        let height = height.max(0.0);

        Some(ResolvedRectangle {
            left: position_x - rectangle.anchor.x * width,
            top: position_y - rectangle.anchor.y * height,
            width,
            height,
        })
    }
}

/// An axis-aligned rectangle resolved into physical-pixel coordinates.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ResolvedRectangle {
    /// The physical-pixel coordinate of the left edge.
    pub left: f64,
    /// The physical-pixel coordinate of the top edge.
    pub top: f64,
    /// The width in physical pixels.
    pub width: f64,
    /// The height in physical pixels.
    pub height: f64,
}

impl ResolvedRectangle {
    /// Returns the physical-pixel coordinate of the right edge.
    #[must_use]
    pub const fn right(self) -> f64 {
        self.left + self.width
    }

    /// Returns the physical-pixel coordinate of the bottom edge.
    #[must_use]
    pub const fn bottom(self) -> f64 {
        self.top + self.height
    }
}

fn ndc_position_axis(position: f64, viewport_extent: f64, invert: bool) -> f64 {
    if viewport_extent <= 0.0 {
        return 0.0;
    }

    let ndc = position / viewport_extent * 2.0 - 1.0;
    if invert { -ndc } else { ndc }
}

fn ndc_size_axis(size: f64, viewport_extent: f64) -> f64 {
    if viewport_extent <= 0.0 {
        0.0
    } else {
        size / viewport_extent * 2.0
    }
}

/// Append-only access to the shared vertex buffer being assembled.
///
/// The writer is scoped to one batch. Its length is therefore the number of
/// bytes written by that batch, not the offset of the batch in the shared
/// buffer.
#[derive(Debug)]
pub struct VertexWriter<'a> {
    bytes: &'a mut Vec<u8>,
    start: usize,
}

impl<'a> VertexWriter<'a> {
    fn new(bytes: &'a mut Vec<u8>) -> Self {
        let start = bytes.len();
        Self { bytes, start }
    }

    /// Appends plain-old-data values in their native byte representation.
    pub fn write<T>(&mut self, values: &[T])
    where
        T: bytemuck::Pod,
    {
        self.write_bytes(bytemuck::cast_slice(values));
    }

    /// Appends already encoded vertex bytes.
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    /// Returns the number of bytes appended through this writer.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len() - self.start
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Prepares and renders one concrete [`Primitive`] type.
///
/// Each renderer binds the pipelines and resources required by its layer-local
/// type batch in [`Self::render_batch`].
pub trait PrimitiveRenderer: Sized + 'static {
    type Primitive: Primitive<Renderer = Self>;

    /// Creates the state owned by this primitive renderer.
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: &RendererConfig,
        pipeline_cache: &mut RenderPipelineCache,
    ) -> Self;

    /// Starts preparation for a frame.
    ///
    /// This is called once per [`Renderer::prepare`] before any batches are
    /// prepared, including when this renderer has no batches in the frame. It
    /// is a suitable place to clear renderer-owned CPU staging collections.
    /// `viewport` describes the full render target.
    fn start_prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, viewport: Viewport) {
        let _ = (device, queue, viewport);
    }

    /// Prepares one layer-local type batch and appends its data to the shared
    /// vertex buffer through `vertices`.
    ///
    /// `viewport` describes the layer's local coordinate system. Its physical
    /// dimensions differ from the render target when the layer has a viewport.
    ///
    /// Renderers that manage their own GPU buffers can use the default empty
    /// implementation.
    ///
    /// # Errors
    ///
    /// Returns an error when the batch cannot be prepared for rendering.
    fn prepare_batch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport: Viewport,
        primitives: &[Self::Primitive],
        vertices: &mut VertexWriter<'_>,
    ) -> ::std::result::Result<(), PrimitiveRendererError> {
        let _ = (device, queue, viewport, primitives, vertices);
        Ok(())
    }

    /// Finishes preparation for a frame.
    ///
    /// This is called once per [`Renderer::prepare`] after all batches have
    /// been prepared, including when this renderer had no batches in the
    /// frame. It is a suitable place to upload renderer-owned storage or
    /// uniform buffers and finalize bind groups used during rendering.
    fn finish_prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let _ = (device, queue);
    }

    /// Draws one layer-local type batch.
    ///
    /// `vertex_buffer` contains the bytes appended by [`Self::prepare_batch`]
    /// for this batch. It is `None` when preparation appended no bytes.
    ///
    /// The layer's viewport and scissor are already set. Implementations must
    /// not change either state; Daub retains them across draws and layers.
    ///
    /// # Errors
    ///
    /// Returns an error when the prepared batch cannot be recorded.
    fn render_batch(
        &mut self,
        primitives: &[Self::Primitive],
        render_pass: &mut wgpu::RenderPass<'_>,
        vertex_buffer: Option<wgpu::BufferSlice<'_>>,
    ) -> ::std::result::Result<(), PrimitiveRendererError>;
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RenderPipelineCache {
    pipelines: HashMap<TypeId, wgpu::RenderPipeline>,
}

impl RenderPipelineCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_or_create<R>(
        &mut self,
        build: impl FnOnce() -> wgpu::RenderPipeline,
    ) -> wgpu::RenderPipeline
    where
        R: 'static,
    {
        self.pipelines
            .entry(TypeId::of::<R>())
            .or_insert_with(build)
            .clone()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pipelines.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.pipelines.len()
    }

    pub fn clear(&mut self) {
        self.pipelines.clear();
    }
}

/// The crate-level renderer.
///
/// It owns the primitive renderers, their pipelines, and a shared vertex
/// buffer. Call [`Renderer::prepare`] before beginning the render pass, then
/// pass the returned [`PreparedFrame`] to that pass.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: RendererConfig,
    pipeline_cache: RenderPipelineCache,
    primitive_renderers: HashMap<TypeId, Box<dyn ErasedPrimitiveRenderer>>,
    vertex_bytes: Vec<u8>,
    vertex_buffer: Option<wgpu::Buffer>,
    vertex_buffer_capacity: wgpu::BufferAddress,
}

impl Renderer {
    #[must_use]
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, config: RendererConfig) -> Self {
        Self {
            device: device.clone(),
            queue: queue.clone(),
            config,
            pipeline_cache: RenderPipelineCache::new(),
            primitive_renderers: HashMap::new(),
            vertex_bytes: Vec::new(),
            vertex_buffer: None,
            vertex_buffer_capacity: 0,
        }
    }

    #[must_use]
    pub const fn config(&self) -> RendererConfig {
        self.config
    }

    /// Prepares all batches and uploads their data before rendering starts.
    ///
    /// The returned frame borrows this renderer, preventing its shared buffers
    /// from being replaced until the frame has been rendered or dropped.
    ///
    /// # Errors
    ///
    /// Returns an error when a primitive renderer cannot prepare one of its
    /// batches.
    pub fn prepare<'renderer, 'scene>(
        &'renderer mut self,
        viewport: Viewport,
        scene: &'scene Scene,
    ) -> Result<PreparedFrame<'renderer, 'scene>> {
        self.vertex_bytes.clear();
        let mut layers = Vec::new();

        for layer in scene.layers() {
            let Some(render_viewport) = resolve_render_viewport(layer.viewport, viewport) else {
                continue;
            };
            let Some(scissor) = resolve_layer_scissor(layer.clip_bounds, viewport) else {
                continue;
            };
            let local_viewport = Viewport::new(
                render_viewport.width,
                render_viewport.height,
                viewport.scale_factor,
            );
            let mut batches = Vec::new();

            for batch in layer.batches() {
                if batch.is_empty() {
                    continue;
                }

                batches.push(batch);
            }

            if !batches.is_empty() {
                layers.push(PendingLayer {
                    viewport: render_viewport,
                    scissor,
                    local_viewport,
                    batches,
                });
            }
        }

        for layer in &layers {
            for batch in &layer.batches {
                self.ensure_primitive_renderer(*batch);
            }
        }

        for renderer in self.primitive_renderers.values_mut() {
            renderer.start_prepare(&self.device, &self.queue, viewport);
        }

        let mut prepared_layers = Vec::with_capacity(layers.len());
        for layer in layers {
            let mut draws = Vec::with_capacity(layer.batches.len());
            for batch in layer.batches {
                align_vec(&mut self.vertex_bytes);
                let start = self.vertex_bytes.len() as wgpu::BufferAddress;

                let renderer_type_id = batch.renderer_type_id();
                let renderer = self.primitive_renderers.get_mut(&renderer_type_id);
                let Some(renderer) = renderer else {
                    continue;
                };
                let mut vertices = VertexWriter::new(&mut self.vertex_bytes);
                renderer.prepare_batch(
                    &self.device,
                    &self.queue,
                    layer.local_viewport,
                    batch,
                    &mut vertices,
                )?;
                let end = start + vertices.len() as wgpu::BufferAddress;

                draws.push(PreparedDraw {
                    batch,
                    renderer_type_id,
                    vertex_range: (start != end).then_some(start..end),
                });
            }
            prepared_layers.push(PreparedLayer {
                viewport: layer.viewport,
                scissor: layer.scissor,
                draws,
            });
        }

        for renderer in self.primitive_renderers.values_mut() {
            renderer.finish_prepare(&self.device, &self.queue);
        }

        align_vec(&mut self.vertex_bytes);
        self.upload_vertices();

        Ok(PreparedFrame {
            renderer: self,
            layers: prepared_layers,
        })
    }

    fn ensure_primitive_renderer(&mut self, batch: &dyn ErasedBatch) {
        self.primitive_renderers
            .entry(batch.renderer_type_id())
            .or_insert_with(|| {
                batch.create_renderer(
                    &self.device,
                    &self.queue,
                    &self.config,
                    &mut self.pipeline_cache,
                )
            });
    }

    fn upload_vertices(&mut self) {
        if self.vertex_bytes.is_empty() {
            return;
        }

        let required = self.vertex_bytes.len() as wgpu::BufferAddress;
        if required > self.vertex_buffer_capacity {
            let capacity = required.next_power_of_two();
            self.vertex_buffer = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("daub shared vertex buffer"),
                size: capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.vertex_buffer_capacity = capacity;
        }

        if let Some(buffer) = &self.vertex_buffer {
            // TODO: use queue.write_buffer_with
            self.queue.write_buffer(buffer, 0, &self.vertex_bytes);
        }
    }
}

impl fmt::Debug for Renderer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Renderer")
            .field("config", &self.config)
            .field("primitive_renderer_count", &self.primitive_renderers.len())
            .field("pipeline_count", &self.pipeline_cache.len())
            .field("vertex_staging_capacity", &self.vertex_bytes.capacity())
            .field("vertex_buffer_capacity", &self.vertex_buffer_capacity)
            .finish_non_exhaustive()
    }
}

/// A scene whose batches and shared vertex data are ready to render.
///
/// Values are created by [`Renderer::prepare`].
pub struct PreparedFrame<'renderer, 'scene> {
    renderer: &'renderer mut Renderer,
    layers: Vec<PreparedLayer<'scene>>,
}

impl PreparedFrame<'_, '_> {
    /// Records every prepared draw in strict layer order.
    ///
    /// # Errors
    ///
    /// Returns an error when a primitive renderer cannot record its batch.
    pub fn render(&mut self, render_pass: &mut wgpu::RenderPass<'_>) -> Result<()> {
        let Renderer {
            primitive_renderers,
            vertex_buffer,
            ..
        } = &mut *self.renderer;

        let mut current_viewport = None;
        let mut current_scissor = None;
        for layer in &self.layers {
            if current_viewport != Some(layer.viewport) {
                layer.viewport.set(render_pass);
                current_viewport = Some(layer.viewport);
            }
            if current_scissor != Some(layer.scissor) {
                layer.scissor.set(render_pass);
                current_scissor = Some(layer.scissor);
            }

            for draw in &layer.draws {
                let renderer = primitive_renderers.get_mut(&draw.renderer_type_id);
                let Some(renderer) = renderer else {
                    continue;
                };

                let vertex_slice = draw.vertex_range.as_ref().and_then(|range| {
                    vertex_buffer
                        .as_ref()
                        .map(|buffer| buffer.slice(range.clone()))
                });
                renderer.render_batch(draw.batch, render_pass, vertex_slice)?;
            }
        }

        Ok(())
    }

    #[must_use]
    pub fn draw_count(&self) -> usize {
        self.layers.iter().map(|layer| layer.draws.len()).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.layers.iter().all(|layer| layer.draws.is_empty())
    }
}

impl fmt::Debug for PreparedFrame<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedFrame")
            .field("layer_count", &self.layers.len())
            .field("draw_count", &self.draw_count())
            .finish_non_exhaustive()
    }
}

struct PreparedDraw<'scene> {
    batch: &'scene dyn ErasedBatch,
    renderer_type_id: TypeId,
    vertex_range: Option<Range<wgpu::BufferAddress>>,
}

struct PendingLayer<'scene> {
    viewport: RenderViewport,
    scissor: ScissorRect,
    local_viewport: Viewport,
    batches: Vec<&'scene dyn ErasedBatch>,
}

struct PreparedLayer<'scene> {
    viewport: RenderViewport,
    scissor: ScissorRect,
    draws: Vec<PreparedDraw<'scene>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RenderViewport {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl RenderViewport {
    #[expect(
        clippy::cast_precision_loss,
        reason = "wgpu texture dimensions are small enough to be represented exactly as f32"
    )]
    fn set(self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_viewport(
            self.x as f32,
            self.y as f32,
            self.width as f32,
            self.height as f32,
            0.0,
            1.0,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScissorRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl ScissorRect {
    fn set(self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_scissor_rect(self.x, self.y, self.width, self.height);
    }
}

pub(crate) trait ErasedPrimitiveRenderer: Any {
    fn start_prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, viewport: Viewport);

    fn prepare_batch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport: Viewport,
        batch: &dyn ErasedBatch,
        vertices: &mut VertexWriter<'_>,
    ) -> Result<()>;

    fn finish_prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue);

    fn render_batch(
        &mut self,
        batch: &dyn ErasedBatch,
        render_pass: &mut wgpu::RenderPass<'_>,
        vertex_buffer: Option<wgpu::BufferSlice<'_>>,
    ) -> Result<()>;
}

impl<R> ErasedPrimitiveRenderer for R
where
    R: PrimitiveRenderer,
{
    fn start_prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, viewport: Viewport) {
        PrimitiveRenderer::start_prepare(self, device, queue, viewport);
    }

    fn prepare_batch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport: Viewport,
        batch: &dyn ErasedBatch,
        vertices: &mut VertexWriter<'_>,
    ) -> Result<()> {
        let Some(batch) = batch.downcast_ref::<R::Primitive>() else {
            return Ok(());
        };
        PrimitiveRenderer::prepare_batch(
            self,
            device,
            queue,
            viewport,
            batch.primitives(),
            vertices,
        )
        .map_err(|source| Error::new::<R>(RenderStage::Prepare, source))
    }

    fn finish_prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        PrimitiveRenderer::finish_prepare(self, device, queue);
    }

    fn render_batch(
        &mut self,
        batch: &dyn ErasedBatch,
        render_pass: &mut wgpu::RenderPass<'_>,
        vertex_buffer: Option<wgpu::BufferSlice<'_>>,
    ) -> Result<()> {
        let Some(batch) = batch.downcast_ref::<R::Primitive>() else {
            return Ok(());
        };
        PrimitiveRenderer::render_batch(self, batch.primitives(), render_pass, vertex_buffer)
            .map_err(|source| Error::new::<R>(RenderStage::Render, source))
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "wgpu's copy-buffer alignment is four on every supported target"
)]
fn align_vec(bytes: &mut Vec<u8>) {
    let alignment = wgpu::COPY_BUFFER_ALIGNMENT as usize;
    let remainder = bytes.len() % alignment;
    if remainder != 0 {
        bytes.resize(bytes.len() + alignment - remainder, 0);
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "finite coordinates are clamped to the viewport's u32 dimensions before conversion"
)]
fn resolve_scissor(rectangle: Rectangle, viewport: Viewport) -> Option<ScissorRect> {
    let viewport_width = f64::from(viewport.physical_width);
    let viewport_height = f64::from(viewport.physical_height);
    let resolved = viewport.resolve_rectangle(rectangle)?;

    let left = resolved.left.clamp(0.0, viewport_width).floor();
    let top = resolved.top.clamp(0.0, viewport_height).floor();
    let right = resolved.right().clamp(0.0, viewport_width).ceil();
    let bottom = resolved.bottom().clamp(0.0, viewport_height).ceil();

    if right <= left || bottom <= top {
        return None;
    }

    Some(ScissorRect {
        x: left as u32,
        y: top as u32,
        width: (right - left) as u32,
        height: (bottom - top) as u32,
    })
}

fn resolve_layer_scissor(rectangle: Option<Rectangle>, viewport: Viewport) -> Option<ScissorRect> {
    match rectangle {
        Some(rectangle) => resolve_scissor(rectangle, viewport),
        None if viewport.physical_width > 0 && viewport.physical_height > 0 => Some(ScissorRect {
            x: 0,
            y: 0,
            width: viewport.physical_width,
            height: viewport.physical_height,
        }),
        None => None,
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "finite coordinates are clamped to the render target's u32 dimensions before \
              conversion"
)]
fn resolve_render_viewport(
    rectangle: Option<Rectangle>,
    target: Viewport,
) -> Option<RenderViewport> {
    let Some(rectangle) = rectangle else {
        return (target.physical_width > 0 && target.physical_height > 0).then_some(
            RenderViewport {
                x: 0,
                y: 0,
                width: target.physical_width,
                height: target.physical_height,
            },
        );
    };

    let target_width = f64::from(target.physical_width);
    let target_height = f64::from(target.physical_height);
    let resolved = target.resolve_rectangle(rectangle)?;
    let left = resolved.left.clamp(0.0, target_width).floor();
    let top = resolved.top.clamp(0.0, target_height).floor();
    let right = resolved.right().clamp(0.0, target_width).ceil();
    let bottom = resolved.bottom().clamp(0.0, target_height).ceil();

    if right <= left || bottom <= top {
        return None;
    }

    Some(RenderViewport {
        x: left as u32,
        y: top as u32,
        width: (right - left) as u32,
        height: (bottom - top) as u32,
    })
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use super::{
        Primitive, PrimitiveRenderer, PrimitiveRendererError, RenderPipelineCache, RenderViewport,
        Renderer, RendererConfig, ScissorRect, VertexWriter, Viewport, resolve_scissor,
    };
    use crate::{
        color::Color,
        geometry::{LayoutValue, Point, Rectangle, Size},
        primitive::{Quad, QuadRenderer, Text, TextRenderer},
        scene::{Layer, Scene},
    };

    struct ViewportProbe;

    struct ViewportProbeRenderer;

    impl Primitive for ViewportProbe {
        type Renderer = ViewportProbeRenderer;
    }

    impl PrimitiveRenderer for ViewportProbeRenderer {
        type Primitive = ViewportProbe;

        fn new(
            _: &wgpu::Device,
            _: &wgpu::Queue,
            _: &RendererConfig,
            _: &mut RenderPipelineCache,
        ) -> Self {
            Self
        }

        fn prepare_batch(
            &mut self,
            _: &wgpu::Device,
            _: &wgpu::Queue,
            viewport: Viewport,
            _: &[Self::Primitive],
            vertices: &mut VertexWriter<'_>,
        ) -> Result<(), PrimitiveRendererError> {
            vertices.write(&[viewport.physical_width, viewport.physical_height]);
            Ok(())
        }

        fn render_batch(
            &mut self,
            _: &[Self::Primitive],
            _: &mut wgpu::RenderPass<'_>,
            _: Option<wgpu::BufferSlice<'_>>,
        ) -> Result<(), PrimitiveRendererError> {
            Ok(())
        }
    }

    #[test]
    fn viewport_resolves_layout_values_to_physical_pixels() {
        let viewport = Viewport::new(800, 600, 2.0);

        assert_close(
            &[
                viewport.resolve_x(LayoutValue::relative(0.25)),
                viewport.resolve_y(LayoutValue::relative(0.25)),
                viewport.resolve_x(LayoutValue::pixels(12.0)),
                viewport.resolve_y(LayoutValue::physical_pixels(12.0)),
                viewport.resolve(LayoutValue::relative(0.25), 40.0),
            ],
            &[200.0, 150.0, 24.0, 12.0, 10.0],
        );
    }

    #[test]
    fn viewport_converts_physical_geometry_to_ndc() {
        let viewport = Viewport::new(800, 600, 2.0);

        assert_close(&viewport.to_ndc_position(0.0, 0.0), &[-1.0, 1.0]);
        assert_close(&viewport.to_ndc_position(400.0, 300.0), &[0.0, 0.0]);
        assert_close(&viewport.to_ndc_position(800.0, 600.0), &[1.0, -1.0]);
        assert_close(&viewport.to_ndc_size(400.0, 150.0), &[1.0, 0.5]);
    }

    #[test]
    fn zero_sized_viewport_produces_finite_ndc_geometry() {
        let viewport = Viewport::new(0, 0, 1.0);

        assert_close(&viewport.to_ndc_position(10.0, 10.0), &[0.0, 0.0]);
        assert_close(&viewport.to_ndc_size(10.0, 10.0), &[0.0, 0.0]);
    }

    #[test]
    fn resolves_and_clamps_scissor_rectangles() {
        let viewport = Viewport::new(800, 600, 2.0);
        let rectangle = Rectangle::new(
            Point::new(LayoutValue::pixels(-5.0), LayoutValue::pixels(10.0)),
            Size::new(LayoutValue::relative(0.5), LayoutValue::pixels(50.0)),
        );

        assert_eq!(
            resolve_scissor(rectangle, viewport),
            Some(ScissorRect {
                x: 0,
                y: 20,
                width: 390,
                height: 100,
            })
        );
    }

    #[test]
    fn rejects_empty_scissor_rectangles() {
        let viewport = Viewport::new(800, 600, 1.0);
        let rectangle = Rectangle::new(
            Point::default(),
            Size::new(LayoutValue::ZERO, LayoutValue::pixels(20.0)),
        );

        assert_eq!(resolve_scissor(rectangle, viewport), None);
    }

    #[test]
    fn layer_viewport_controls_preparation_and_resets_draw_state() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut renderer = Renderer::new(
            &device,
            &queue,
            RendererConfig::new(wgpu::TextureFormat::Rgba8UnormSrgb),
        );
        let target = Viewport::new(800, 600, 2.0);
        let viewport_bounds = Rectangle::new(
            Point::new(LayoutValue::pixels(50.0), LayoutValue::pixels(25.0)),
            Size::new(LayoutValue::pixels(200.0), LayoutValue::pixels(100.0)),
        );
        let clip_bounds = Rectangle::new(
            Point::new(LayoutValue::pixels(60.0), LayoutValue::pixels(35.0)),
            Size::new(LayoutValue::pixels(180.0), LayoutValue::pixels(80.0)),
        );
        let mut local = Layer::new()
            .viewport(viewport_bounds)
            .clip_bounds(clip_bounds);
        local.push(ViewportProbe);
        let mut global = Layer::new();
        global.push(ViewportProbe);
        let scene = Scene::from([local, global]);

        let Ok(prepared) = renderer.prepare(target, &scene) else {
            std::process::abort();
        };

        assert_eq!(prepared.draw_count(), 2);
        assert_eq!(prepared.layers.len(), 2);
        assert_eq!(
            prepared.layers[0].viewport,
            RenderViewport {
                x: 100,
                y: 50,
                width: 400,
                height: 200,
            }
        );
        assert_eq!(
            prepared.layers[0].scissor,
            ScissorRect {
                x: 120,
                y: 70,
                width: 360,
                height: 160,
            }
        );
        assert_eq!(prepared.layers[0].draws.len(), 1);
        assert_eq!(
            prepared.layers[1].viewport,
            RenderViewport {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            }
        );
        assert_eq!(
            prepared.layers[1].scissor,
            ScissorRect {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            }
        );
        assert_eq!(prepared.layers[1].draws.len(), 1);
        assert_eq!(
            prepared.renderer.vertex_bytes,
            [
                400_u32.to_ne_bytes(),
                200_u32.to_ne_bytes(),
                800_u32.to_ne_bytes(),
                600_u32.to_ne_bytes(),
            ]
            .concat()
        );
    }

    #[test]
    fn vertex_writer_only_reports_bytes_from_its_batch() {
        let mut bytes = vec![0xFF];

        {
            let mut writer = VertexWriter::new(&mut bytes);
            writer.write(&[0x0102_0304_u32]);
            writer.write_bytes(&[5, 6]);

            assert_eq!(writer.len(), size_of::<u32>() + 2);
            assert!(!writer.is_empty());
        }

        assert_eq!(bytes.len(), 1 + size_of::<u32>() + 2);
        assert_eq!(bytes[0], 0xFF);
        assert_eq!(&bytes[1..5], &0x0102_0304_u32.to_ne_bytes());
        assert_eq!(&bytes[5..], &[5, 6]);
    }

    #[test]
    fn batches_disjoint_interleaved_ui_primitives_by_renderer() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut renderer = Renderer::new(
            &device,
            &queue,
            RendererConfig::new(wgpu::TextureFormat::Rgba8UnormSrgb),
        );
        let viewport = Viewport::new(400, 200, 1.0);
        let left = Rectangle::new(
            Point::new(LayoutValue::pixels(0.0), LayoutValue::pixels(0.0)),
            Size::new(LayoutValue::pixels(100.0), LayoutValue::pixels(50.0)),
        );
        let right = Rectangle::new(
            Point::new(LayoutValue::pixels(200.0), LayoutValue::pixels(0.0)),
            Size::new(LayoutValue::pixels(100.0), LayoutValue::pixels(50.0)),
        );
        let mut layer = Layer::new();
        layer.push(Quad::new(left, Color::BLACK));
        layer.push(Text::new(left, "Left", Color::WHITE));
        layer.push(Quad::new(right, Color::BLACK));
        layer.push(Text::new(right, "Right", Color::WHITE));
        let mut scene = Scene::new();
        scene.push(layer);

        let Ok(prepared) = renderer.prepare(viewport, &scene) else {
            std::process::abort();
        };

        assert_eq!(prepared.draw_count(), 2);
        assert_eq!(prepared.layers.len(), 1);
        assert_eq!(prepared.layers[0].draws.len(), 2);
        assert_eq!(
            prepared.layers[0].draws[0].renderer_type_id,
            TypeId::of::<QuadRenderer>()
        );
        assert_eq!(prepared.layers[0].draws[0].batch.len(), 2);
        assert_eq!(
            prepared.layers[0].draws[1].renderer_type_id,
            TypeId::of::<TextRenderer>()
        );
        assert_eq!(prepared.layers[0].draws[1].batch.len(), 2);
    }

    #[test]
    fn batches_overlapping_primitives_within_a_layer() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut renderer = Renderer::new(
            &device,
            &queue,
            RendererConfig::new(wgpu::TextureFormat::Rgba8UnormSrgb),
        );
        let viewport = Viewport::new(400, 200, 1.0);
        let bounds = Rectangle::new(
            Point::default(),
            Size::new(LayoutValue::pixels(400.0), LayoutValue::pixels(200.0)),
        );
        let mut layer = Layer::new();
        layer.push(Quad::new(bounds, Color::BLACK));
        layer.push(Text::new(bounds, "First", Color::WHITE));
        layer.push(Quad::new(bounds, Color::BLACK));
        layer.push(Text::new(bounds, "Second", Color::WHITE));
        let mut scene = Scene::new();
        scene.push(layer);

        let Ok(prepared) = renderer.prepare(viewport, &scene) else {
            std::process::abort();
        };

        assert_eq!(prepared.draw_count(), 2);
    }

    #[test]
    fn layers_are_strict_ordering_barriers() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut renderer = Renderer::new(
            &device,
            &queue,
            RendererConfig::new(wgpu::TextureFormat::Rgba8UnormSrgb),
        );
        let viewport = Viewport::new(400, 200, 1.0);
        let bounds = Rectangle::new(
            Point::default(),
            Size::new(LayoutValue::pixels(400.0), LayoutValue::pixels(200.0)),
        );
        let mut first = Layer::new();
        first.push(Quad::new(bounds, Color::BLACK));
        first.push(Text::new(bounds, "First", Color::WHITE));
        let mut second = Layer::new();
        second.push(Quad::new(bounds, Color::BLACK));
        second.push(Text::new(bounds, "Second", Color::WHITE));
        let mut scene = Scene::new();
        scene.push(first);
        scene.push(second);

        let Ok(prepared) = renderer.prepare(viewport, &scene) else {
            std::process::abort();
        };

        assert_eq!(prepared.draw_count(), 4);
        assert_eq!(prepared.layers.len(), 2);
        assert_eq!(prepared.layers[0].draws.len(), 2);
        assert_eq!(prepared.layers[1].draws.len(), 2);
        assert_eq!(prepared.layers[0].viewport, prepared.layers[1].viewport);
        assert_eq!(prepared.layers[0].scissor, prepared.layers[1].scissor);
        assert_eq!(
            prepared.layers[0].draws[0].renderer_type_id,
            TypeId::of::<QuadRenderer>()
        );
        assert_eq!(
            prepared.layers[0].draws[1].renderer_type_id,
            TypeId::of::<TextRenderer>()
        );
        assert_eq!(
            prepared.layers[1].draws[0].renderer_type_id,
            TypeId::of::<QuadRenderer>()
        );
    }

    fn assert_close(actual: &[f64], expected: &[f64]) {
        assert_eq!(actual.len(), expected.len());
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(actual, expected)| (actual - expected).abs() < f64::EPSILON)
        );
    }
}
