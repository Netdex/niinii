use std::{ffi::CString, num::NonZeroU32, time::Instant};

use glow::HasContext;
use glutin::{
    config::ConfigTemplateBuilder,
    context::ContextAttributesBuilder,
    display::GetGlDisplay,
    prelude::{
        GlDisplay, NotCurrentGlContextSurfaceAccessor, PossiblyCurrentContextGlSurfaceAccessor,
    },
    surface::{GlSurface, SurfaceAttributesBuilder, WindowSurface},
};
use glutin_winit::DisplayBuilder;
use imgui::ConfigFlags;
use imgui_winit_glow_renderer_viewports::Renderer as GlowViewportsRenderer;
use raw_window_handle::HasRawWindowHandle;
use winit::{
    event::WindowEvent, event_loop::EventLoop, platform::run_return::EventLoopExtRunReturn,
    window::Window,
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
        let window_builder = Self::create_window_builder(settings);
        let event_loop = EventLoop::new();

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
        // let dpi_scale = imgui.main_viewport().dpi_scale.into();
        let dpi_scale = 2.0;
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

        let glow = unsafe {
            glow::Context::from_loader_function(|name| {
                let name = CString::new(name).unwrap();
                context.display().get_proc_address(&name)
            })
        };

        let renderer = GlowViewportsRenderer::new(&mut imgui, &window, &glow).unwrap();

        // let dpi = match settings.use_force_dpi {
        //     true => Some(settings.force_dpi),
        //     false => None,
        // };
        // let platform = Self::create_platform(&mut imgui, window.window(), dpi);

        Self {
            event_loop,
            window,
            // platform,
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
    fn main_loop(&mut self, app: &mut App) {
        let GlowRenderer {
            event_loop,
            window,
            // platform,
            imgui,
            ctx,
            renderer,
            glow_context: glow,
            glutin_context: context,
            glutin_surface: surface,
        } = self;
        let mut last_frame = Instant::now();
        event_loop.run_return(move |event, window_target, control_flow| {
            control_flow.set_poll();

            renderer.handle_event(imgui, window, &event);

            match event {
                winit::event::Event::NewEvents(_) => {
                    let now = Instant::now();
                    imgui.io_mut().update_delta_time(now - last_frame);
                    last_frame = now;
                }
                winit::event::Event::WindowEvent {
                    window_id,
                    event: WindowEvent::CloseRequested,
                } if window_id == window.id() => {
                    control_flow.set_exit();
                }
                winit::event::Event::WindowEvent {
                    window_id,
                    event: WindowEvent::Resized(new_size),
                } if window_id == window.id() => {
                    surface.resize(
                        context,
                        NonZeroU32::new(new_size.width).unwrap(),
                        NonZeroU32::new(new_size.height).unwrap(),
                    );
                }
                winit::event::Event::MainEventsCleared => {
                    window.request_redraw();
                }
                winit::event::Event::RedrawRequested(_) => {
                    let ui = imgui.frame();

                    let mut run = true;
                    app.ui(ctx, ui, &mut run);
                    if !run {
                        *control_flow = winit::event_loop::ControlFlow::Exit;
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
        });
    }
}
