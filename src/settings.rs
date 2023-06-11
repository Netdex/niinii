use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::{Deserialize, Serialize};
use strum_macros::{EnumString, EnumVariantNames};

#[derive(Debug, FromPrimitive, EnumString, EnumVariantNames)]
pub enum RendererType {
    Glow = 0,
    #[cfg(windows)]
    Direct3D11 = 1,
}

#[derive(Copy, Clone, PartialEq, Eq, FromPrimitive, EnumString, EnumVariantNames)]
pub enum RubyTextType {
    None = 0,
    Furigana = 1,
    Romaji = 2,
}

#[derive(Copy, Clone, PartialEq, Eq, FromPrimitive, EnumString, EnumVariantNames)]
pub enum TranslatorType {
    DeepL = 0,
    ChatGpt = 1,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub ichiran_path: String,
    pub postgres_path: String,
    pub db_path: String,

    pub renderer_type_idx: usize,
    pub transparent: bool,
    pub on_top: bool,
    pub overlay_mode: bool,
    pub use_force_dpi: bool,
    pub force_dpi: f64,

    pub ruby_text_type_idx: usize,
    pub more_variants: bool,
    pub stroke_text: bool,

    pub translator_type_idx: usize,
    pub auto_translate: bool,
    pub deepl_api_key: String,
    pub openai_api_key: String,
    pub chatgpt_system_prompt: String,
    pub chatgpt_max_context_tokens: u32,
    pub chatgpt_max_tokens: u32,
    pub chatgpt_moderation: bool,

    pub vv_model_path: String,

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

            renderer_type_idx: RendererType::Glow as usize,
            transparent: Default::default(),
            on_top: false,
            overlay_mode: false,
            use_force_dpi: false,
            force_dpi: 0.0,

            ruby_text_type_idx: RubyTextType::None as usize,
            more_variants: true,
            stroke_text: true,

            translator_type_idx: TranslatorType::DeepL as usize,
            auto_translate: false,
            deepl_api_key: Default::default(),
            openai_api_key: Default::default(),
            chatgpt_system_prompt:
                "You will translate the following visual novel script into English.".into(),
            chatgpt_max_context_tokens: 64,
            chatgpt_max_tokens: 128,
            chatgpt_moderation: false,

            vv_model_path: Default::default(),

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
    pub fn renderer_type(&self) -> RendererType {
        RendererType::from_usize(self.renderer_type_idx).unwrap()
    }

    pub fn translator_type(&self) -> TranslatorType {
        TranslatorType::from_usize(self.translator_type_idx).unwrap()
    }

    pub fn ruby_text_type(&self) -> RubyTextType {
        RubyTextType::from_usize(self.ruby_text_type_idx).unwrap()
    }

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
