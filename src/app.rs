use ichiran::types::Root;
use ichiran::IchiranError;
use imgui::*;
use serde::{Deserialize, Serialize};

use crate::view::RawView;
use crate::{
    support::{Env, ImStringDef},
    view::{RikaiView, SettingsView},
};

const ERROR_MODAL_TITLE: &ImStr = im_str!("Error");

#[derive(Default, Deserialize, Serialize)]
pub struct App {
    #[serde(with = "ImStringDef")]
    text: ImString,

    root: Root,
    settings: SettingsView,
    rikai: RikaiView,

    show_imgui_demo: bool,
    show_settings: bool,
    show_raw: bool,

    #[serde(skip)]
    last_err: Option<IchiranError>,
}
impl App {
    fn update(&mut self, ui: &Ui) {
        match ichiran::romanize(self.settings.ichiran_path(), self.text.to_str()) {
            Ok(root) => {
                self.rikai = RikaiView::new();
                self.root = root;
            }
            Err(err) => {
                self.last_err = Some(err);
                ui.open_popup(ERROR_MODAL_TITLE);
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
                if MenuItem::new(im_str!("Raw")).build(ui) {
                    self.show_raw = true;
                }
            }
        }
    }
    fn show_error_modal(&mut self, env: &mut Env, ui: &Ui) {
        if let Some(err) = &self.last_err {
            PopupModal::new(ERROR_MODAL_TITLE)
                .always_auto_resize(true)
                .build(ui, || {
                    ui.text(format!("{}", err));
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
                    .input_text(im_str!("Text"), &mut self.text)
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
                    if clipboard != self.text {
                        self.text = clipboard;
                        self.update(ui);
                    }
                }
                self.show_error_modal(env, ui);
            });

        Window::new(im_str!("Rikai"))
            .size([300., 110.], Condition::FirstUseEver)
            .build(ui, || {
                self.rikai.ui(env, ui, &self.root);
            });

        if self.show_imgui_demo {
            ui.show_demo_window(&mut self.show_imgui_demo);
        }

        if self.show_settings {
            let mut opened = true;
            Window::new(im_str!("Settings"))
                .size([300.0, 110.0], Condition::FirstUseEver)
                .opened(&mut opened)
                .build(ui, || {
                    self.settings.ui(env, ui);
                });
            self.show_settings = opened;
        }
        if self.show_raw {
            let mut opened = true;
            Window::new(im_str!("Raw"))
                .size([300., 110.], Condition::FirstUseEver)
                .opened(&mut opened)
                .build(ui, || {
                    RawView::new(&self.root).ui(env, ui);
                });
            self.show_raw = opened;
        }
    }
}
