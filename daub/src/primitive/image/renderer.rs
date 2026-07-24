use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use bytemuck::{Pod, Zeroable};

use super::Image;
use crate::render::{
    PrimitiveRenderer, PrimitiveRendererError, RenderPipelineCache, RendererConfig, VertexWriter,
    Viewport,
};

#[derive(Debug)]
pub struct ImageRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    atlas_bindings: Vec<AtlasBinding>,
}

impl ImageRenderer {
    fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("daub image bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
    }

    fn build_pipeline(
        device: &wgpu::Device,
        config: &RendererConfig,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("daub image shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("daub image pipeline layout"),
            bind_group_layouts: &[Some(bind_group_layout)],
            immediate_size: 0,
        });
        let vertex_buffers = [Some(ImageInstance::LAYOUT)];
        let targets = [Some(wgpu::ColorTargetState {
            format: config.target_format,
            blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("daub image pipeline"),
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

    fn create_bind_group(&self, device: &wgpu::Device, texture: &wgpu::Texture) -> wgpu::BindGroup {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("daub image atlas bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    fn ensure_atlas_binding(&mut self, device: &wgpu::Device, atlas: &Rc<RefCell<wgpu::Texture>>) {
        let texture = atlas.borrow();
        if let Some(index) = self
            .atlas_bindings
            .iter()
            .position(|binding| binding.belongs_to(atlas))
        {
            if self.atlas_bindings[index].texture != *texture {
                let bind_group = self.create_bind_group(device, &texture);
                self.atlas_bindings[index].texture = texture.clone();
                self.atlas_bindings[index].bind_group = bind_group;
            }
            return;
        }

        let bind_group = self.create_bind_group(device, &texture);
        self.atlas_bindings.push(AtlasBinding {
            atlas: Rc::downgrade(atlas),
            texture: texture.clone(),
            bind_group,
        });
    }

    fn atlas_binding(&self, atlas: &Rc<RefCell<wgpu::Texture>>) -> Option<&AtlasBinding> {
        self.atlas_bindings
            .iter()
            .find(|binding| binding.belongs_to(atlas))
    }
}

impl PrimitiveRenderer for ImageRenderer {
    type Primitive = Image;

    fn new(
        device: &wgpu::Device,
        _: &wgpu::Queue,
        config: &RendererConfig,
        _: &mut RenderPipelineCache,
    ) -> Self {
        let bind_group_layout = Self::create_bind_group_layout(device);
        let pipeline = Self::build_pipeline(device, config, &bind_group_layout);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("daub image sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            atlas_bindings: Vec::new(),
        }
    }

    fn start_prepare(&mut self, _: &wgpu::Device, _: &wgpu::Queue, _: Viewport) {
        self.atlas_bindings
            .retain(|binding| binding.atlas.strong_count() > 0);
    }

    fn prepare_batch(
        &mut self,
        device: &wgpu::Device,
        _: &wgpu::Queue,
        viewport: Viewport,
        primitives: &[Self::Primitive],
        vertices: &mut VertexWriter<'_>,
    ) -> Result<(), PrimitiveRendererError> {
        for image in primitives {
            self.ensure_atlas_binding(device, &image.image.storage);
            let texture = image.image.storage.borrow();
            let instance = ImageInstance::new(image, viewport, texture.width(), texture.height());
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

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_vertex_buffer(0, vertex_buffer);

        let mut start = 0;
        while start < primitives.len() {
            let atlas = &primitives[start].image.storage;
            let end = primitives[start + 1..]
                .iter()
                .position(|image| !Rc::ptr_eq(atlas, &image.image.storage))
                .map_or(primitives.len(), |offset| start + 1 + offset);
            let Some(binding) = self.atlas_binding(atlas) else {
                start = end;
                continue;
            };
            let Ok(start_instance) = u32::try_from(start) else {
                return Ok(());
            };
            let Ok(end_instance) = u32::try_from(end) else {
                return Ok(());
            };

            render_pass.set_bind_group(0, &binding.bind_group, &[]);
            render_pass.draw(0..4, start_instance..end_instance);
            start = end;
        }

        Ok(())
    }
}

#[derive(Debug)]
struct AtlasBinding {
    atlas: Weak<RefCell<wgpu::Texture>>,
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

impl AtlasBinding {
    fn belongs_to(&self, atlas: &Rc<RefCell<wgpu::Texture>>) -> bool {
        self.atlas
            .upgrade()
            .is_some_and(|bound_atlas| Rc::ptr_eq(&bound_atlas, atlas))
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, PartialEq)]
struct ImageInstance {
    center_ndc: [f32; 2],
    size_ndc: [f32; 2],
    texture_origin: [f32; 2],
    texture_size: [f32; 2],
}

impl ImageInstance {
    const ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x2,
        3 => Float32x2,
    ];

    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: size_of::<Self>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &Self::ATTRIBUTES,
    };

    fn new(image: &Image, viewport: Viewport, atlas_width: u32, atlas_height: u32) -> Self {
        let rectangle = viewport
            .resolve_rectangle(image.rectangle)
            .unwrap_or_default();
        let center_x = rectangle.left + rectangle.width * 0.5;
        let center_y = rectangle.top + rectangle.height * 0.5;
        let center_ndc = viewport.to_ndc_position(center_x, center_y).map(to_f32);
        let size_ndc = viewport
            .to_ndc_size(rectangle.width, rectangle.height)
            .map(to_f32);
        let atlas_width = f64::from(atlas_width);
        let atlas_height = f64::from(atlas_height);
        let region = image.image.region;

        Self {
            center_ndc,
            size_ndc,
            texture_origin: [
                to_f32(f64::from(region.x) / atlas_width),
                to_f32(f64::from(region.y) / atlas_height),
            ],
            texture_size: [
                to_f32(f64::from(region.width) / atlas_width),
                to_f32(f64::from(region.height) / atlas_height),
            ],
        }
    }
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
    use super::{ImageInstance, ImageRenderer};
    use crate::{
        geometry::{LayoutValue, Point, Rectangle, Size},
        primitive::{Image, ImageAtlas},
        render::{PrimitiveRenderer, RenderPipelineCache, Renderer, RendererConfig, Viewport},
        scene::{Layer, Scene},
    };

    #[test]
    fn resolves_image_instance_against_viewport_and_atlas() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut atlas = ImageAtlas::new(&device, &queue);
        let atlas_image = atlas
            .add_rgba8(2, 4, &[255; 32])
            .unwrap_or_else(|_| std::process::abort());
        let rectangle = Rectangle::from_center(
            Point::new(LayoutValue::relative(0.5), LayoutValue::relative(0.5)),
            Size::new(LayoutValue::pixels(100.0), LayoutValue::pixels(50.0)),
        );
        let image = Image::new(rectangle, atlas_image);
        let texture = image.image.storage.borrow();

        let instance = ImageInstance::new(
            &image,
            Viewport::new(800, 600, 2.0),
            texture.width(),
            texture.height(),
        );

        assert_close(&instance.center_ndc, &[0.0, 0.0]);
        assert_close(&instance.size_ndc, &[0.5, 1.0 / 3.0]);
        assert_close(&instance.texture_origin, &[1.0 / 256.0, 1.0 / 256.0]);
        assert_close(&instance.texture_size, &[2.0 / 256.0, 4.0 / 256.0]);
    }

    #[test]
    fn instance_layout_matches_shader_inputs() {
        assert_eq!(size_of::<ImageInstance>(), 32);
        assert_eq!(ImageInstance::LAYOUT.array_stride, 32);
        assert_eq!(
            ImageInstance::LAYOUT.step_mode,
            wgpu::VertexStepMode::Instance
        );
        assert_eq!(ImageInstance::ATTRIBUTES.len(), 4);
    }

    #[test]
    fn binding_is_replaced_after_atlas_growth() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut atlas = ImageAtlas::new(&device, &queue);
        let image = atlas
            .add_rgba8(1, 1, &[255; 4])
            .unwrap_or_else(|_| std::process::abort());
        let mut renderer = ImageRenderer::new(
            &device,
            &queue,
            &RendererConfig::new(wgpu::TextureFormat::Rgba8UnormSrgb),
            &mut RenderPipelineCache::new(),
        );
        renderer.ensure_atlas_binding(&device, &image.storage);
        let original_texture = renderer.atlas_bindings[0].texture.clone();

        atlas
            .add_rgba8(255, 1, &[255; 1020])
            .unwrap_or_else(|_| std::process::abort());
        renderer.ensure_atlas_binding(&device, &image.storage);

        assert_eq!(renderer.atlas_bindings.len(), 1);
        assert_ne!(renderer.atlas_bindings[0].texture, original_texture);
    }

    #[test]
    fn renders_images_from_multiple_atlases() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut first_atlas = ImageAtlas::new(&device, &queue);
        let first_image = first_atlas
            .add_rgba8(1, 1, &[255, 0, 0, 255])
            .unwrap_or_else(|_| std::process::abort());
        let mut second_atlas = ImageAtlas::new(&device, &queue);
        let second_image = second_atlas
            .add_rgba8(1, 1, &[0, 0, 255, 255])
            .unwrap_or_else(|_| std::process::abort());
        let rectangle = Rectangle::new(
            Point::new(LayoutValue::pixels(0.0), LayoutValue::pixels(0.0)),
            Size::new(LayoutValue::pixels(16.0), LayoutValue::pixels(16.0)),
        );
        let mut layer = Layer::new();
        layer.push(Image::new(rectangle, first_image.clone()));
        layer.push(Image::new(rectangle, second_image));
        layer.push(Image::new(rectangle, first_image));
        let mut renderer = Renderer::new(
            &device,
            &queue,
            RendererConfig::new(wgpu::TextureFormat::Rgba8UnormSrgb),
        );
        let scene = Scene::from(layer);
        let prepared = renderer.prepare(Viewport::new(16, 16, 1.0), &scene);
        let Ok(mut prepared) = prepared else {
            std::process::abort();
        };

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("image primitive test target"),
            size: wgpu::Extent3d {
                width: 16,
                height: 16,
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
            label: Some("image primitive test encoder"),
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
                label: Some("image primitive test pass"),
                color_attachments: &color_attachments,
                ..Default::default()
            });
            prepared.render(&mut render_pass)
        };

        assert!(result.is_ok(), "{result:?}");
        queue.submit([encoder.finish()]);
    }

    #[test]
    fn pipeline_matches_shader_and_instance_layout() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let _renderer = ImageRenderer::new(
            &device,
            &queue,
            &RendererConfig::new(wgpu::TextureFormat::Rgba8UnormSrgb),
            &mut RenderPipelineCache::new(),
        );
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
