use hudhook::hooks::{dx9::ImguiDx9Hooks, ImguiRenderLoop, ImguiRenderLoopFlags};
use hudhook::log::*;
use hudhook::reexports::*;
use hudhook::*;

use std::sync::Mutex;

use imgui::*;

use app::App;
use backend::{
    env::{Env, EnvFlags},
    renderer::Renderer,
};
use view::settings::Settings;

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
        hudhook::utils::simplelog();
        // env_logger::init();

        let settings = Settings::from_file();
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
        let mut run = true;
        app.ui(env, ui, &mut run);
        if !run {
            hudhook::lifecycle::eject();
        }
    }
}

impl Renderer for HudHookRenderer {}

hudhook::hudhook!(HudHookRenderer::new().into_hook::<ImguiDx9Hooks>());

// #[no_mangle]
// pub unsafe extern "stdcall" fn DllMain(hmodule: HINSTANCE, reason: u32, _: *mut std::ffi::c_void) {
//     match reason {
//         DLL_PROCESS_ATTACH => {
//             hudhook::lifecycle::global_state::set_module(hmodule);

//             trace!("DllMain()");
//             std::thread::spawn(move || {
//                 let hooks: Box<dyn hooks::Hooks> =
//                     { HudHookRenderer::new().into_hook::<ImguiDx9Hooks>() };
//                 hooks.hook();
//                 hudhook::lifecycle::global_state::set_hooks(hooks);
//             });
//         }
//         DLL_PROCESS_DETACH => {}
//         _ => {}
//     }
// }
