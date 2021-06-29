mod config;
mod rikai;

use eframe::{egui::{self, FontDefinitions, FontFamily, TextStyle, Visuals}, epi};
use ichiran::types::Root;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, fs};

use crate::config::Config;
use crate::rikai::Rikai;

pub trait View {
    fn ui(&mut self, ui: &mut egui::Ui);
}

#[derive(Deserialize, Serialize)]
#[serde(default)]
pub struct App {
    config: Config,
    text: String,
    ichiran_root: Option<Root>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            text: "".to_owned(),
            config: Config::default(),
            ichiran_root: None,
        }
    }
}

impl epi::App for App {
    fn name(&self) -> &str {
        "niinii"
    }

    fn setup(
        &mut self,
        ctx: &egui::CtxRef,
        _frame: &mut epi::Frame<'_>,
        storage: Option<&dyn epi::Storage>,
    ) {
        ctx.set_visuals(Visuals::light());
        ctx.set_fonts(get_font_definitions());
        if let Some(storage) = storage {
            *self = epi::get_value(storage, epi::APP_KEY).unwrap_or_default()
        }
    }

    fn save(&mut self, storage: &mut dyn epi::Storage) {
        epi::set_value(storage, epi::APP_KEY, self);
    }

    fn update(&mut self, ctx: &egui::CtxRef, frame: &mut epi::Frame<'_>) {
        let Self {
            config,
            text,
            ichiran_root,
        } = self;

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                egui::menu::menu(ui, "File", |ui| {
                    if ui.button("Quit").clicked() {
                        frame.quit();
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label("Text: ");
                ui.text_edit_singleline(text);
                if ui.button("Go").clicked() {
                    match ichiran::romanize(&config.ichiran_path, &text) {
                        Ok(root) => {
                            ichiran_root.replace(root);
                        }
                        Err(err) => eprintln!("{}", err),
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("egui template");
            ui.hyperlink("https://github.com/emilk/egui_template");
            ui.add(egui::github_link_file!(
                "https://github.com/emilk/egui_template/blob/master/",
                "Source code."
            ));

            if let Some(root) = ichiran_root {
                Rikai::new(root).ui(ui);
            }
            egui::warn_if_debug_build(ui);
        });

        egui::Window::new("Settings").show(ctx, |ui| {
            ctx.settings_ui(ui);
        });

        egui::Window::new("Configuration").show(ctx, |ui| {
            config.ui(ui);
        });
    }
}

fn get_font_definitions() -> FontDefinitions {
    let mut font = FontDefinitions::default();
    // font.font_data.insert(
    //     "Noto Sans JP Medium".into(),
    //     Cow::Owned(fs::read("res/NotoSansJP-Medium.otf").unwrap()),
    // );
    font.font_data.insert(
        "Sarasa UI J".into(),
        Cow::Owned(fs::read("res/sarasa-ui-j-regular.ttf").unwrap()),
    );
    font.font_data.insert(
        "Iosevka Regular".into(),
        Cow::Owned(fs::read("res/iosevka-regular.ttf").unwrap()),
    );
    // font.fonts_for_family
    //     .get_mut(&egui::FontFamily::Proportional)
    //     .unwrap()
    //     .insert(0, "Noto Sans JP Medium".into());
    font.fonts_for_family
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, "Sarasa UI J".into());
    font.fonts_for_family
        .get_mut(&egui::FontFamily::Monospace)
        .unwrap()
        .insert(0, "Iosevka Regular".into());
    font.family_and_size
        .insert(TextStyle::Small, (FontFamily::Proportional, 12.0));
    font.family_and_size
        .insert(TextStyle::Body, (FontFamily::Proportional, 16.0));
    font.family_and_size
        .insert(TextStyle::Button, (FontFamily::Proportional, 16.0));
    // font.family_and_size
    //     .insert(TextStyle::Heading, (FontFamily::Proportional, 20.0));
    font.family_and_size
        .insert(TextStyle::Heading, (FontFamily::Proportional, 24.0));
    font.family_and_size
        .insert(TextStyle::Monospace, (FontFamily::Monospace, 14.0));
    font
}
