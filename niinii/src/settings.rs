use num_derive::FromPrimitive;
use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, IntoStaticStr};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Deserialize,
    Serialize,
    FromPrimitive,
    IntoStaticStr,
    EnumIter,
)]
pub enum RendererType {
    Glow,
    #[cfg(windows)]
    Direct3D11,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, IntoStaticStr, EnumIter)]
pub enum RubyTextType {
    None,
    Furigana,
    Romaji,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, IntoStaticStr, EnumIter)]
pub enum TranslatorType {
    Chat,
    Responses,
    Realtime,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ChatSettings {
    pub system_prompt: String,
    pub max_context_tokens: [u32; 2],
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub presence_penalty: Option<f32>,
    pub connection_timeout: u64,
    pub timeout: u64,
    pub stream: bool,
    pub service_tier: Option<openai::ServiceTier>,
    pub reasoning_effort: Option<openai::ReasoningEffort>,
    pub verbosity: Option<openai::Verbosity>,
}
impl Default for ChatSettings {
    fn default() -> Self {
        Self {
            system_prompt: "You will translate the following visual novel script into English."
                .into(),
            max_context_tokens: [64, 64],
            temperature: None,
            top_p: None,
            max_tokens: Some(128),
            presence_penalty: None,
            connection_timeout: 3000,
            timeout: 10000,
            stream: true,
            service_tier: Some(openai::ServiceTier::Priority),
            reasoning_effort: None,
            verbosity: None,
        }
    }
}

/// Settings for the Responses API backend. Mirrors `ChatSettings` but drops the
/// context-buffer knobs (state lives server-side via `previous_response_id`).
/// Latency-oriented defaults (per the
/// OpenAI latency-optimization + GPT-5.5 guides): streaming on, priority
/// service tier, capped output tokens, low reasoning effort (the recommended
/// efficient setting for most production workflows), and low verbosity (fewer
/// output tokens). All are adjustable in the tuning UI.
#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ResponsesSettings {
    pub system_prompt: String,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub reasoning_effort: Option<openai::ReasoningEffort>,
    pub verbosity: Option<openai::Verbosity>,
    pub service_tier: Option<openai::ServiceTier>,
    pub stream: bool,
    /// Maintain server-side conversation state across turns via
    /// `previous_response_id` (`store: true`). When off, each line is
    /// translated independently (no `previous_response_id`, `store: false`), so
    /// the conversation never inflates -- lowest/flattest latency and cost, at
    /// the expense of cross-line memory (the VNDB addendum still supplies
    /// character/speaker context).
    pub chain: bool,
    pub connection_timeout: u64,
    pub timeout: u64,
}
impl Default for ResponsesSettings {
    fn default() -> Self {
        Self {
            system_prompt: "You will translate the following visual novel script into English."
                .into(),
            temperature: None,
            top_p: None,
            max_tokens: Some(128),
            reasoning_effort: Some(openai::ReasoningEffort::Low),
            verbosity: Some(openai::Verbosity::Low),
            service_tier: Some(openai::ServiceTier::Priority),
            stream: true,
            chain: true,
            connection_timeout: 3000,
            timeout: 10000,
        }
    }
}

/// Conversation-truncation strategy for the Realtime session, mirroring the GA
/// `RealtimeTruncation` schema. `Auto` drops the oldest messages once the input
/// token limit is hit; `Disabled` never truncates (the server errors instead);
/// `RetentionRatio` truncates early, keeping messages up to
/// [`RealtimeSettings::truncation_retention_ratio`] of the model's max context
/// (fewer future truncations, better cache rate). The ratio is held separately
/// so toggling between modes in the UI does not discard it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, IntoStaticStr, EnumIter)]
pub enum TruncationMode {
    Auto,
    Disabled,
    #[strum(serialize = "Retention ratio")]
    RetentionRatio,
}

