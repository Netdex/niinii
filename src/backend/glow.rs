use glow::HasContext;
use glutin::{
    event_loop::{ControlFlow, EventLoop},
    platform::run_return::EventLoopExtRunReturn,
};
use imgui::Ui;
use std::time::Instant;

use crate::common::{imgui_init, Env};

pub type Window = glutin::WindowedContext<glutin::PossiblyCurrent>;

pub fn main_loop<F>(window: winit::window::WindowBuilder, mut run_ui: F)
where
    F: FnMut(&mut bool, &mut Env, &mut Ui),
{
    // Common setup for creating a winit window and imgui context, not specifc
    // to this renderer at all except that glutin is used to create the window
    // since it will give us access to a GL context
    let (mut event_loop, window) = create_window(window);
    let (mut platform, mut imgui, mut env) = imgui_init(window.window());

    // OpenGL context from glow
    let gl = glow_context(&window);

    // OpenGL renderer from this crate
    let mut ig_renderer = imgui_glow_renderer::AutoRenderer::initialize(gl, &mut imgui)
        .expect("failed to create renderer");

    let mut last_frame = Instant::now();

    // Standard winit event loop
    event_loop.run_return(move |event, _, control_flow| {
        // *control_flow = glutin::event_loop::ControlFlow::Wait;
        match event {
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
                // The renderer assumes you'll be clearing the buffer yourself
                unsafe { ig_renderer.gl_context().clear(glow::COLOR_BUFFER_BIT) };

                let mut ui = imgui.frame();
                let mut run = true;
                run_ui(&mut run, &mut env, &mut ui);
                if !run {
                    *control_flow = ControlFlow::Exit;
                }

                platform.prepare_render(&ui, window.window());
                let draw_data = ui.render();

                // This is the only extra render step to add
                ig_renderer
                    .render(draw_data)
                    .expect("error rendering imgui");

                window.swap_buffers().unwrap();
            }
            glutin::event::Event::WindowEvent {
                event: glutin::event::WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = glutin::event_loop::ControlFlow::Exit;
            }
            event => {
                platform.handle_event(imgui.io_mut(), window.window(), &event);
            }
        }
    });
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
