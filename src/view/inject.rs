use imgui::*;

use super::settings::Settings;
use crate::backend::env::Env;

#[derive(Debug)]
pub struct InjectView;
impl InjectView {
    pub fn new() -> Self {
        Self
    }

    pub fn ui(&mut self, _env: &mut Env, ui: &Ui, settings: &mut Settings) {
        if CollapsingHeader::new("Remote Hook")
            .default_open(true)
            .build(ui)
        {
            ui.input_text("Process name", &mut settings.inject_proc_name)
                .build();
            if ui.button_with_size(
                "Inject (MAY CAUSE INSTABILITY)",
                [ui.window_content_region_width(), 0.0],
            ) {
                Self::inject_by_process_name(&settings.inject_proc_name);
            }
        }
    }

    fn inject_by_process_name(name: impl AsRef<str>) {
        let proc_name = name.as_ref();
        let dll_path = std::env::current_exe()
            .ok()
            .and_then(|x| x.parent().map(ToOwned::to_owned))
            .map(|x| x.join("libniinii.dll"))
            .and_then(|x| x.canonicalize().ok())
            .unwrap();
        log::info!(
            "injecting dll {} into process {}",
            dll_path.display(),
            proc_name
        );
        // hudhook::inject::Process::by_name(proc_name)
        //     .unwrap()
        //     .inject(dll_path)
        //     .unwrap();
    }
}
