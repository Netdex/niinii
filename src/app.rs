use ichiran::types::Root;
use imgui::*;
use serde::{Deserialize, Serialize};

use crate::{rikai::Rikai, support::{Env, ImStringDef, View}};

#[derive(Deserialize, Serialize)]
pub struct App {
    #[serde(with = "ImStringDef")]
    text: ImString,
    #[serde(with = "ImStringDef")]
    ichiran_path: ImString,
    root: Option<Root>,
}
impl Default for App {
    fn default() -> Self {
        Self {
            text: ImString::with_capacity(256),
            ichiran_path: ImString::with_capacity(256),
            root: None,
        }
    }
}

impl View for App {
    fn ui(&mut self, env: &mut Env, ui: &Ui) {
        let mut opened = false;
        ui.show_demo_window(&mut opened);
        Window::new(im_str!("niinii"))
            .size([300.0, 110.0], Condition::FirstUseEver)
            .build(ui, || {
                ui.input_text(im_str!("ichiran-cli"), &mut self.ichiran_path)
                    .resize_buffer(true)
                    .build();
                ui.separator();
                ui.text(im_str!("Text:"));
                ui.same_line();
                ui.input_text(im_str!(""), &mut self.text)
                    .resize_buffer(true)
                    .build();
                ui.same_line();
                if ui.button(im_str!("Go")) {
                    match ichiran::romanize(self.ichiran_path.to_str(), self.text.to_str()) {
                        Ok(root) => {
                            self.root.replace(root);
                            println!("{:?}", self.root);
                        }
                        Err(err) => eprintln!("{}", err),
                    }
                }
            });

        Window::new(im_str!("Rikai"))
            .size([300., 110.], Condition::FirstUseEver)
            .build(ui, || {
                if let Some(root) = &self.root {
                    Rikai::new(root).ui(env, ui);
                }
            });
    }
}
