use daub::{
    Layer, RendererConfig,
    color::Color,
    geometry::{LayoutValue, Point, Rectangle, Size},
    primitive::{CornerRadii, Quad, Text},
};

struct Example;

impl daub::winit::Application for Example {
    fn new(_: &daub::winit::Window, _: &wgpu::Device, _: &wgpu::Queue, _: RendererConfig) -> Self {
        Self
    }

    fn render(&mut self, frame: daub::winit::Frame<'_>) {
        let card = Rectangle::new(
            Point::new(LayoutValue::relative(0.1), LayoutValue::relative(0.15)),
            Size::new(LayoutValue::relative(0.8), LayoutValue::relative(0.7)),
        );
        let text_bounds = Rectangle::new(
            Point::new(LayoutValue::relative(0.15), LayoutValue::relative(0.22)),
            Size::new(LayoutValue::relative(0.7), LayoutValue::relative(0.5)),
        );

        let mut layer = Layer::new();
        layer.push(
            Quad::new(card, Color::rgb(0.12, 0.16, 0.24))
                .corner_radii(CornerRadii::uniform(LayoutValue::pixels(12.0))),
        );
        layer.push(
            Text::new(
                text_bounds,
                "Hello from Daub and Glyphon 👋\n\nThis text wraps inside its rectangle, supports \
                 Unicode shaping, and remains crisp on HiDPI displays.",
                Color::WHITE,
            )
            .font_size(28.0)
            .line_height(36.0),
        );
        layer.push(
            Quad::new(
                Rectangle::new(
                    Point::new(LayoutValue::relative(0.15), LayoutValue::relative(0.78)),
                    Size::new(LayoutValue::relative(0.22), LayoutValue::pixels(4.0)),
                ),
                Color::rgb(0.35, 0.7, 1.0),
            )
            .corner_radii(CornerRadii::uniform(LayoutValue::pixels(2.0))),
        );

        let result = frame.render(layer);
        assert!(result.is_ok(), "{result:?}");
    }
}

fn main() -> Result<(), daub::winit::Error> {
    let settings = daub::winit::Settings::default()
        .window(
            daub::winit::Window::default_attributes()
                .with_title("Daub text example")
                .with_inner_size(daub::winit::LogicalSize::new(900, 600)),
        )
        .clear_color(Color::rgb(0.05, 0.07, 0.11));

    daub::winit::run::<Example>(settings)
}
