//! An opinionated single-window runner for Daub applications.

use std::{error, fmt, sync::Arc};

pub use ::winit::{
    dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize},
    event::WindowEvent,
    keyboard::{Key, KeyCode, PhysicalKey},
    window::{Window, WindowAttributes},
};
use derive_setters::Setters;

use crate::{
    Renderer, RendererConfig, Scene, Viewport,
    color::Color,
    geometry::{LayoutValue, Point, Rectangle, Size},
};

/// Controls when another frame is requested after rendering.
#[cfg_attr(feature = "ocaml", derive(ocaml::FromValue, ocaml::ToValue))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RedrawMode {
    /// Render only for the initial frame, resizing, or an explicit
    /// [`EventAction::Redraw`].
    #[default]
    OnDemand,
    /// Request another redraw after every presented frame.
    Continuous,
}

/// An action requested in response to a window event.
#[cfg_attr(feature = "ocaml", derive(ocaml::FromValue, ocaml::ToValue))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventAction {
    #[default]
    None,
    Redraw,
    Exit,
}

/// Window and graphics settings used by [`run`].
#[derive(Debug, Clone, Setters)]
pub struct Settings {
    pub window: WindowAttributes,
    pub clear_color: Color,
    pub redraw_mode: RedrawMode,
    pub power_preference: wgpu::PowerPreference,
    pub present_mode: wgpu::PresentMode,
    pub required_features: wgpu::Features,
    pub required_limits: wgpu::Limits,
    pub sample_count: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            window: Window::default_attributes().with_title("Daub"),
            clear_color: Color::BLACK,
            redraw_mode: RedrawMode::OnDemand,
            power_preference: wgpu::PowerPreference::default(),
            present_mode: wgpu::PresentMode::AutoVsync,
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            sample_count: 1,
        }
    }
}

/// User state driven by the Daub window runner.
pub trait Application: Sized + 'static {
    /// Creates the application after the window and graphics device exist.
    fn new(
        window: &Window,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer_config: RendererConfig,
    ) -> Self;

    /// Handles a window event not consumed by the runner.
    fn window_event(&mut self, window: &Window, event: &WindowEvent) -> EventAction {
        let _ = (window, event);
        EventAction::None
    }

    /// Builds and renders one frame.
    ///
    /// Call [`Frame::render`] to present a scene. The frame is single-use.
    fn render(&mut self, frame: Frame<'_>);
}

/// A single opportunity to render to the current surface texture.
pub struct Frame<'a> {
    window: &'a Window,
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    renderer: &'a mut Renderer,
    viewport: Viewport,
    clear_color: Color,
    multisample_view: Option<&'a wgpu::TextureView>,
    target_view: wgpu::TextureView,
    surface_texture: wgpu::SurfaceTexture,
}

impl Frame<'_> {
    #[must_use]
    pub const fn window(&self) -> &Window {
        self.window
    }

    #[must_use]
    pub const fn device(&self) -> &wgpu::Device {
        self.device
    }

    #[must_use]
    pub const fn queue(&self) -> &wgpu::Queue {
        self.queue
    }

    #[must_use]
    pub const fn viewport(&self) -> Viewport {
        self.viewport
    }

    /// Returns a top-left-anchored rectangle covering the complete viewport.
    #[must_use]
    pub const fn bounds(&self) -> Rectangle {
        Rectangle::new(
            Point::new(LayoutValue::ZERO, LayoutValue::ZERO),
            Size::new(LayoutValue::relative(1.0), LayoutValue::relative(1.0)),
        )
    }

    /// Renders and presents the supplied scenes.
    ///
    /// Consuming the frame prevents the renderer's shared vertex data from
    /// being prepared more than once before submission.
    pub fn render(self, scenes: &[Scene]) {
        let Self {
            device,
            queue,
            renderer,
            viewport,
            clear_color,
            multisample_view,
            target_view,
            surface_texture,
            ..
        } = self;

        let mut prepared = renderer.prepare(viewport, scenes);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("daub frame encoder"),
        });

        let store = if multisample_view.is_some() {
            wgpu::StoreOp::Discard
        } else {
            wgpu::StoreOp::Store
        };
        let (render_view, resolve_target) =
            multisample_view.map_or((&target_view, None), |view| (view, Some(&target_view)));
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: render_view,
            depth_slice: None,
            resolve_target,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(to_wgpu_color(clear_color)),
                store,
            },
        })];

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("daub frame render pass"),
                color_attachments: &color_attachments,
                ..Default::default()
            });
            prepared.render(&mut render_pass);
        }

        queue.submit([encoder.finish()]);
        queue.present(surface_texture);
    }
}

impl fmt::Debug for Frame<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Frame")
            .field("viewport", &self.viewport)
            .field("clear_color", &self.clear_color)
            .finish_non_exhaustive()
    }
}

