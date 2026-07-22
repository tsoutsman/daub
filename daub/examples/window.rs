use daub::{
    RendererConfig, Scene,
    color::Color,
    geometry::{Anchor, LayoutValue, Point, Rectangle, Size},
    primitive::{Border, CornerRadii, Quad},
};

struct Example;

impl daub::winit::Application for Example {
    fn new(_: &daub::winit::Window, _: &wgpu::Device, _: &wgpu::Queue, _: RendererConfig) -> Self {
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
        frame.render(std::slice::from_ref(&scene));
    }
}

fn main() -> Result<(), daub::winit::Error> {
    let settings = daub::winit::Settings::default()
        .window(
            daub::winit::Window::default_attributes()
                .with_title("Daub window example")
                .with_inner_size(daub::winit::LogicalSize::new(800, 600)),
        )
        .clear_color(Color::rgb(0.08, 0.12, 0.2));

    daub::winit::run::<Example>(settings)
}
