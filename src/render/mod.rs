use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt,
    ops::Range,
};

use crate::{
    geometry::{LayoutValue, Rectangle},
    scene::{ErasedBatch, Scene, TypedBatch},
};

/// A value that can be submitted to a [`Scene`].
pub trait Primitive: 'static {
    type Renderer: PrimitiveRenderer<Primitive = Self>;
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
/// Each primitive renderer type owns exactly one render pipeline. A renderer
/// may be called several times in one pass when batches of its primitive type
/// are separated by other primitive types.
pub trait PrimitiveRenderer: Sized + 'static {
    type Primitive: Primitive<Renderer = Self>;

    /// Creates the state owned by this primitive renderer.
    fn new(
        device: &wgpu::Device,
        config: &RendererConfig,
        pipeline_cache: &mut RenderPipelineCache,
    ) -> Self;

    /// Builds the primitive renderer's pipeline when it is not already cached.
    fn build_pipeline(device: &wgpu::Device, config: &RendererConfig) -> wgpu::RenderPipeline;

    /// Returns the primitive renderer's cached pipeline.
    ///
    /// The engine binds this pipeline before calling [`Self::render_batch`].
    fn render_pipeline(&self) -> &wgpu::RenderPipeline;

    /// Starts preparation for a frame.
    ///
    /// This is called once per [`Renderer::prepare`] before any batches are
    /// prepared, including when this renderer has no batches in the frame. It
    /// is a suitable place to clear renderer-owned CPU staging collections.
    fn start_prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, viewport: Viewport) {
        let _ = (device, queue, viewport);
    }

    /// Prepares one consecutive batch and appends its data to the shared vertex
    /// buffer through `vertices`.
    ///
    /// Renderers that manage their own GPU buffers can use the default empty
    /// implementation.
    fn prepare_batch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport: Viewport,
        primitives: &[Self::Primitive],
        vertices: &mut VertexWriter<'_>,
    ) {
        let _ = (device, queue, viewport, primitives, vertices);
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

    /// Draws one consecutive batch.
    ///
    /// `vertex_buffer` contains the bytes appended by [`Self::prepare_batch`]
    /// for this batch. It is `None` when preparation appended no bytes.
    fn render_batch(
        &mut self,
        primitives: &[Self::Primitive],
        render_pass: &mut wgpu::RenderPass<'_>,
        vertex_buffer: Option<wgpu::BufferSlice<'_>>,
    );
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
        device: &wgpu::Device,
        config: &RendererConfig,
    ) -> wgpu::RenderPipeline
    where
        R: PrimitiveRenderer,
    {
        self.pipelines
            .entry(TypeId::of::<R>())
            .or_insert_with(|| R::build_pipeline(device, config))
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
    #[must_use]
    pub fn prepare<'renderer, 'scene>(
        &'renderer mut self,
        viewport: Viewport,
        scenes: &'scene [Scene],
    ) -> PreparedFrame<'renderer, 'scene> {
        self.vertex_bytes.clear();
        let mut draws = Vec::new();
        let mut batches = Vec::new();

        for scene in scenes {
            let Some(scissor) = resolve_scissor(scene.clip_bounds, viewport) else {
                continue;
            };

            for batch in scene.batches() {
                if batch.is_empty() {
                    continue;
                }

                self.ensure_primitive_renderer(batch);
                batches.push((batch, scissor));
            }
        }

        for renderer in self.primitive_renderers.values_mut() {
            renderer.start_prepare(&self.device, &self.queue, viewport);
        }

        for (batch, scissor) in batches {
            align_vec(&mut self.vertex_bytes);
            let start = self.vertex_bytes.len() as wgpu::BufferAddress;

            let renderer = self.primitive_renderers.get_mut(&batch.renderer_type_id());
            let Some(renderer) = renderer else {
                continue;
            };
            let mut vertices = VertexWriter::new(&mut self.vertex_bytes);
            renderer.prepare_batch(&self.device, &self.queue, viewport, batch, &mut vertices);
            let end = start + vertices.len() as wgpu::BufferAddress;

            draws.push(PreparedDraw {
                batch,
                renderer_type_id: batch.renderer_type_id(),
                vertex_range: (start != end).then_some(start..end),
                scissor,
            });
        }

        for renderer in self.primitive_renderers.values_mut() {
            renderer.finish_prepare(&self.device, &self.queue);
        }

        align_vec(&mut self.vertex_bytes);
        self.upload_vertices();

        PreparedFrame {
            renderer: self,
            draws,
        }
    }

    fn ensure_primitive_renderer(&mut self, batch: &dyn ErasedBatch) {
        self.primitive_renderers
            .entry(batch.renderer_type_id())
            .or_insert_with(|| {
                batch.create_renderer(&self.device, &self.config, &mut self.pipeline_cache)
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
    draws: Vec<PreparedDraw<'scene>>,
}

impl PreparedFrame<'_, '_> {
    /// Records every prepared draw in scene and submission order.
    pub fn render(&mut self, render_pass: &mut wgpu::RenderPass<'_>) {
        let Renderer {
            primitive_renderers,
            vertex_buffer,
            ..
        } = &mut *self.renderer;

        for draw in &self.draws {
            let renderer = primitive_renderers.get_mut(&draw.renderer_type_id);
            let Some(renderer) = renderer else {
                continue;
            };

            render_pass.set_scissor_rect(
                draw.scissor.x,
                draw.scissor.y,
                draw.scissor.width,
                draw.scissor.height,
            );
            render_pass.set_pipeline(renderer.render_pipeline());

            let vertex_slice = draw.vertex_range.as_ref().and_then(|range| {
                vertex_buffer
                    .as_ref()
                    .map(|buffer| buffer.slice(range.clone()))
            });
            renderer.render_batch(draw.batch, render_pass, vertex_slice);
        }
    }

    #[must_use]
    pub fn draw_count(&self) -> usize {
        self.draws.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.draws.is_empty()
    }
}

impl fmt::Debug for PreparedFrame<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedFrame")
            .field("draw_count", &self.draw_count())
            .finish_non_exhaustive()
    }
}

struct PreparedDraw<'scene> {
    batch: &'scene dyn ErasedBatch,
    renderer_type_id: TypeId,
    vertex_range: Option<Range<wgpu::BufferAddress>>,
    scissor: ScissorRect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScissorRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

pub(crate) trait ErasedPrimitiveRenderer: Any {
    fn render_pipeline(&self) -> &wgpu::RenderPipeline;

    fn start_prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, viewport: Viewport);

    fn prepare_batch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport: Viewport,
        batch: &dyn ErasedBatch,
        vertices: &mut VertexWriter<'_>,
    );

    fn finish_prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue);

    fn render_batch(
        &mut self,
        batch: &dyn ErasedBatch,
        render_pass: &mut wgpu::RenderPass<'_>,
        vertex_buffer: Option<wgpu::BufferSlice<'_>>,
    );
}

