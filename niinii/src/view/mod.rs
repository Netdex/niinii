pub mod gloss;
pub mod index;
pub mod inject;
pub mod kanji;
pub mod mixins;
pub mod raw;
pub mod settings;
pub mod term;
pub mod translator;
pub mod tts;

pub trait View {
    fn ui(&mut self, ui: &imgui::Ui);
}