/// An error produced while creating or running a windowed application.
#[derive(Debug)]
pub enum Error {
    EventLoop(::winit::error::EventLoopError),
    Window(::winit::error::OsError),
    Surface(wgpu::CreateSurfaceError),
    Adapter(wgpu::RequestAdapterError),
    Device(wgpu::RequestDeviceError),
    UnsupportedSurface,
    UnsupportedSampleCount {
        format: wgpu::TextureFormat,
        sample_count: u32,
    },
    SurfaceValidation,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EventLoop(error) => write!(formatter, "window event-loop error: {error}"),
            Self::Window(error) => write!(formatter, "window creation error: {error}"),
            Self::Surface(error) => write!(formatter, "surface creation error: {error}"),
            Self::Adapter(error) => write!(formatter, "adapter request error: {error}"),
            Self::Device(error) => write!(formatter, "device request error: {error}"),
            Self::UnsupportedSurface => formatter.write_str("the surface is unsupported"),
            Self::UnsupportedSampleCount {
                format,
                sample_count,
            } => write!(
                formatter,
                "sample count {sample_count} is unsupported for {format:?}"
            ),
            Self::SurfaceValidation => {
                formatter.write_str("surface texture acquisition failed validation")
            }
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::EventLoop(error) => Some(error),
            Self::Window(error) => Some(error),
            Self::Surface(error) => Some(error),
            Self::Adapter(error) => Some(error),
            Self::Device(error) => Some(error),
            Self::UnsupportedSurface
            | Self::UnsupportedSampleCount { .. }
            | Self::SurfaceValidation => None,
        }
    }
}

/// Runs a single-window Daub application.
///
/// This initial runner uses blocking graphics initialization and is intended
/// for native targets. A Web runner requires an asynchronous initialization
/// state machine.
///
/// # Errors
///
/// Returns an error if the window, event loop, surface, adapter, or device
/// cannot be created, or if the requested surface configuration is unsupported.
pub fn run<A>(settings: Settings) -> Result<(), Error>
where
    A: Application,
{
    use ::winit::event_loop::{ControlFlow, EventLoop};

    let event_loop = EventLoop::new().map_err(Error::EventLoop)?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut runner = Runner::<A>::new(settings);
    let result = event_loop.run_app(&mut runner);

    if let Some(error) = runner.error {
        Err(error)
    } else {
        result.map_err(Error::EventLoop)
    }
}

struct Runner<A> {
    settings: Settings,
    state: Option<State<A>>,
    error: Option<Error>,
}

impl<A> Runner<A> {
    const fn new(settings: Settings) -> Self {
        Self {
            settings,
            state: None,
            error: None,
        }
    }

    fn fail(&mut self, event_loop: &::winit::event_loop::ActiveEventLoop, error: Error) {
        self.error = Some(error);
        event_loop.exit();
    }
}

impl<A> ::winit::application::ApplicationHandler for Runner<A>
where
    A: Application,
{
    fn resumed(&mut self, event_loop: &::winit::event_loop::ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.graphics.window.request_redraw();
            return;
        }

        let window = match event_loop.create_window(self.settings.window.clone()) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.fail(event_loop, Error::Window(error));
                return;
            }
        };

        let graphics = match pollster::block_on(Graphics::new(
            Arc::clone(&window),
            event_loop,
            &self.settings,
        )) {
            Ok(graphics) => graphics,
            Err(error) => {
                self.fail(event_loop, error);
                return;
            }
        };
        let app = A::new(
            &window,
            &graphics.device,
            &graphics.queue,
            graphics.renderer.config(),
        );

        window.request_redraw();
        self.state = Some(State {
            app,
            graphics,
            occluded: false,
        });
    }

    fn window_event(
        &mut self,
        event_loop: &::winit::event_loop::ActiveEventLoop,
        window_id: ::winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else {
            return;
        };
        if state.graphics.window.id() != window_id {
            return;
        }

        if matches!(event, WindowEvent::CloseRequested) {
            event_loop.exit();
            return;
        }

        if matches!(event, WindowEvent::RedrawRequested) {
            let result = state.render(self.settings.clear_color);
            match result {
                Ok(request_redraw) => {
                    if request_redraw
                        || (self.settings.redraw_mode == RedrawMode::Continuous && !state.occluded)
                    {
                        state.graphics.window.request_redraw();
                    }
                }
                Err(error) => self.fail(event_loop, error),
            }
            return;
        }

        let mut request_redraw = false;
        match &event {
            WindowEvent::Resized(size) => {
                state.graphics.resize(*size);
                request_redraw = true;
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                state.graphics.resize(state.graphics.window.inner_size());
                request_redraw = true;
            }
            WindowEvent::Occluded(occluded) => {
                state.occluded = *occluded;
                request_redraw = !occluded;
            }
            _ => {}
        }

        match state.app.window_event(&state.graphics.window, &event) {
            EventAction::None => {}
            EventAction::Redraw => request_redraw = true,
            EventAction::Exit => {
                event_loop.exit();
                return;
            }
        }

        if request_redraw {
            state.graphics.window.request_redraw();
        }
    }
}

