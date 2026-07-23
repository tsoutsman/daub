use super::Text;
use crate::{
    color::Color,
    render::{
        PrimitiveRenderer, PrimitiveRendererError, RenderPipelineCache, RendererConfig,
        ResolvedRectangle, VertexWriter, Viewport,
    },
};

/// The Glyphon-backed renderer for [`Text`] primitives.
pub struct TextRenderer {
    cache: glyphon::Cache,
    font_system: glyphon::FontSystem,
    swash_cache: glyphon::SwashCache,
    atlas: glyphon::TextAtlas,
    batches: Vec<TextBatch>,
    sample_count: u32,
    prepare_batch_index: usize,
    render_batch_index: usize,
}

impl std::fmt::Debug for TextRenderer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TextRenderer")
            .field("batch_capacity", &self.batches.len())
            .field("sample_count", &self.sample_count)
            .finish_non_exhaustive()
    }
}

impl PrimitiveRenderer for TextRenderer {
    type Primitive = Text;

    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: &RendererConfig,
        _: &mut RenderPipelineCache,
    ) -> Self {
        let cache = glyphon::Cache::new(device);
        let atlas = glyphon::TextAtlas::new(device, queue, &cache, config.target_format);

        Self {
            cache,
            font_system: glyphon::FontSystem::new(),
            swash_cache: glyphon::SwashCache::new(),
            atlas,
            batches: Vec::new(),
            sample_count: config.sample_count,
            prepare_batch_index: 0,
            render_batch_index: 0,
        }
    }

    fn start_prepare(&mut self, _: &wgpu::Device, _: &wgpu::Queue, _: Viewport) {
        self.atlas.trim();
        self.prepare_batch_index = 0;
        self.render_batch_index = 0;
    }

    fn prepare_batch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport: Viewport,
        primitives: &[Self::Primitive],
        _: &mut VertexWriter<'_>,
    ) -> Result<(), PrimitiveRendererError> {
        let batch_index = self.prepare_batch_index;
        self.prepare_batch_index += 1;

        if batch_index == self.batches.len() {
            self.batches.push(TextBatch::new(
                &mut self.atlas,
                device,
                &self.cache,
                self.sample_count,
            ));
        }

        let batch = &mut self.batches[batch_index];
        batch.viewport.update(
            queue,
            glyphon::Resolution {
                width: viewport.physical_width,
                height: viewport.physical_height,
            },
        );
        batch.prepare_buffers(&mut self.font_system, viewport, primitives);

        let scale = physical_scale(viewport.scale_factor);
        let TextBatch {
            renderer,
            viewport: glyphon_viewport,
            buffers,
            areas,
        } = batch;
        let text_areas = buffers
            .iter()
            .zip(areas.iter())
            .filter(|(_, area)| area.visible)
            .map(|(buffer, area)| glyphon::TextArea {
                buffer,
                left: to_f32(area.rectangle.left),
                top: to_f32(area.rectangle.top),
                scale,
                bounds: text_bounds(area.rectangle),
                default_color: to_glyphon_color(area.color),
                custom_glyphs: &[],
            });

        renderer
            .prepare(
                device,
                queue,
                &mut self.font_system,
                &mut self.atlas,
                glyphon_viewport,
                text_areas,
                &mut self.swash_cache,
            )
            .map_err(|error| Box::new(error) as PrimitiveRendererError)
    }

    fn render_batch(
        &mut self,
        _: &[Self::Primitive],
        render_pass: &mut wgpu::RenderPass<'_>,
        _: Option<wgpu::BufferSlice<'_>>,
    ) -> Result<(), PrimitiveRendererError> {
        let batch_index = self.render_batch_index;
        self.render_batch_index += 1;

        let Some(batch) = self.batches.get(batch_index) else {
            return Ok(());
        };

        batch
            .renderer
            .render(&self.atlas, &batch.viewport, render_pass)
            .map_err(|error| Box::new(error) as PrimitiveRendererError)
    }
}

struct TextBatch {
    renderer: glyphon::TextRenderer,
    viewport: glyphon::Viewport,
    buffers: Vec<glyphon::Buffer>,
    areas: Vec<PreparedTextArea>,
}

impl TextBatch {
    fn new(
        atlas: &mut glyphon::TextAtlas,
        device: &wgpu::Device,
        cache: &glyphon::Cache,
        sample_count: u32,
    ) -> Self {
        Self {
            renderer: glyphon::TextRenderer::new(
                atlas,
                device,
                wgpu::MultisampleState {
                    count: sample_count,
                    ..Default::default()
                },
                None,
            ),
            viewport: glyphon::Viewport::new(device, cache),
            buffers: Vec::new(),
            areas: Vec::new(),
        }
    }

