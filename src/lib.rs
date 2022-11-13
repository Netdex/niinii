#[macro_use]
pub mod util;

pub mod app;
pub mod backend;
pub mod clipboard;
pub mod gloss;
pub mod translation;
pub mod view;

use std::sync::Mutex;

use app::App;
use backend::{
    env::{Env, EnvFlags},
    renderer::Renderer,
};
use hudhook::hooks::{dx9::ImguiDx9Hooks, ImguiRenderLoop, ImguiRenderLoopFlags};
use imgui::*;
use view::settings::SettingsView;

const STATE_PATH: &str = "niinii.toml";

struct State {
    env: Env,
    app: App,
}

struct HudHookRenderer {
    state: Mutex<State>,
}

impl HudHookRenderer {
    fn new() -> Self {
        hudhook::utils::alloc_console();
        env_logger::init();

        let settings: SettingsView = std::fs::read_to_string(STATE_PATH)
            .ok()
            .and_then(|x| toml::from_str(&x).ok())
            .unwrap_or_default();
        let env = Env::new(EnvFlags::SHARED_RENDER_CONTEXT);
        let app = App::new(settings);
        Self {
            state: Mutex::new(State { env, app }),
        }
    }
}

impl ImguiRenderLoop for HudHookRenderer {
    fn initialize(&mut self, ctx: &mut Context) {
        let mut state = self.state.lock().unwrap();
        let State { env, app } = &mut *state;

        Self::configure_imgui(ctx, app.settings());
        env.update_fonts(ctx, 1.0);
    }

    fn should_block_messages(&self, io: &Io) -> bool {
        io.want_capture_mouse
    }

    fn render(&mut self, ui: &mut Ui, _flags: &ImguiRenderLoopFlags) {
        let mut state = self.state.lock().unwrap();
        let State { env, app } = &mut *state;
        let mut dummy = true;
        app.ui(env, ui, &mut dummy);
    }
}

impl Renderer for HudHookRenderer {}

hudhook::hudhook!(HudHookRenderer::new().into_hook::<ImguiDx9Hooks>());
