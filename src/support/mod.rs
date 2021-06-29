use glium::{
    glutin::{
        self,
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoop},
        platform::run_return::EventLoopExtRunReturn,
        window::WindowBuilder,
    },
    Display, Surface,
};
use imgui::{Context, FontConfig, FontGlyphRanges, FontId, FontSource, ImString, Ui};
use imgui_glium_renderer::Renderer;
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::{collections::HashMap, fs};

mod clipboard;

pub trait View {
    fn ui(&mut self, env: &mut Env, ui: &Ui);
}

#[derive(Deserialize, Serialize)]
#[serde(remote = "ImString")]
pub struct ImStringDef(#[serde(getter = "ImString::to_string")] String);
impl From<ImStringDef> for ImString {
    fn from(str: ImStringDef) -> Self {
        ImString::new(str.0)
    }
}

pub struct System {
    pub event_loop: EventLoop<()>,
    pub display: glium::Display,
    pub imgui: Context,
    pub platform: WinitPlatform,
    pub renderer: Renderer,
    pub env: Env,
}

#[derive(Default)]
pub struct Env {
    pub fonts: HashMap<&'static str, FontId>,
}

pub fn init(title: &str) -> System {
    let title = match Path::new(&title).file_name() {
        Some(file_name) => file_name.to_str().unwrap(),
        None => title,
    };
    let event_loop = EventLoop::new();
    let context = glutin::ContextBuilder::new().with_vsync(true);
    let builder = WindowBuilder::new()
        .with_title(title.to_owned())
        .with_inner_size(glutin::dpi::LogicalSize::new(1024f64, 768f64));
    let display =
        Display::new(builder, context, &event_loop).expect("Failed to initialize display");

    let mut imgui = Context::create();
    imgui.set_ini_filename(Some(PathBuf::from("imgui.ini")));

    if let Some(backend) = clipboard::init() {
        imgui.set_clipboard_backend(Box::new(backend));
    } else {
        eprintln!("Failed to initialize clipboard");
    }

    let mut platform = WinitPlatform::init(&mut imgui);
    {
        let gl_window = display.gl_window();
        let window = gl_window.window();
        platform.attach_window(imgui.io_mut(), window, HiDpiMode::Rounded);
    }

    let mut env = Env::default();
    let hidpi_factor = platform.hidpi_factor();

    let mut add_font = |name: &'static str, path: &str, size_pt: f64, config: FontConfig| {
        env.fonts.insert(
            name,
            imgui.fonts().add_font(&[FontSource::TtfData {
                data: &fs::read(path).unwrap(),
                size_pixels: (size_pt * hidpi_factor) as f32,
                config: Some(FontConfig {
                    name: Some(name.to_owned()),
                    ..config
                }),
            }]),
        );
    };
    add_font(
        "Sarasa Mono J 13pt",
        "res/sarasa-mono-j-regular.ttf",
        13.0,
        FontConfig {
            rasterizer_multiply: 1.75,
            glyph_ranges: FontGlyphRanges::japanese(),
            ..Default::default()
        },
    );
    add_font(
        "Sarasa Mono J 40pt",
        "res/sarasa-mono-j-regular.ttf",
        40.0,
        FontConfig {
            rasterizer_multiply: 1.75,
            glyph_ranges: FontGlyphRanges::japanese(),
            ..Default::default()
        },
    );
    imgui.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;

    let renderer = Renderer::init(&mut imgui, &display).expect("Failed to initialize renderer");

    System {
        event_loop,
        display,
        imgui,
        platform,
        renderer,
        env,
    }
}

impl System {
    pub fn main_loop<F>(self, mut run_ui: F)
    where
        F: FnMut(&mut bool, &mut Env, &mut Ui),
    {
        let System {
            mut event_loop,
            display,
            mut imgui,
            mut platform,
            mut renderer,
            mut env,
            ..
        } = self;
        let mut last_frame = Instant::now();

        event_loop.run_return(move |event, _, control_flow| match event {
            Event::NewEvents(_) => {
                let now = Instant::now();
                imgui.io_mut().update_delta_time(now - last_frame);
                last_frame = now;
            }
            Event::MainEventsCleared => {
                let gl_window = display.gl_window();
                platform
                    .prepare_frame(imgui.io_mut(), gl_window.window())
                    .expect("Failed to prepare frame");
                gl_window.window().request_redraw();
            }
            Event::RedrawRequested(_) => {
                let mut ui = imgui.frame();

                let mut run = true;
                run_ui(&mut run, &mut env, &mut ui);
                if !run {
                    *control_flow = ControlFlow::Exit;
                }

                let gl_window = display.gl_window();
                let mut target = display.draw();
                target.clear_color_srgb(1.0, 1.0, 1.0, 1.0);
                platform.prepare_render(&ui, gl_window.window());
                let draw_data = ui.render();
                renderer
                    .render(&mut target, draw_data)
                    .expect("Rendering failed");
                target.finish().expect("Failed to swap buffers");
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            event => {
                let gl_window = display.gl_window();
                platform.handle_event(imgui.io_mut(), gl_window.window(), &event);
            }
        })
    }
}