struct State<A> {
    app: A,
    graphics: Graphics,
    occluded: bool,
}

impl<A> State<A>
where
    A: Application,
{
    fn render(&mut self, clear_color: Color) -> Result<bool, Error> {
        if !self.graphics.configured {
            return Ok(false);
        }

        let (surface_texture, suboptimal) = match self.graphics.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => (texture, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => (texture, true),
            wgpu::CurrentSurfaceTexture::Timeout => return Ok(true),
            wgpu::CurrentSurfaceTexture::Occluded => return Ok(false),
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.graphics.resize(self.graphics.window.inner_size());
                return Ok(true);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.graphics.recreate_surface()?;
                return Ok(true);
            }
            wgpu::CurrentSurfaceTexture::Validation => return Err(Error::SurfaceValidation),
        };

        let target_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let viewport = Viewport::new(
            self.graphics.surface_config.width,
            self.graphics.surface_config.height,
            self.graphics.window.scale_factor(),
        );
        let frame = Frame {
            window: &self.graphics.window,
            device: &self.graphics.device,
            queue: &self.graphics.queue,
            renderer: &mut self.graphics.renderer,
            viewport,
            clear_color,
            multisample_view: self
                .graphics
                .multisample_target
                .as_ref()
                .map(|target| &target.view),
            target_view,
            surface_texture,
        };
        self.app.render(frame);

        if suboptimal {
            self.graphics.resize(self.graphics.window.inner_size());
        }

        Ok(false)
    }
}

struct Graphics {
    instance: wgpu::Instance,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    multisample_target: Option<MultisampleTarget>,
    sample_count: u32,
    configured: bool,
}

impl Graphics {
    async fn new(
        window: Arc<Window>,
        event_loop: &::winit::event_loop::ActiveEventLoop,
        settings: &Settings,
    ) -> Result<Self, Error> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(
                Box::new(event_loop.owned_display_handle()),
            ));
        let surface = instance
            .create_surface(Arc::clone(&window))
            .map_err(Error::Surface)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: settings.power_preference,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(Error::Adapter)?;

        let size = window.inner_size();
        let mut surface_config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or(Error::UnsupportedSurface)?;
        surface_config.present_mode = settings.present_mode;

        let format_features = adapter.get_texture_format_features(surface_config.format);
        let supports_multisampling = format_features
            .flags
            .sample_count_supported(settings.sample_count)
            && (settings.sample_count == 1
                || format_features
                    .flags
                    .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE));
        if !supports_multisampling {
            return Err(Error::UnsupportedSampleCount {
                format: surface_config.format,
                sample_count: settings.sample_count,
            });
        }

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("daub window device"),
                required_features: settings.required_features,
                required_limits: settings.required_limits.clone(),
                ..Default::default()
            })
            .await
            .map_err(Error::Device)?;
        let renderer = Renderer::new(
            &device,
            &queue,
            RendererConfig {
                target_format: surface_config.format,
                sample_count: settings.sample_count,
            },
        );

        let mut graphics = Self {
            instance,
            window,
            surface,
            device,
            queue,
            surface_config,
            renderer,
            multisample_target: None,
            sample_count: settings.sample_count,
            configured: false,
        };
        graphics.resize(size);
        Ok(graphics)
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            self.configured = false;
            self.multisample_target = None;
            return;
        }

        self.surface_config.width = size.width;
        self.surface_config.height = size.height;
        self.surface.configure(&self.device, &self.surface_config);
        self.multisample_target =
            create_multisample_target(&self.device, &self.surface_config, self.sample_count);
        self.configured = true;
    }

    fn recreate_surface(&mut self) -> Result<(), Error> {
        self.surface = self
            .instance
            .create_surface(Arc::clone(&self.window))
            .map_err(Error::Surface)?;
        self.resize(self.window.inner_size());
        Ok(())
    }
}

struct MultisampleTarget {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

fn create_multisample_target(
    device: &wgpu::Device,
    surface_config: &wgpu::SurfaceConfiguration,
    sample_count: u32,
) -> Option<MultisampleTarget> {
    if sample_count == 1 {
        return None;
    }

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("daub multisample texture"),
        size: wgpu::Extent3d {
            width: surface_config.width,
            height: surface_config.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: surface_config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    Some(MultisampleTarget {
        _texture: texture,
        view,
    })
}

fn to_wgpu_color(color: Color) -> wgpu::Color {
    wgpu::Color {
        r: f64::from(color.red),
        g: f64::from(color.green),
        b: f64::from(color.blue),
        a: f64::from(color.alpha),
    }
}