    fn prepare_buffers(
        &mut self,
        font_system: &mut glyphon::FontSystem,
        viewport: Viewport,
        primitives: &[Text],
    ) {
        while self.buffers.len() < primitives.len() {
            self.buffers.push(glyphon::Buffer::new(
                font_system,
                glyphon::Metrics::new(16.0, 20.0),
            ));
        }
        self.buffers.truncate(primitives.len());
        self.areas.clear();
        self.areas.reserve(primitives.len());

        let scale = f64::from(physical_scale(viewport.scale_factor));
        for (text, buffer) in primitives.iter().zip(self.buffers.iter_mut()) {
            let rectangle = viewport
                .resolve_rectangle(text.rectangle)
                .unwrap_or_default();
            let valid_metrics = text.font_size.is_finite()
                && text.font_size > 0.0
                && text.line_height.is_finite()
                && text.line_height > 0.0;
            let visible = valid_metrics && rectangle.width > 0.0 && rectangle.height > 0.0;
            let (font_size, line_height) = if valid_metrics {
                (text.font_size, text.line_height)
            } else {
                (1.0, 1.0)
            };

            buffer.set_metrics_and_size(
                glyphon::Metrics::new(font_size, line_height),
                Some(to_f32(rectangle.width / scale)),
                Some(to_f32(rectangle.height / scale)),
            );
            buffer.set_wrap(text.wrap.to_glyphon());
            let attrs = glyphon::Attrs::new()
                .family(text.family.as_glyphon())
                .weight(text.weight.to_glyphon())
                .stretch(text.stretch.to_glyphon())
                .style(text.style.to_glyphon());
            buffer.set_text(
                if visible { &text.content } else { "" },
                &attrs,
                text.shaping.to_glyphon(),
                None,
            );
            buffer.shape_until_scroll(font_system, false);

            self.areas.push(PreparedTextArea {
                rectangle,
                color: text.color,
                visible,
            });
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PreparedTextArea {
    rectangle: ResolvedRectangle,
    color: Color,
    visible: bool,
}

fn physical_scale(scale_factor: f64) -> f32 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        to_f32(scale_factor)
    } else {
        1.0
    }
}

fn to_glyphon_color(color: Color) -> glyphon::Color {
    glyphon::Color::rgba(
        color_component(color.r),
        color_component(color.g),
        color_component(color.b),
        color_component(color.a),
    )
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "finite normalized color components are clamped before conversion"
)]
fn color_component(component: f32) -> u8 {
    if component.is_finite() {
        (component.clamp(0.0, 1.0) * 255.0).round() as u8
    } else {
        0
    }
}

fn text_bounds(rectangle: ResolvedRectangle) -> glyphon::TextBounds {
    glyphon::TextBounds {
        left: to_i32(rectangle.left.floor()),
        top: to_i32(rectangle.top.floor()),
        right: to_i32(rectangle.right().ceil()),
        bottom: to_i32(rectangle.bottom().ceil()),
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "finite text bounds are saturated to Glyphon's i32 coordinate range"
)]
fn to_i32(value: f64) -> i32 {
    value.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "finite text layout values are intentionally narrowed for Glyphon"
)]
fn to_f32(value: f64) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }

    value.clamp(f64::from(f32::MIN), f64::from(f32::MAX)) as f32
}

#[cfg(test)]
mod tests {
    use super::{color_component, text_bounds};
    use crate::{
        color::Color,
        geometry::{Anchor, LayoutValue, Point, Rectangle, Size},
        primitive::{Quad, Text},
        render::{Renderer, RendererConfig, Viewport},
        scene::{Layer, Scene},
    };

    #[test]
    fn color_conversion_clamps_and_handles_non_finite_values() {
        assert_eq!(color_component(-1.0), 0);
        assert_eq!(color_component(0.5), 128);
        assert_eq!(color_component(2.0), 255);
        assert_eq!(color_component(f32::NAN), 0);
    }

    #[test]
    fn text_bounds_honor_anchor_and_hidpi() {
        let viewport = Viewport::new(800, 600, 2.0);
        let rectangle = Rectangle::from_anchor(
            Point::new(LayoutValue::pixels(200.0), LayoutValue::pixels(100.0)),
            Size::new(LayoutValue::pixels(100.0), LayoutValue::pixels(40.0)),
            Anchor::CENTER,
        );
        let Some(resolved) = viewport.resolve_rectangle(rectangle) else {
            std::process::abort();
        };

        assert_eq!(
            text_bounds(resolved),
            glyphon::TextBounds {
                left: 300,
                top: 160,
                right: 500,
                bottom: 240,
            }
        );
    }

    #[test]
    fn prepares_one_text_batch_for_a_layer_viewport() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut renderer = Renderer::new(
            &device,
            &queue,
            RendererConfig::new(wgpu::TextureFormat::Rgba8UnormSrgb),
        );
        let viewport_bounds = Rectangle::new(
            Point::new(LayoutValue::pixels(100.0), LayoutValue::pixels(50.0)),
            Size::new(LayoutValue::pixels(200.0), LayoutValue::pixels(100.0)),
        );
        let local_bounds = Rectangle::new(
            Point::default(),
            Size::new(LayoutValue::pixels(200.0), LayoutValue::pixels(100.0)),
        );
        let mut layer = Layer::new().viewport(viewport_bounds);
        layer.push(Text::new(local_bounds, "First", Color::WHITE));
        layer.push(Quad::new(local_bounds, Color::BLACK));
        layer.push(Text::new(local_bounds, "Second", Color::WHITE));
        let mut scene = Scene::new();
        scene.push(layer);

        let prepared = renderer.prepare(Viewport::new(400, 200, 1.0), &scene);
        let Ok(mut prepared) = prepared else {
            std::process::abort();
        };

        assert_eq!(prepared.draw_count(), 2);

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("text primitive test target"),
            size: wgpu::Extent3d {
                width: 400,
                height: 200,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("text primitive test encoder"),
        });
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })];
        let result = {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("text primitive test pass"),
                color_attachments: &color_attachments,
                ..Default::default()
            });
            prepared.render(&mut render_pass)
        };

        assert!(result.is_ok(), "{result:?}");
        queue.submit([encoder.finish()]);
    }
}
