use imgui::*;

use crate::settings::Settings;

#[derive(Debug, Default)]
pub struct InjectView {
    pub open: bool,
}
impl InjectView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show_menu_item(&mut self, ui: &Ui) {
        if ui.menu_item("Inject") {
            self.open = true;
        }
    }

    pub fn ui(&mut self, ui: &Ui, settings: &mut Settings) {
        if !self.open {
            return;
        }
        let Some(_window) = ui
            .window("Inject")
            .always_auto_resize(true)
            .opened(&mut self.open)
            .begin()
        else {
            return;
        };
        if CollapsingHeader::new("Remote Hook")
            .default_open(true)
            .build(ui)
        {
            ui.input_text("Process name", &mut settings.inject_proc_name)
                .build();
            let width = ui.window_content_region_max()[0] - ui.window_content_region_min()[0];
            if ui.button_with_size("Inject (MAY CAUSE INSTABILITY)", [width, 0.0]) {
                Self::inject_by_process_name(&settings.inject_proc_name);
            }
        }
    }

    #[cfg(feature = "hook")]
    fn inject_by_process_name(name: impl AsRef<str>) {
        let proc_name = name.as_ref();
        let dll_path = std::env::current_exe()
            .ok()
            .and_then(|x| x.parent().map(ToOwned::to_owned))
            .map(|x| x.join("libniinii.dll"))
            .and_then(|x| x.canonicalize().ok())
            .unwrap();
        hudhook::inject::Process::by_name(proc_name)
            .unwrap()
            .inject(dll_path)
            .unwrap();
    }

    #[cfg(not(feature = "hook"))]
    fn inject_by_process_name(_name: impl AsRef<str>) {}
}
