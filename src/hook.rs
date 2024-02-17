use hudhook::hooks::{dx9::ImguiDx9Hooks, ImguiRenderLoop};
use hudhook::*;

use std::sync::Mutex;

use imgui::*;

use crate::app::App;
use crate::renderer::{
    context::{Context, ContextFlags},
    Renderer,
};
use crate::settings::Settings;

struct State {
    ctx: Context,
    app: App,
}

struct HudHookRenderer {
    state: Mutex<State>,
}

impl HudHookRenderer {
    fn new() -> Self {
        hudhook::alloc_console().unwrap();
        hudhook::enable_console_colors();

        // TODO: need to instantiate a tokio runtime here
        let settings = Settings::from_file();
        let env = Context::new(ContextFlags::SHARED_RENDER_CONTEXT);
        // let app = App::new(settings).await;
        Self {
            state: Mutex::new(State { ctx: env, app }),
        }
    }
}

impl ImguiRenderLoop for HudHookRenderer {
    fn initialize(&mut self, ctx: &mut imgui::Context) {
        let mut state = self.state.lock().unwrap();
        let State { ctx: env, app } = &mut *state;

        Self::configure_imgui(ctx, app.settings());
        env.update_fonts(ctx, 1.0);
    }

    fn should_block_messages(&self, io: &Io) -> bool {
        io.want_capture_mouse
    }

    fn render(&mut self, ui: &mut Ui) {
        let mut state = self.state.lock().unwrap();
        let State { ctx: env, app } = &mut *state;
        let mut run = true;
        app.ui(env, ui, &mut run);
        if !run {
            hudhook::eject();
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
