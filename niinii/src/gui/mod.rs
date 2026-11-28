//! Dear ImGui on winit + wgpu. Every ImGui window is its own native window;
//! the winit host window only anchors the main viewport and stays hidden.

mod fonts;
mod frame;
mod native;
mod style;
mod transitions;

use std::{path::PathBuf, sync::Arc};

use dear_imgui_rs::{
    ConfigFlags, Context, SystemClipboard, Theme, TooltipHoveredFlags, TreeLineMode, Ui,
};
use dear_imgui_wgpu::{
    multi_viewport::{WgpuPreparedMainFrame, WinitViewportRoute},
    FramebufferExtent, GammaMode, WgpuInitInfo, WgpuRenderer, WgpuViewportSurfaceConfig,
};
use dear_imgui_winit::{HiDpiMode, WinitPlatform, WinitViewportWindowConfig};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::{Event, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

pub use fonts::{px, px2, FontSize};
pub use frame::Frame;
use native::NativeWindows;
pub use style::capture_theme;
use style::StyleScale;
use transitions::NoTransitions;

pub type Error = Box<dyn std::error::Error>;

/// Startup configuration.
pub struct Options {
    pub title: String,
    pub ini_path: PathBuf,
    /// Applied over the default style, before scaling to the monitor.
    pub theme: Option<Theme>,
}

/// Runs the UI until a frame calls [`Frame::exit`] or the event loop ends.
pub fn run(options: Options, build: impl FnMut(&Ui, &mut Frame)) -> Result<(), Error> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut shell = Shell {
        options,
        build,
        gui: None,
        error: None,
    };
    event_loop.run_app(&mut shell)?;
    shell.error.map_or(Ok(()), Err)
}

struct Shell<F> {
    options: Options,
    build: F,
    gui: Option<Gui>,
    error: Option<Error>,
}

impl<F> Shell<F> {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: Error) {
        self.error.get_or_insert(error);
        event_loop.exit();
    }
}

impl<F: FnMut(&Ui, &mut Frame)> ApplicationHandler for Shell<F> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gui.is_some() {
            return;
        }
        match Gui::new(event_loop, &self.options) {
            Ok(gui) => self.gui = Some(gui),
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(gui) = self.gui.as_mut() else { return };
        if let Err(error) = gui.handle_window_event(id, event) {
            self.fail(event_loop, error);
        }
    }

    // The host window is hidden and never painted, so frames are driven from
    // the loop itself; presenting to the vsynced surfaces paces it.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(gui) = self.gui.as_mut() else { return };
        match gui.frame(event_loop, &mut self.build) {
            Ok(true) => {}
            Ok(false) => event_loop.exit(),
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(mut gui) = self.gui.take() {
            if let Err(error) = gui.shutdown() {
                self.error.get_or_insert(error);
            }
        }
    }
}

struct Gui {
    host: Arc<Window>,
    host_surface: HostSurface,
    platform: WinitPlatform,
    route: WinitViewportRoute,
    native: NativeWindows,
    _transitions: NoTransitions,
    /// DPI of the primary monitor at startup, which the style is scaled to.
    dpi: f32,
    style: StyleScale,
    adapter: Arc<str>,
    /// Declared by the previous frame.
    text_scale: f32,
    // Dropped last: the platform and renderer hold state registered with it.
    imgui: Context,
}

