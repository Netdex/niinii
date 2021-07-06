use ichiran::Ichiran;
use ichiran::IchiranError;
use imgui::*;
use serde::{Deserialize, Serialize};

use crate::{
    support::{Env, ImStringDef},
    view::{RikaiView, SettingsView},
};

const ERROR_MODAL_TITLE: &ImStr = im_str!("Error");

#[derive(Default, Deserialize, Serialize)]
pub struct App {
    #[serde(with = "ImStringDef")]
    text: ImString,

    settings: SettingsView,
    rikai: RikaiView,

    show_imgui_demo: bool,
    show_settings: bool,

    #[serde(skip)]
    last_clipboard: ImString,
    #[serde(skip)]
    last_err: Option<IchiranError>,
}
impl App {
    fn open_error_modal(&mut self, ui: &Ui, err: IchiranError) {
        self.last_err = Some(err);
        ui.open_popup(ERROR_MODAL_TITLE);
    }
    fn update(&mut self, ui: &Ui) {
        let ichiran = Ichiran::new(self.settings.ichiran_path.to_str());
        match ichiran.romanize(self.text.to_str()) {
            Ok(root) => match ichiran.jmdict_data() {
                Ok(jmdict_data) => {
                    self.rikai = RikaiView::new(root, jmdict_data);
                }
                Err(err) => {
                    self.open_error_modal(ui, err);
                }
            },
            Err(err) => {
                self.open_error_modal(ui, err);
            }
        }
    }
    fn show_main_menu(&mut self, env: &mut Env, ui: &Ui) {
        if let Some(_menu_bar) = ui.begin_main_menu_bar() {
            if let Some(_menu) = ui.begin_menu(im_str!("File")) {}
            if let Some(_menu) = ui.begin_menu(im_str!("Edit")) {
                if MenuItem::new(im_str!("Settings")).build(ui) {
                    self.show_settings = true;
                }
            }
            if let Some(_menu) = ui.begin_menu(im_str!("View")) {
                if MenuItem::new(im_str!("ImGui Demo")).build(ui) {
                    self.show_imgui_demo = true;
                }
            }
        }
    }
    fn show_error_modal(&mut self, env: &mut Env, ui: &Ui) {
        if let Some(err) = &self.last_err {
            PopupModal::new(ERROR_MODAL_TITLE)
                .always_auto_resize(true)
                .build(ui, || {
                    let _wrap_token = ui.push_text_wrap_pos_with_pos(300.0);
                    ui.text(&im_str!("{}", err));
                    ui.separator();
                    if ui.button_with_size(im_str!("OK"), [120.0, 0.0]) {
                        ui.close_current_popup();
                    }
                });
        }
    }

    pub fn ui(&mut self, env: &mut Env, ui: &Ui) {
        self.show_main_menu(env, ui);

        Window::new(im_str!("niinii"))
            .size([300.0, 110.0], Condition::FirstUseEver)
            .build(ui, || {
                if ui
                    .input_text_multiline(im_str!("Text"), &mut self.text, [0.0, 50.0])
                    .resize_buffer(true)
                    .enter_returns_true(true)
                    .build()
                {
                    self.update(ui);
                }
                if ui.button_with_size(im_str!("Go"), [120.0, 0.0]) {
                    self.update(ui);
                }
                if let Some(clipboard) = ui.clipboard_text() {
                    if clipboard != self.last_clipboard {
                        self.text = clipboard.clone();
                        self.last_clipboard = clipboard;
                        self.update(ui);
                    }
                }
                self.show_error_modal(env, ui);
            });

        Window::new(im_str!("Rikai"))
            .size([300., 110.], Condition::FirstUseEver)
            .build(ui, || {
                self.rikai.ui(env, ui, &self.settings);
            });

        if self.show_imgui_demo {
            ui.show_demo_window(&mut self.show_imgui_demo);
        }

        if self.show_settings {
            Window::new(im_str!("Settings"))
                .size([300.0, 110.0], Condition::FirstUseEver)
                .always_auto_resize(true)
                .resizable(false)
                .build(ui, || {
                    self.settings.ui(env, ui);
                    ui.separator();
                    if ui.button_with_size(im_str!("OK"), [120.0, 0.0]) {
                        self.show_settings = false;
                    }
                });
        }
    }
}
