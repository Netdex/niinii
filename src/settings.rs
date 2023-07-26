use num_derive::FromPrimitive;
use openai_chat::chat::Model;
use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, IntoStaticStr};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, FromPrimitive, IntoStaticStr, EnumIter)]
pub enum RendererType {
    Glow,
    #[cfg(windows)]
    Direct3D11,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, IntoStaticStr, EnumIter)]
pub enum RubyTextType {
    None,
    Furigana,
    Romaji,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, IntoStaticStr, EnumIter)]
pub enum TranslatorType {
    DeepL,
    ChatGpt,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ChatGptSettings {
    pub system_prompt: String,
    pub max_context_tokens: u32,
    pub moderation: bool,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub presence_penalty: Option<f32>,
    pub model: Model,
}

impl Default for ChatGptSettings {
    fn default() -> Self {
        Self {
            system_prompt: "You will translate the following visual novel script into English."
                .into(),
            max_context_tokens: 64,
            moderation: false,
            temperature: None,
            top_p: None,
            max_tokens: Some(128),
            presence_penalty: None,
            model: Model::Gpt35Turbo0613,
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub ichiran_path: String,
    pub postgres_path: String,
    pub db_path: String,

    pub renderer_type: RendererType,
    pub transparent: bool,
    pub on_top: bool,
    pub overlay_mode: bool,
    pub use_force_dpi: bool,
    pub force_dpi: f64,

    pub ruby_text_type: RubyTextType,
    pub more_variants: bool,
    pub stroke_text: bool,

    pub translator_type: TranslatorType,
    pub auto_translate: bool,
    pub deepl_api_key: String,
    pub openai_api_key: String,
    pub chatgpt: ChatGptSettings,

    pub vv_model_path: String,
    pub auto_tts_regex: Option<String>,

    pub watch_clipboard: bool,
    pub show_manual_input: bool,
    pub style: Option<Vec<u8>>,

    pub regex_match: String,
    pub regex_replace: String,

    pub inject_proc_name: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            ichiran_path: Default::default(),
            postgres_path: Default::default(),
            db_path: Default::default(),

            renderer_type: RendererType::Glow,
            transparent: Default::default(),
            on_top: false,
            overlay_mode: false,
            use_force_dpi: false,
            force_dpi: 0.0,

            ruby_text_type: RubyTextType::None,
            more_variants: true,
            stroke_text: true,

            translator_type: TranslatorType::DeepL,
            auto_translate: false,
            deepl_api_key: Default::default(),
            openai_api_key: Default::default(),
            chatgpt: Default::default(),

            vv_model_path: Default::default(),
            auto_tts_regex: None,

            watch_clipboard: true,
            show_manual_input: true,
            style: None,

            regex_match: Default::default(),
            regex_replace: Default::default(),

            inject_proc_name: Default::default(),
        }
    }
}
impl Settings {
    pub fn set_style(&mut self, style: Option<&imgui::Style>) {
        if let Some(style) = style {
            self.style = Some(
                unsafe {
                    std::slice::from_raw_parts(
                        (style as *const _) as *const u8,
                        std::mem::size_of::<imgui::Style>(),
                    )
                }
                .to_vec(),
            );
        } else {
            self.style = None;
        }
    }
    pub fn style(&self) -> Option<imgui::Style> {
        self.style
            .as_ref()
            .map(|style| unsafe { std::ptr::read(style.as_ptr() as *const _) })
    }

    const CONFIG_FILE: &str = "niinii.toml";
    pub fn from_file() -> Self {
        let user_config = dirs::config_dir().map(|x| x.join("niinii").join(Self::CONFIG_FILE));
        let settings: Settings = std::fs::read_to_string(Self::CONFIG_FILE)
            .ok()
            .or_else(|| user_config.and_then(|x| std::fs::read_to_string(x).ok()))
            .and_then(|x| toml::from_str(&x).ok())
            .unwrap_or_default();
        settings
    }
    pub fn write_to_file(&self) -> std::io::Result<()> {
        std::fs::write(Self::CONFIG_FILE, toml::to_string(self).unwrap())
    }
}