impl Gui {
    fn new(event_loop: &ActiveEventLoop, options: &Options) -> Result<Self, Error> {
        let host = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title(options.title.clone())
                    .with_visible(false)
                    .with_decorations(false)
                    .with_position(PhysicalPosition::new(0, 0))
                    .with_inner_size(PhysicalSize::new(1, 1)),
            )?,
        );

        // A DXGI swapchain made directly from an HWND cannot be transparent;
        // one made from a DirectComposition visual can.
        let mut instance_desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        let backends =
            wgpu::Backends::from_env().unwrap_or(wgpu::Backends::DX12 | wgpu::Backends::GL);
        instance_desc.backends = backends;
        instance_desc.backend_options.dx12.presentation_system =
            wgpu::Dx12SwapchainKind::DxgiFromVisual;
        let instance = wgpu::Instance::new(instance_desc);
        let surface = instance.create_surface(Arc::clone(&host))?;
        let adapter = pick_adapter(&instance, backends, &surface)?;
        let info = adapter.get_info();
        tracing::info!(adapter = ?info);
        let adapter_name: Arc<str> =
            format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type).into();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;

        let caps = surface.get_capabilities(&adapter);
        // Not sRGB: ImGui's colors and alpha are meant to blend in gamma space,
        // and wgpu's GL backend presents sRGB targets through a blit whose sRGB
        // handling differs between drivers.
        let format = [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Rgba8Unorm,
        ]
        .into_iter()
        .find(|f| caps.formats.contains(f))
        .unwrap_or(caps.formats[0]);
        tracing::info!(?format, formats = ?caps.formats, alpha_modes = ?caps.alpha_modes, "surface");
        let size = host.inner_size();
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            // Only DirectComposition swapchains can be transparent.
            alpha_mode: if caps
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
            {
                wgpu::CompositeAlphaMode::PreMultiplied
            } else {
                caps.alpha_modes[0]
            },
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &surface_config);

        let mut imgui = Context::create();
        imgui.set_ini_filename(Some(options.ini_path.clone()))?;
        imgui.set_clipboard_backend(SystemClipboard::new()?);
        imgui.enable_multi_viewport();
        {
            let io = imgui.io_mut();
            let mut flags = io.config_flags();
            flags.insert(ConfigFlags::DOCKING_ENABLE);
            io.set_config_flags(flags);
            io.set_config_viewports_no_auto_merge(true);
            // Windows opt back in with a window class clearing NO_TASK_BAR_ICON.
            io.set_config_viewports_no_task_bar_icon(true);
            io.set_config_dpi_scale_fonts(true);
            io.set_config_dpi_scale_viewports(true);
            io.set_font_allow_user_scaling(true);
            // Enter submits a text field without leaving it, so the next line
            // can be typed straight away.
            io.set_config_input_text_enter_keep_active(true);
        }
        let dpi = host.scale_factor() as f32;
        imgui
            .style_mut()
            .set_tree_lines_mode(TreeLineMode::TO_NODES);
        imgui.style_mut().set_font_scale_dpi(dpi);
        // Tooltips show at once, without waiting for the mouse to rest.
        imgui
            .style_mut()
            .set_hover_flags_for_tooltip_mouse(TooltipHoveredFlags::ALLOW_WHEN_DISABLED);
        let style = StyleScale::new(&mut imgui, options.theme.clone(), dpi);
        fonts::load(&mut imgui);

        let transitions = NoTransitions::install()?;
        let mut platform = WinitPlatform::new(&mut imgui)?;
        platform.attach_window(Arc::clone(&host), HiDpiMode::Default, &mut imgui)?;
        platform.set_viewport_window_config(WinitViewportWindowConfig {
            transparent: true,
            ..Default::default()
        })?;
        platform.enable_viewports(&mut imgui)?;

        let init = WgpuInitInfo::new(device.clone(), queue.clone(), format)
            .with_instance(instance)
            .with_adapter(adapter)
            .with_viewport_surface_config(WgpuViewportSurfaceConfig::from(&surface_config));
        let mut renderer = WgpuRenderer::new(init, &mut imgui)?;
        renderer.set_gamma_mode(GammaMode::Auto);
        let route = WinitViewportRoute::attach(&mut imgui, &platform, renderer)
            .map_err(|failure| failure.into_parts().0)?;
        route.set_viewport_clear_color(wgpu::Color::TRANSPARENT)?;

        Ok(Self {
            host,
            host_surface: HostSurface {
                surface,
                config: surface_config,
                device,
                queue,
            },
            platform,
            route,
            native: NativeWindows::default(),
            _transitions: transitions,
            dpi,
            style,
            adapter: adapter_name,
            text_scale: 1.0,
            imgui,
        })
    }

    fn handle_window_event(&mut self, id: WindowId, event: WindowEvent) -> Result<(), Error> {
        let event: Event<()> = Event::WindowEvent {
            window_id: id,
            event,
        };
        self.platform.handle_event(&mut self.imgui, &event)?;
        Ok(())
    }

    /// Builds and presents one frame. Returns false once the UI asked to exit.
    fn frame(
        &mut self,
        event_loop: &ActiveEventLoop,
        build: &mut impl FnMut(&Ui, &mut Frame),
    ) -> Result<bool, Error> {
        self.platform.prepare_frame(&mut self.imgui, &self.host)?;
        self.imgui.style_mut().set_font_scale_main(self.text_scale);
        self.style
            .update(&mut self.imgui, self.dpi * self.text_scale);
        let mut declared = Frame::new(self.style.applied(), Arc::clone(&self.adapter));
        let frame = self.imgui.begin_frame();
        build(frame.ui(), &mut declared);
        self.platform.prepare_render(frame.ui(), &self.host)?;
        self.text_scale = declared.text_scale();
        if let Some(theme) = declared.take_saved_theme() {
            self.style.set_theme(theme);
        }

        let prepared = self.route.prepare(event_loop, frame)?;
        let main = self
            .route
            .complete_secondary(prepared, std::iter::empty())?;
        // After the viewport update, so windows created this frame are covered.
        self.native.reconcile(&self.platform, &declared)?;
        self.host_surface.present(&self.route, main)?;
        Ok(!declared.exit_requested())
    }

    fn shutdown(&mut self) -> Result<(), Error> {
        self.route.shutdown(&mut self.imgui)?;
        self.platform.disable_viewports(&mut self.imgui)?;
        self.platform.shutdown(&mut self.imgui)?;
        Ok(())
    }
}

/// The hidden host window's surface. Nothing is drawn in its viewport, but
/// its frame still has to be recorded to complete the viewport transaction.
struct HostSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl HostSurface {
    fn present(
        &self,
        route: &WinitViewportRoute,
        main: WgpuPreparedMainFrame<'_>,
    ) -> Result<(), Error> {
        let texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("host surface acquisition failed validation".into());
            }
        };
        let view = texture.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("host"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                ..Default::default()
            });
            route.record_main(
                main,
                &mut pass,
                FramebufferExtent::from_texture(&texture.texture),
            )?;
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(texture);
        Ok(())
    }
}

/// Hardware DX12, then hardware OpenGL (VMware's guest driver has no DX12),
/// then software DX12 (WARP).
fn pick_adapter(
    instance: &wgpu::Instance,
    backends: wgpu::Backends,
    surface: &wgpu::Surface,
) -> Result<wgpu::Adapter, Error> {
    let rank = |adapter: &wgpu::Adapter| {
        let info = adapter.get_info();
        (
            info.device_type == wgpu::DeviceType::Cpu,
            info.backend != wgpu::Backend::Dx12,
            info.device_type != wgpu::DeviceType::IntegratedGpu,
        )
    };
    pollster::block_on(instance.enumerate_adapters(backends))
        .into_iter()
        .filter(|adapter| adapter.is_surface_supported(surface))
        .min_by_key(rank)
        .ok_or_else(|| format!("no {backends:?} adapter can present to the window").into())
}
