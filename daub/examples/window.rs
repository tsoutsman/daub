use daub::{
    RendererConfig, Scene,
    color::Color,
    geometry::{Anchor, LayoutValue, Point, Rectangle, Size},
    primitive::{Border, CornerRadii, Quad},
    winit::{Settings, Window, run},
};
use winit::dpi::LogicalSize;

struct Example;

impl daub::winit::Application for Example {
    fn new(_: &Window, _: &wgpu::Device, _: &wgpu::Queue, _: RendererConfig) -> Self {
        Self
    }

    fn render(&mut self, frame: daub::winit::Frame<'_>) {
        let mut scene = Scene::new(frame.bounds());
        scene.push(Quad {
            rectangle: Rectangle {
                position: Point::new(LayoutValue::Relative(0.5), LayoutValue::Relative(0.5)),
                size: Size::new(LayoutValue::Relative(0.5), LayoutValue::Relative(0.5)),
                anchor: Anchor::CENTER,
            },
            color: Color::WHITE,
            border: Border::new(Color::BLACK, LayoutValue::pixels(5.)),
            corner_radii: CornerRadii::uniform(LayoutValue::pixels(3.)),
        });
        frame.render(&[scene]);
    }
}

fn main() -> daub::winit::Result<()> {
    let settings = Settings::default()
        .window(
            Window::default_attributes()
                .with_title("Daub window example")
                .with_inner_size(LogicalSize::new(800, 600)),
        )
        .clear_color(Color::rgb(0.08, 0.12, 0.2));

    run::<Example>(settings)
}
