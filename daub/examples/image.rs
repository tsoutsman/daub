use daub::{
    Layer, RendererConfig,
    color::Color,
    geometry::{LayoutValue, Point, Rectangle, Size},
    primitive::{AtlasImage, Image, ImageAtlas},
    winit::{Settings, Window, run},
};
use winit::dpi::LogicalSize;

struct Example {
    _atlas: ImageAtlas,
    small_checkerboard: AtlasImage,
    large_checkerboard: AtlasImage,
}

impl daub::winit::Application for Example {
    fn new(_: &Window, device: &wgpu::Device, queue: &wgpu::Queue, _: RendererConfig) -> Self {
        let mut atlas = ImageAtlas::new(device, queue);
        let small_pixels = checkerboard_pixels(16, 4);
        let Ok(small_checkerboard) = atlas.add_rgba8(16, 16, &small_pixels) else {
            std::process::abort();
        };
        let large_pixels = checkerboard_pixels(320, 80);
        let Ok(large_checkerboard) = atlas.add_rgba8(320, 320, &large_pixels) else {
            std::process::abort();
        };

        Self {
            _atlas: atlas,
            small_checkerboard,
            large_checkerboard,
        }
    }

    fn render(&mut self, frame: daub::winit::Frame<'_>) {
        let left = Rectangle::from_center(
            Point::new(LayoutValue::relative(0.28), LayoutValue::relative(0.5)),
            Size::new(LayoutValue::pixels(320.0), LayoutValue::pixels(320.0)),
        );
        let right = Rectangle::from_center(
            Point::new(LayoutValue::relative(0.72), LayoutValue::relative(0.5)),
            Size::new(LayoutValue::pixels(320.0), LayoutValue::pixels(320.0)),
        );
        let mut layer = Layer::new();
        layer.push(Image::new(left, self.small_checkerboard.clone()));
        layer.push(Image::new(right, self.large_checkerboard.clone()));

        let result = frame.render(layer);
        assert!(result.is_ok(), "{result:?}");
    }
}

fn checkerboard_pixels(size: usize, square_size: usize) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let color = if (x / square_size + y / square_size).is_multiple_of(2) {
                [240, 180, 45, 255]
            } else {
                [45, 95, 190, 255]
            };
            pixels.extend_from_slice(&color);
        }
    }
    pixels
}

fn main() -> daub::winit::Result<()> {
    let settings = Settings::default()
        .window(
            Window::default_attributes()
                .with_title("Daub image example")
                .with_inner_size(LogicalSize::new(900, 500)),
        )
        .clear_color(Color::rgb(0.08, 0.1, 0.14));

    run::<Example>(settings)
}
