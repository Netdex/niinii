use imgui::{SliderFlags, StyleColor, TableColumnSetup, TableFlags, TreeNodeFlags};

use crate::{
    settings::Settings,
    translator::realtime::{RealtimeTranslation, RealtimeTranslator},
    view::{
        mixins::{checkbox_option_with_default, combo_enum, stroke_text_with_highlight},
        View,
    },
};

pub struct ViewRealtimeTranslator<'a>(pub &'a RealtimeTranslator, pub &'a mut Settings);
impl View for ViewRealtimeTranslator<'_> {
    fn ui(&mut self, ui: &imgui::Ui) {
        let Self(translator, settings) = self;
        let realtime = &mut settings.realtime;
        ui.menu_bar(|| {
            ui.menu("Session", || {
                if ui.menu_item("Reset") {
                    translator.session_mut().take();
                }
            });
        });
        if ui.collapsing_header("Session", TreeNodeFlags::DEFAULT_OPEN) {
            if let Some(session) = translator.session().get() {
                let info = session.info();
                let mut session_id = info.id.to_string();
                ui.input_text("Session ID", &mut session_id)
                    .read_only(true)
                    .build();
            } else {
                ui.text_disabled("No active session");
            }
        }
        if ui.collapsing_header("Parameters", TreeNodeFlags::DEFAULT_OPEN) {
            if let Some(_token) = ui.begin_table("##", 2) {
                ui.table_next_column();
                checkbox_option_with_default(
                    ui,
                    &mut realtime.temperature,
                    1.0,
                    |ui, temperature| {
                        ui.set_next_item_width(ui.current_font_size() * -8.0);
                        ui.slider_config("Temperature", 0.0f32, 2.0f32)
                            .display_format("%.2f")
                            .flags(SliderFlags::ALWAYS_CLAMP)
                            .build(temperature);
                    },
                );
                ui.table_next_column();
                ui.set_next_item_width(ui.current_font_size() * -8.0);
                combo_enum(ui, "Model", &mut realtime.model);
            }
        }
        ui.child_window("context_window").build(|| {
            if let Some(_t) = ui.begin_table_header_with_flags(
                "context",
                [
                    TableColumnSetup::new(""),
                    TableColumnSetup::new(""),
                    TableColumnSetup::new("Role"),
                    TableColumnSetup::new("Message"),
                ],
                TableFlags::SIZING_STRETCH_PROP,
            ) {
                ui.table_next_column();
                ui.table_next_column();
                ui.table_next_column();
                ui.disabled(true, || {
                    ui.set_next_item_width(ui.current_font_size() * 6.0);
                    ui.text("System");
                });
                ui.table_next_column();
                ui.input_text_multiline(
                    "##",
                    &mut realtime.system_prompt,
                    [ui.content_region_avail()[0], 200.0],
                )
                .build();
            }
        });
    }
}

pub struct ViewRealtimeTranslation<'a>(pub &'a RealtimeTranslation);
impl View for ViewRealtimeTranslation<'_> {
    fn ui(&mut self, ui: &imgui::Ui) {
        let _wrap_token = ui.push_text_wrap_pos_with_pos(0.0);
        ui.text(""); // anchor for line wrapping
        ui.same_line();
        let inner = self.0.inner.blocking_lock();
        let draw_list = ui.get_window_draw_list();
        stroke_text_with_highlight(
            ui,
            &draw_list,
            "[ChatGPT]",
            1.0,
            Some(StyleColor::NavHighlight),
        );
        ui.same_line();
        stroke_text_with_highlight(
            ui,
            &draw_list,
            &inner.text,
            1.0,
            Some(StyleColor::TextSelectedBg),
        );
    }
}

pub struct ViewRealtimeTranslationUsage<'a>(pub &'a RealtimeTranslation);
impl View for ViewRealtimeTranslationUsage<'_> {
    fn ui(&mut self, _ui: &imgui::Ui) {}
}