impl<R> ErasedPrimitiveRenderer for R
where
    R: PrimitiveRenderer,
{
    fn render_pipeline(&self) -> &wgpu::RenderPipeline {
        PrimitiveRenderer::render_pipeline(self)
    }

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
    ) {
        let Some(batch) = typed_batch::<R>(batch) else {
            return;
        };
        PrimitiveRenderer::prepare_batch(
            self,
            device,
            queue,
            viewport,
            batch.primitives(),
            vertices,
        );
    }

    fn finish_prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        PrimitiveRenderer::finish_prepare(self, device, queue);
    }

    fn render_batch(
        &mut self,
        batch: &dyn ErasedBatch,
        render_pass: &mut wgpu::RenderPass<'_>,
        vertex_buffer: Option<wgpu::BufferSlice<'_>>,
    ) {
        let Some(batch) = typed_batch::<R>(batch) else {
            return;
        };
        PrimitiveRenderer::render_batch(self, batch.primitives(), render_pass, vertex_buffer);
    }
}

fn typed_batch<R>(batch: &dyn ErasedBatch) -> Option<&TypedBatch<R::Primitive>>
where
    R: PrimitiveRenderer,
{
    batch.downcast_ref::<R::Primitive>()
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
    let width = viewport.resolve_x(rectangle.size.width);
    let height = viewport.resolve_y(rectangle.size.height);
    let position_x = viewport.resolve_x(rectangle.position.x);
    let position_y = viewport.resolve_y(rectangle.position.y);

    let left = position_x - rectangle.anchor.x * width;
    let top = position_y - rectangle.anchor.y * height;
    let right = left + width;
    let bottom = top + height;

    if ![left, top, right, bottom].into_iter().all(f64::is_finite) {
        return None;
    }

    let left = left.clamp(0.0, viewport_width).floor();
    let top = top.clamp(0.0, viewport_height).floor();
    let right = right.clamp(0.0, viewport_width).ceil();
    let bottom = bottom.clamp(0.0, viewport_height).ceil();

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

#[cfg(test)]
mod tests {
    use super::{ScissorRect, VertexWriter, Viewport, resolve_scissor};
    use crate::geometry::{LayoutValue, Point, Rectangle, Size};

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