/// Settings for the Realtime API (WebSocket) backend. The Realtime API is a
/// stateful, always-streaming text session. It exposes the realtime-applicable
/// generation knobs only: `reasoning_effort` (reasoning-capable realtime models
/// such as `gpt-realtime-2`) and `max_tokens`. It has no service-tier /
/// verbosity / temperature / top_p surface (none are fields on the GA
/// `RealtimeSessionCreateRequest`). `chain` keeps the server-side conversation
/// across turns; turning it off reconnects per line so context never
/// accumulates.
///
/// `reasoning_effort` defaults to off so non-reasoning realtime models (e.g.
/// `gpt-realtime`) are not sent an unsupported field.
///
/// Requires a realtime-capable model in the shared `openai_model` setting.
#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct RealtimeSettings {
    pub system_prompt: String,
    pub reasoning_effort: Option<openai::ReasoningEffort>,
    pub max_tokens: Option<u32>,
    pub chain: bool,
    pub truncation: TruncationMode,
    /// Fraction (0.0..=1.0) of the model's max context to retain when
    /// `truncation` is [`TruncationMode::RetentionRatio`]; ignored otherwise.
    pub truncation_retention_ratio: f32,
    /// Optional cap on tokens kept after the instructions
    /// (`token_limits.post_instructions`) when `truncation` is
    /// [`TruncationMode::RetentionRatio`]. `None` uses the model default.
    pub truncation_post_instructions_token_limit: Option<u32>,
    pub connection_timeout: u64,
    pub timeout: u64,
}
impl Default for RealtimeSettings {
    fn default() -> Self {
        Self {
            system_prompt: "You will translate the following visual novel script into English."
                .into(),
            reasoning_effort: None,
            max_tokens: Some(128),
            chain: true,
            truncation: TruncationMode::Auto,
            truncation_retention_ratio: 0.8,
            truncation_post_instructions_token_limit: None,
            connection_timeout: 3000,
            timeout: 10000,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub ichiran_path: String,
    pub ichiran_pool_size: usize,
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
    pub openai_api_key: String,
    /// OpenAI(-compatible) API base URL, shared by all translator backends.
    pub openai_api_endpoint: String,
    /// Model id, shared by all translator backends.
    pub openai_model: openai::ModelId,
    pub chat: ChatSettings,
    pub responses: ResponsesSettings,
    pub realtime: RealtimeSettings,

    pub vv_model_path: String,
    pub auto_tts_regex: Option<String>,

    pub watch_clipboard: bool,
    pub show_manual_input: bool,
    pub style: Option<Vec<u8>>,

    pub regex_match: String,
    pub regex_replace: String,

    pub inject_proc_name: String,

    /// VNDB id (e.g. "v17") of the most recently selected active visual
    /// novel. Restored on startup.
    pub vndb_active_id: Option<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            ichiran_path: "data/ichiran-cli.exe".into(),
            ichiran_pool_size: ichiran::default_pool_size(),
            postgres_path: "data/pgsql/bin".into(),
            db_path: "data/pgsql/data".into(),

            renderer_type: RendererType::Direct3D11,
            transparent: Default::default(),
            on_top: false,
            overlay_mode: false,
            use_force_dpi: false,
            force_dpi: 0.0,

            ruby_text_type: RubyTextType::None,
            more_variants: true,
            stroke_text: true,

            translator_type: TranslatorType::Chat,
            auto_translate: false,
            openai_api_key: Default::default(),
            openai_api_endpoint: "https://api.openai.com".into(),
            openai_model: Default::default(),
            chat: Default::default(),
            responses: Default::default(),
            realtime: Default::default(),

            vv_model_path: Default::default(),
            auto_tts_regex: None,

            watch_clipboard: true,
            show_manual_input: true,
            style: None,

            regex_match: Default::default(),
            regex_replace: Default::default(),

            inject_proc_name: Default::default(),

            vndb_active_id: None,
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

    const CONFIG_FILE: &'static str = "niinii.toml";
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
