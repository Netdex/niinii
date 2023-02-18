use std::time::Instant;

use glow::HasContext;
use glutin::{
    event_loop::{ControlFlow, EventLoop},
    platform::run_return::EventLoopExtRunReturn,
};
use imgui_glow_renderer::AutoRenderer;
use imgui_winit_support::WinitPlatform;

use super::env::{Env, EnvFlags};
use super::renderer::Renderer;
use crate::{app::App, view::settings::Settings};

pub type Window = glutin::WindowedContext<glutin::PossiblyCurrent>;

pub struct GlowRenderer {
    event_loop: EventLoop<()>,
    window: Window,
    platform: WinitPlatform,
    imgui: imgui::Context,
    env: Env,
    renderer: AutoRenderer,
}
impl GlowRenderer {
    pub fn new(settings: &Settings) -> Self {
        let (event_loop, window) = create_window(Self::create_window_builder(settings));
        let mut imgui = imgui::Context::create();
        Self::configure_imgui(&mut imgui, settings);
        let platform = Self::create_platform(&mut imgui, window.window());
        let mut env = Env::new(EnvFlags::empty());
        env.update_fonts(&mut imgui, platform.hidpi_factor());

        let gl = glow_context(&window);

        let renderer = imgui_glow_renderer::AutoRenderer::initialize(gl, &mut imgui)
            .expect("failed to create renderer");

        Self {
            event_loop,
            window,
            platform,
            imgui,
            env,
            renderer,
        }
    }
}
impl Renderer for GlowRenderer {
    fn main_loop(&mut self, app: &mut App) {
        let GlowRenderer {
            event_loop,
            window,
            platform,
            imgui,
            env,
            renderer,
        } = self;
        let mut last_frame = Instant::now();

        event_loop.run_return(|event, _, control_flow| match event {
            glutin::event::Event::NewEvents(_) => {
                let now = Instant::now();
                imgui
                    .io_mut()
                    .update_delta_time(now.duration_since(last_frame));
                last_frame = now;
            }
            glutin::event::Event::MainEventsCleared => {
                platform
                    .prepare_frame(imgui.io_mut(), window.window())
                    .unwrap();
                window.window().request_redraw();
            }
            glutin::event::Event::RedrawRequested(_) => {
                unsafe { renderer.gl_context().clear(glow::COLOR_BUFFER_BIT) };

                let mut ui = imgui.frame();
                let mut run = true;
                app.ui(env, &mut ui, &mut run);
                if !run {
                    *control_flow = ControlFlow::Exit;
                }

                platform.prepare_render(&ui, window.window());
                let draw_data = imgui.render();
                renderer.render(draw_data).unwrap();

                window.swap_buffers().unwrap();
            }
            glutin::event::Event::WindowEvent {
                event: glutin::event::WindowEvent::CloseRequested,
                ..
            } => *control_flow = glutin::event_loop::ControlFlow::Exit,
            event => {
                platform.handle_event(imgui.io_mut(), window.window(), &event);
            }
        });
    }
}

fn create_window(window: winit::window::WindowBuilder) -> (EventLoop<()>, Window) {
    let event_loop = EventLoop::new();
    let window = glutin::ContextBuilder::new()
        .with_vsync(true)
        .build_windowed(window, &event_loop)
        .expect("could not create window");
    let window = unsafe {
        window
            .make_current()
            .expect("could not make window context current")
    };
    (event_loop, window)
}

fn glow_context(window: &Window) -> glow::Context {
    unsafe { glow::Context::from_loader_function(|s| window.get_proc_address(s).cast()) }
}
