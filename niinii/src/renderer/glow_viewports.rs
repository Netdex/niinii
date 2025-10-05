//! As of imgui-rs 0.12.0, the glow viewports renderer doesn't seem to be
//! DPI-aware. Or at least, I can't figure out how it's support to work, since
//! WinitPlatform isn't part of the API like with the other renderers.

use std::{ffi::CString, num::NonZeroU32, time::Instant};

use glow::HasContext;
use glutin::{
    config::ConfigTemplateBuilder,
    context::{ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentGlContext},
    display::GetGlDisplay,
    prelude::GlDisplay,
    surface::{GlSurface, SurfaceAttributesBuilder, SwapInterval, WindowSurface},
};
use glutin_winit::DisplayBuilder;
use imgui::{ConfigFlags, ViewportFlags};
use imgui_winit_glow_renderer_viewports::Renderer as GlowViewportsRenderer;
use raw_window_handle::HasRawWindowHandle;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::WindowEvent,
    event_loop::{ControlFlow, EventLoop},
    platform::{run_on_demand::EventLoopExtRunOnDemand, windows::WindowBuilderExtWindows},
    window::{Window, WindowBuilder, WindowLevel},
};

use crate::{app::App, settings::Settings};

use super::{
    context::{Context, ContextFlags},
    Renderer,
};

pub struct GlowRenderer {
    event_loop: EventLoop<()>,
    window: Window,
    imgui: imgui::Context,
    ctx: Context,
    renderer: GlowViewportsRenderer,
    glow_context: glow::Context,
    glutin_context: glutin::context::PossiblyCurrentContext,
    glutin_surface: glutin::surface::Surface<glutin::surface::WindowSurface>,
}
impl GlowRenderer {
    pub fn new(settings: &Settings) -> Self {
        // The main window is useless except for driving the main renderer loop,
        // so just make it invisible and as small as possible.
        let window_builder = winit::window::WindowBuilder::new()
            .with_title("niinii")
            .with_decorations(false)
            .with_skip_taskbar(true)
            .with_resizable(false)
            .with_transparent(true)
            .with_position(PhysicalPosition::new(0, 0))
            .with_inner_size(PhysicalSize::new(1, 1));
        let event_loop = EventLoop::new().unwrap();

        let template_builder = ConfigTemplateBuilder::new();
        let (window, gl_config) = DisplayBuilder::new()
            .with_window_builder(Some(window_builder))
            .build(&event_loop, template_builder, |mut configs| {
                configs.next().unwrap()
            })
            .unwrap();

        let window = window.unwrap();
        let mut imgui = imgui::Context::create();
        Self::configure_imgui(&mut imgui, settings);
        imgui
            .io_mut()
            .config_flags
            .insert(ConfigFlags::VIEWPORTS_ENABLE);

        let mut ctx = Context::new(ContextFlags::empty());
        let dpi_scale = window.scale_factor();
        ctx.update_fonts(&mut imgui, dpi_scale);

        let context_attribs =
            ContextAttributesBuilder::new().build(Some(window.raw_window_handle()));
        let context = unsafe {
            gl_config
                .display()
                .create_context(&gl_config, &context_attribs)
                .unwrap()
        };

        let size = window.inner_size();
        let surface_attribs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            window.raw_window_handle(),
            NonZeroU32::new(size.width).unwrap(),
            NonZeroU32::new(size.height).unwrap(),
        );
        let surface = unsafe {
            gl_config
                .display()
                .create_window_surface(&gl_config, &surface_attribs)
                .unwrap()
        };

        let context = context.make_current(&surface).unwrap();
        surface
            .set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()))
            .unwrap();

        let glow = unsafe {
            glow::Context::from_loader_function(|name| {
                let name = CString::new(name).unwrap();
                context.display().get_proc_address(&name)
            })
        };

        let on_top = settings.on_top;
        let renderer = GlowViewportsRenderer::new(
            &mut imgui,
            &window,
            &glow,
            Some(Box::new(move |viewport: &imgui::Viewport| {
                WindowBuilder::new()
                    .with_resizable(true)
                    .with_transparent(true)
                    .with_skip_taskbar(viewport.flags.contains(ViewportFlags::NO_TASK_BAR_ICON))
                    .with_decorations(!viewport.flags.contains(ViewportFlags::NO_DECORATION))
                    .with_window_level(if on_top {
                        WindowLevel::AlwaysOnTop
                    } else {
                        WindowLevel::Normal
                    })
            })),
        )
        .unwrap();

        Self {
            event_loop,
            window,
            imgui,
            ctx,
            renderer,
            glow_context: glow,
            glutin_context: context,
            glutin_surface: surface,
        }
    }
}
impl Renderer for GlowRenderer {
    fn run(&mut self, app: &mut App) {
        use winit::event::Event;

        let GlowRenderer {
            event_loop,
            window,
            imgui,
            ctx,
            renderer,
            glow_context: glow,
            glutin_context: context,
            glutin_surface: surface,
        } = self;
        let mut last_frame = Instant::now();

        event_loop
            .run_on_demand(|event, window_target| {
                window_target.set_control_flow(ControlFlow::Wait);
                renderer.handle_event(imgui, window, &event);

                match event {
                    Event::NewEvents(_) => {
                        let now = Instant::now();
                        imgui.io_mut().update_delta_time(now - last_frame);
                        last_frame = now;
                    }
                    Event::WindowEvent {
                        window_id,
                        event: WindowEvent::CloseRequested,
                    } if window_id == window.id() => {
                        window_target.exit();
                    }
                    Event::WindowEvent {
                        window_id,
                        event: WindowEvent::Resized(new_size),
                    } if window_id == window.id() => {
                        let width = NonZeroU32::new(new_size.width);
                        let height = NonZeroU32::new(new_size.height);
                        if let Some((width, height)) = width.zip(height) {
                            surface.resize(context, width, height);
                        }
                    }
                    Event::AboutToWait => {
                        window.request_redraw();
                    }
                    Event::WindowEvent {
                        event: WindowEvent::RedrawRequested,
                        ..
                    } => {
                        let ui = imgui.frame();

                        let mut run = true;
                        app.ui(ctx, ui, &mut run);
                        if !run {
                            window_target.exit();
                        }

                        ui.end_frame_early();

                        renderer.prepare_render(imgui, window);

                        imgui.update_platform_windows();
                        renderer
                            .update_viewports(imgui, window_target, glow)
                            .unwrap();

                        let draw_data = imgui.render();

                        if let Err(e) = context.make_current(surface) {
                            // For some reason make_current randomly throws errors on windows.
                            // Until the reason for this is found, we just print it out instead of panicing.
                            eprintln!("Failed to make current: {e}");
                        }

                        unsafe {
                            glow.disable(glow::SCISSOR_TEST);
                            glow.clear(glow::COLOR_BUFFER_BIT);
                        }

                        renderer
                            .render(window, glow, draw_data)
                            .expect("Failed to render main viewport");

                        surface
                            .swap_buffers(context)
                            .expect("Failed to swap buffers");

                        renderer
                            .render_viewports(glow, imgui)
                            .expect("Failed to render viewports");
                    }
                    _ => {}
                }
            })
            .unwrap();
    }
}
