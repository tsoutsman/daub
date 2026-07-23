use bytemuck::{Pod, Zeroable};

use super::Quad;
use crate::{
    color::Color,
    render::{
        PrimitiveRenderer, PrimitiveRendererError, RenderPipelineCache, RendererConfig,
        VertexWriter, Viewport,
    },
};

#[derive(Debug)]
pub struct QuadRenderer {
    pipeline: wgpu::RenderPipeline,
}

impl QuadRenderer {
    fn build_pipeline(device: &wgpu::Device, config: &RendererConfig) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("daub quad shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("daub quad pipeline layout"),
            ..Default::default()
        });
        let vertex_buffers = [Some(QuadInstance::LAYOUT)];
        let targets = [Some(wgpu::ColorTargetState {
            format: config.target_format,
            blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("daub quad pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &vertex_buffers,
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: config.sample_count,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    }
}

impl PrimitiveRenderer for QuadRenderer {
    type Primitive = Quad;

    fn new(
        device: &wgpu::Device,
        _: &wgpu::Queue,
        config: &RendererConfig,
        pipeline_cache: &mut RenderPipelineCache,
    ) -> Self {
        Self {
            pipeline: pipeline_cache.get_or_create::<Self>(|| Self::build_pipeline(device, config)),
        }
    }

    fn prepare_batch(
        &mut self,
        _: &wgpu::Device,
        _: &wgpu::Queue,
        viewport: Viewport,
        primitives: &[Self::Primitive],
        vertices: &mut VertexWriter<'_>,
    ) -> Result<(), PrimitiveRendererError> {
        for quad in primitives {
            let instance = QuadInstance::new(*quad, viewport);
            vertices.write(std::slice::from_ref(&instance));
        }

        Ok(())
    }

    fn render_batch(
        &mut self,
        primitives: &[Self::Primitive],
        render_pass: &mut wgpu::RenderPass<'_>,
        vertex_buffer: Option<wgpu::BufferSlice<'_>>,
    ) -> Result<(), PrimitiveRendererError> {
        let Some(vertex_buffer) = vertex_buffer else {
            return Ok(());
        };
        let Ok(instance_count) = u32::try_from(primitives.len()) else {
            return Ok(());
        };

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_vertex_buffer(0, vertex_buffer);
        render_pass.draw(0..4, 0..instance_count);
        Ok(())
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, PartialEq)]
struct QuadInstance {
    color: [f32; 4],
    border_color: [f32; 4],
    center_ndc: [f32; 2],
    size_ndc: [f32; 2],
    size_pixels: [f32; 2],
    corner_radii_pixels: [f32; 4],
    border_width_pixels: f32,
}

impl QuadInstance {
    const ATTRIBUTES: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
        0 => Float32x4,
        1 => Float32x4,
        2 => Float32x2,
        3 => Float32x2,
        4 => Float32x2,
        5 => Float32x4,
        6 => Float32,
    ];

    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: size_of::<Self>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &Self::ATTRIBUTES,
    };

    fn new(quad: Quad, viewport: Viewport) -> Self {
        let rectangle = viewport
            .resolve_rectangle(quad.rectangle)
            .unwrap_or_default();
        let center_x = rectangle.left + rectangle.width * 0.5;
        let center_y = rectangle.top + rectangle.height * 0.5;
        let center_ndc = viewport.to_ndc_position(center_x, center_y).map(to_f32);
        let size_ndc = viewport
            .to_ndc_size(rectangle.width, rectangle.height)
            .map(to_f32);
        let relative_radius = rectangle.width.min(rectangle.height);

        Self {
            color: color_components(quad.color),
            border_color: color_components(quad.border.color),
            center_ndc,
            size_ndc,
            size_pixels: [to_f32(rectangle.width), to_f32(rectangle.height)],
            corner_radii_pixels: [
                to_f32(viewport.resolve(quad.corner_radii.top_left, relative_radius)),
                to_f32(viewport.resolve(quad.corner_radii.top_right, relative_radius)),
                to_f32(viewport.resolve(quad.corner_radii.bottom_right, relative_radius)),
                to_f32(viewport.resolve(quad.corner_radii.bottom_left, relative_radius)),
            ],
            border_width_pixels: to_f32(viewport.resolve(quad.border.width, relative_radius)),
        }
    }
}

fn color_components(color: Color) -> [f32; 4] {
    [color.r, color.g, color.b, color.a]
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "GPU vertex attributes use f32; finite layout values are intentionally narrowed"
)]
fn to_f32(value: f64) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }

    value.clamp(f64::from(f32::MIN), f64::from(f32::MAX)) as f32
}

#[cfg(test)]
mod tests {
    use super::{QuadInstance, QuadRenderer};
    use crate::{
        color::Color,
        geometry::{LayoutValue, Point, Rectangle, Size},
        primitive::{Border, CornerRadii, Quad},
        render::{RendererConfig, Viewport},
    };

    #[test]
    fn resolves_quad_instance_against_viewport() {
        let rectangle = Rectangle::from_center(
            Point::new(LayoutValue::relative(0.5), LayoutValue::relative(0.5)),
            Size::new(LayoutValue::pixels(100.0), LayoutValue::pixels(50.0)),
        );
        let quad = Quad::new(rectangle, Color::WHITE)
            .border(Border::new(Color::BLACK, LayoutValue::pixels(2.0)))
            .corner_radii(CornerRadii::uniform(LayoutValue::pixels(3.0)));

        let instance = QuadInstance::new(quad, Viewport::new(800, 600, 2.0));

        assert_close(&instance.center_ndc, &[0.0, 0.0]);
        assert_close(&instance.size_pixels, &[200.0, 100.0]);
        assert_close(&instance.size_ndc, &[0.5, 1.0 / 3.0]);
        assert_close(&instance.corner_radii_pixels, &[6.0; 4]);
        assert_close(&[instance.border_width_pixels], &[4.0]);
    }

    #[test]
    fn instance_layout_matches_shader_inputs() {
        assert_eq!(size_of::<QuadInstance>(), 76);
        assert_eq!(QuadInstance::LAYOUT.array_stride, 76);
        assert_eq!(
            QuadInstance::LAYOUT.step_mode,
            wgpu::VertexStepMode::Instance
        );
        assert_eq!(QuadInstance::ATTRIBUTES.len(), 7);
    }

    #[test]
    fn pipeline_matches_shader_and_instance_layout() {
        let (device, _) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let config = RendererConfig::new(wgpu::TextureFormat::Rgba8UnormSrgb);

        let _pipeline = QuadRenderer::build_pipeline(&device, &config);
    }

    #[test]
    fn shader_is_valid_wgsl() {
        let parse_result = wgpu::naga::front::wgsl::parse_str(include_str!("shader.wgsl"));
        let Ok(module) = parse_result else {
            std::process::abort();
        };
        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        );
        let validation_result = validator.validate(&module);

        assert!(validation_result.is_ok(), "{validation_result:?}");
    }

    fn assert_close(actual: &[f32], expected: &[f32]) {
        assert_eq!(actual.len(), expected.len());
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(actual, expected)| (actual - expected).abs() < 1e-6)
        );
    }
}
