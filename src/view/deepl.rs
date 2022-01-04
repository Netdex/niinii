use imgui::*;

pub struct DeepLView<'a> {
    deepl_text: &'a Option<String>,
    deepl_usage: &'a Option<deepl_api::UsageInformation>,
}
impl<'a> DeepLView<'a> {
    pub fn new(
        deepl_text: &'a Option<String>,
        deepl_usage: &'a Option<deepl_api::UsageInformation>,
    ) -> Self {
        DeepLView {
            deepl_text,
            deepl_usage,
        }
    }
    pub fn ui(&self, ui: &Ui) {
        let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
        if let Some(deepl_usage) = self.deepl_usage {
            let fraction = deepl_usage.character_count as f32 / deepl_usage.character_limit as f32;
            ProgressBar::new(fraction)
                .overlay_text(format!(
                    "DeepL API usage: {}/{} ({:.2}%)",
                    deepl_usage.character_count,
                    deepl_usage.character_limit,
                    fraction * 100.0
                ))
                .build(ui);
        }
        if let Some(deepl_text) = self.deepl_text {
            ui.text(deepl_text);
        }
    }
}
