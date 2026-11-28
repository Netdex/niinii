//! Application configuration, read from and written to `niinii.toml`.
//!
//! Settings are grouped into the sections the Settings window shows; the
//! sections are flattened, so `niinii.toml` stays a flat list of keys. Fields
//! marked `*` in their label take effect on restart.

use dear_imgui_reflect::ImGuiReflect;
use dear_imgui_rs::Theme;
use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, IntoStaticStr};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, IntoStaticStr, EnumIter, ImGuiReflect,
)]
pub enum RubyTextType {
    None,
    Furigana,
    Romaji,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, IntoStaticStr, EnumIter, ImGuiReflect,
)]
pub enum TranslatorType {
    Chat,
    Responses,
    Realtime,
}

/// A path to a file. Edited as text, or picked with the system file dialog.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct FilePath {
    pub path: String,
    /// Why the last pick failed, shown with the field until the next pick.
    #[serde(skip)]
    pub pick_error: Option<String>,
}

/// A path to a folder. Edited as text, or picked with the system folder dialog.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct FolderPath {
    pub path: String,
    /// Why the last pick failed, shown with the field until the next pick.
    #[serde(skip)]
    pub pick_error: Option<String>,
}

impl FilePath {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            pick_error: None,
        }
    }
}

impl FolderPath {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            pick_error: None,
        }
    }
}

/// Context size limits for the chat backend, in tokens: once the context
/// exceeds the threshold (second), it is trimmed down to the target (first).
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct ContextLimit(pub [u32; 2]);

/// Text that is masked when shown, such as an API key.
#[derive(Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Secret(pub String);

#[derive(Clone, Deserialize, Serialize, ImGuiReflect)]
#[serde(default)]
#[imgui(grid, open)]
pub struct IchiranSettings {
    #[imgui(name = "ichiran-cli*", help = "Path of ichiran-cli executable")]
    pub ichiran_path: FilePath,
    /// Resident ichiran-cli workers. Each holds a Postgres connection.
    #[imgui(
        name = "Workers*",
        min = 1,
        max = 64,
        help = "Number of resident ichiran-cli workers. \
         More = better parse parallelism, but each worker holds a Postgres \
         connection. Takes effect on restart."
    )]
    pub ichiran_pool_size: u32,
    #[imgui(name = "Postgres bin*", help = "Path of postgres 'bin' directory")]
    pub postgres_path: FolderPath,
    #[imgui(name = "Database*", help = "Path of postgres database directory")]
    pub db_path: FolderPath,
}
impl Default for IchiranSettings {
    fn default() -> Self {
        Self {
            ichiran_path: FilePath::new("data/ichiran-cli.exe"),
            ichiran_pool_size: ichiran::default_pool_size() as u32,
            postgres_path: FolderPath::new("data/pgsql/bin"),
            db_path: FolderPath::new("data/pgsql/data"),
        }
    }
}

/// Rewrites incoming text before it is glossed.
#[derive(Clone, Default, Deserialize, Serialize, ImGuiReflect)]
#[serde(default)]
#[imgui(grid, open)]
pub struct TextSettings {
    #[imgui(name = "Regex match")]
    pub regex_match: String,
    #[imgui(name = "Regex replace")]
    pub regex_replace: String,
}

#[derive(Clone, Deserialize, Serialize, ImGuiReflect)]
#[serde(default)]
#[imgui(grid, open)]
pub struct InterfaceSettings {
    #[imgui(name = "Ruby text")]
    pub ruby_text_type: RubyTextType,
    /// Search for different ways to interpret a phrase.
    #[imgui(
        name = "Alternate interpretations",
        help = "Search for different ways to interpret a phrase"
    )]
    pub more_variants: bool,
    #[imgui(name = "Stroke text")]
    pub stroke_text: bool,
}
impl Default for InterfaceSettings {
    fn default() -> Self {
        Self {
            ruby_text_type: RubyTextType::None,
            more_variants: true,
            stroke_text: true,
        }
    }
}

#[derive(Clone, Deserialize, Serialize, ImGuiReflect)]
#[serde(default)]
#[imgui(grid, open)]
pub struct TranslationSettings {
    #[imgui(name = "Translator*")]
    pub translator_type: TranslatorType,
    #[imgui(name = "Auto-translate")]
    pub auto_translate: bool,
    #[imgui(name = "OpenAI API key*")]
    pub openai_api_key: Secret,
    /// OpenAI(-compatible) API base URL, shared by all translator backends.
    #[imgui(name = "OpenAI API endpoint*")]
    pub openai_api_endpoint: String,
    /// Model id, shared by all translator backends. Chosen in the
    /// Translator window from the models the endpoint lists.
    #[imgui(skip)]
    pub openai_model: openai::ModelId,
}
impl Default for TranslationSettings {
    fn default() -> Self {
        Self {
            translator_type: TranslatorType::Chat,
            auto_translate: false,
            openai_api_key: Secret::default(),
            openai_api_endpoint: "https://api.openai.com".into(),
            openai_model: Default::default(),
        }
    }
}

#[derive(Clone, Default, Deserialize, Serialize, ImGuiReflect)]
#[serde(default)]
#[imgui(grid, open)]
pub struct TtsSettings {
    #[imgui(name = "VOICEVOX models*", help = "Path of VOICEVOX models")]
    pub vv_model_path: FolderPath,
    /// Lines matching this are spoken; its first capture group, if any, is
    /// the part spoken.
    #[imgui(name = "Auto TTS regex")]
    pub auto_tts_regex: Option<String>,
}

#[derive(Clone, Deserialize, Serialize, ImGuiReflect)]
#[serde(default)]
#[imgui(grid, open)]
pub struct WindowSettings {
    /// Multiplier on all text sizes.
    #[imgui(name = "Text size", slider, min = 0.5, max = 2.0, format = "%.2fx")]
    pub text_scale: f32,
    #[imgui(name = "Always on-top")]
    pub on_top: bool,
    #[imgui(name = "Transparent")]
    pub transparent: bool,
}
impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            text_scale: 1.0,
            on_top: false,
            transparent: false,
        }
    }
}

#[derive(Clone, Deserialize, Serialize, ImGuiReflect)]
#[serde(default)]
#[imgui(grid, columns = 2, open)]
pub struct ChatSettings {
    #[imgui(skip)]
    pub system_prompt: String,
    /// Target/threshold. Limits context size while making good use of prefix
    /// caches.
    #[imgui(
        name = "Context tokens",
        help = "Target/threshold. For limiting context size while optimizing usage of prefix caches"
    )]
    pub max_context_tokens: ContextLimit,
    #[imgui(
        name = "Temperature",
        slider,
        min = 0.0,
        max = 2.0,
        format = "%.2f",
        enable_with = 1.0
    )]
    pub temperature: Option<f32>,
    #[imgui(
        name = "Top P",
        slider,
        min = 0.0,
        max = 1.0,
        format = "%.2f",
        enable_with = 1.0
    )]
    pub top_p: Option<f32>,
    #[imgui(name = "Max response tokens")]
    pub max_tokens: Option<u32>,
    #[imgui(name = "Presence penalty", slider, min = -2.0, max = 2.0, format = "%.2f")]
    pub presence_penalty: Option<f32>,
    #[imgui(name = "Connection timeout (ms)*", slider, min = 100, max = 10000)]
    pub connection_timeout: u64,
    #[imgui(name = "Timeout (ms)*", slider, min = 100, max = 10000)]
    pub timeout: u64,
    /// May require ID verification for some models.
    #[imgui(
        name = "Stream",
        help = "Use streaming API (may require ID verification for some models)"
    )]
    pub stream: bool,
    #[imgui(name = "Service tier")]
    pub service_tier: Option<openai::ServiceTier>,
    #[imgui(
        name = "Reasoning effort",
        help = "Effort for reasoning models (GPT-5 and o3 models)"
    )]
    pub reasoning_effort: Option<openai::ReasoningEffort>,
    #[imgui(name = "Verbosity")]
    pub verbosity: Option<openai::Verbosity>,
}
impl Default for ChatSettings {
    fn default() -> Self {
        Self {
            system_prompt: "You will translate the following visual novel script into English."
                .into(),
            max_context_tokens: ContextLimit([1024, 2048]),
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
#[derive(Clone, Deserialize, Serialize, ImGuiReflect)]
#[serde(default)]
#[imgui(grid, columns = 2, open)]
pub struct ResponsesSettings {
    #[imgui(skip)]
    pub system_prompt: String,
    #[imgui(
        name = "Temperature",
        slider,
        min = 0.0,
        max = 2.0,
        format = "%.2f",
        enable_with = 1.0
    )]
    pub temperature: Option<f32>,
    #[imgui(
        name = "Top P",
        slider,
        min = 0.0,
        max = 1.0,
        format = "%.2f",
        enable_with = 1.0
    )]
    pub top_p: Option<f32>,
    #[imgui(name = "Max output tokens")]
    pub max_tokens: Option<u32>,
    #[imgui(
        name = "Reasoning effort",
        help = "Low is efficient for most tasks; None for lowest latency (classification-style)"
    )]
    pub reasoning_effort: Option<openai::ReasoningEffort>,
    #[imgui(name = "Verbosity")]
    pub verbosity: Option<openai::Verbosity>,
    /// Priority gives the lowest-latency serving tier.
    #[imgui(
        name = "Service tier",
        help = "Priority gives the lowest-latency serving tier"
    )]
    pub service_tier: Option<openai::ServiceTier>,
    #[imgui(name = "Stream", help = "Stream output for lower time-to-first-token")]
    pub stream: bool,
    /// Maintain server-side conversation state across turns via
    /// `previous_response_id` (`store: true`). When off, each line is
    /// translated independently (no `previous_response_id`, `store: false`), so
    /// the conversation never inflates -- lowest/flattest latency and cost, at
    /// the expense of cross-line memory (the VNDB addendum still supplies
    /// character/speaker context).
    #[imgui(
        name = "Chain conversation",
        help = "Keep server-side context across turns (previous_response_id). \
         Off = translate each line independently; lowest latency/cost, no \
         cross-line memory."
    )]
    pub chain: bool,
    #[imgui(name = "Connection timeout (ms)*", slider, min = 100, max = 10000)]
    pub connection_timeout: u64,
    #[imgui(name = "Timeout (ms)*", slider, min = 100, max = 10000)]
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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, IntoStaticStr, EnumIter, ImGuiReflect,
)]
pub enum TruncationMode {
    Auto,
    Disabled,
    #[strum(serialize = "Retention ratio")]
    #[imgui(name = "Retention ratio")]
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
#[derive(Clone, Deserialize, Serialize, ImGuiReflect)]
#[serde(default)]
#[imgui(grid, columns = 2, open)]
pub struct RealtimeSettings {
    #[imgui(skip)]
    pub system_prompt: String,
    #[imgui(name = "Reasoning effort", enable_with = openai::ReasoningEffort::Low, help = "Only for reasoning-capable realtime models (e.g. gpt-realtime-2); \
         leave off for gpt-realtime. Lower effort = lower latency.")]
    pub reasoning_effort: Option<openai::ReasoningEffort>,
    #[imgui(name = "Max output tokens")]
    pub max_tokens: Option<u32>,
    #[imgui(
        name = "Chain conversation",
        help = "Keep the server-side conversation across turns. Off = reconnect \
         per line and translate independently; no cross-line memory."
    )]
    pub chain: bool,
    #[imgui(
        name = "Truncation",
        help = "When the conversation exceeds the model's input-token limit:\n\
         - Auto: drop the oldest messages\n\
         - Disabled: never truncate (the server errors instead)\n\
         - Retention ratio: truncate early to keep a fraction of\n\
         \x20 the max context (fewer truncations, better cache rate)\n\
         Only matters when chaining."
    )]
    pub truncation: TruncationMode,
    /// Fraction (0.0..=1.0) of the model's max context to retain when
    /// `truncation` is [`TruncationMode::RetentionRatio`]; ignored otherwise.
    #[imgui(name = "Retention ratio", slider, min = 0.0, max = 1.0, format = "%.2f", enabled_if = self.truncation == TruncationMode::RetentionRatio)]
    pub truncation_retention_ratio: f32,
    /// Optional cap on tokens kept after the instructions
    /// (`token_limits.post_instructions`) when `truncation` is
    /// [`TruncationMode::RetentionRatio`]. `None` uses the model default.
    #[imgui(name = "Token limit", enable_with = 4096, enabled_if = self.truncation == TruncationMode::RetentionRatio, help = "token_limits.post_instructions: cap on tokens kept\n\
         after the instructions.\n\
         Off = model default. Lower = truncate sooner but\n\
         amortize message drops for a better cache rate.")]
    pub truncation_post_instructions_token_limit: Option<u32>,
    #[imgui(name = "Connection timeout (ms)*", slider, min = 100, max = 10000)]
    pub connection_timeout: u64,
    #[imgui(name = "Timeout (ms)*", slider, min = 100, max = 10000)]
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

// Sections and plain values come before the tables (`chat` onwards): TOML
// cannot hold plain values after a table.
#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    #[serde(flatten)]
    pub ichiran: IchiranSettings,
    #[serde(flatten)]
    pub text: TextSettings,
    #[serde(flatten)]
    pub interface: InterfaceSettings,
    #[serde(flatten)]
    pub translation: TranslationSettings,
    #[serde(flatten)]
    pub tts: TtsSettings,
    #[serde(flatten)]
    pub window: WindowSettings,

    pub watch_clipboard: bool,
    pub show_manual_input: bool,
    /// VNDB id (e.g. "v17") of the most recently selected active visual
    /// novel. Restored on startup.
    pub vndb_active_id: Option<String>,

    pub chat: ChatSettings,
    pub responses: ResponsesSettings,
    pub realtime: RealtimeSettings,
    /// Sizes are unscaled; they are scaled to the monitor at startup.
    pub theme: Option<Theme>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            ichiran: Default::default(),
            text: Default::default(),
            interface: Default::default(),
            translation: Default::default(),
            tts: Default::default(),
            window: Default::default(),
            watch_clipboard: true,
            show_manual_input: true,
            vndb_active_id: None,
            chat: Default::default(),
            responses: Default::default(),
            realtime: Default::default(),
            theme: None,
        }
    }
}
impl Settings {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `niinii.toml` as written before settings were grouped into sections.
    const FLAT: &str = r#"
ichiran_path = "x/ichiran-cli.exe"
ichiran_pool_size = 3
postgres_path = "x/bin"
db_path = "x/data"
transparent = true
on_top = true
text_scale = 1.5
ruby_text_type = "Romaji"
more_variants = false
stroke_text = false
translator_type = "Realtime"
auto_translate = true
openai_api_key = "key"
openai_api_endpoint = "https://example.com"
vv_model_path = "x/vv"
auto_tts_regex = "^(.*)$"
watch_clipboard = false
show_manual_input = false
regex_match = "a"
regex_replace = "b"
vndb_active_id = "v17"

[chat]
max_context_tokens = [10, 20]
"#;

    #[test]
    fn reads_flat_file() {
        let s: Settings = toml::from_str(FLAT).unwrap();
        assert_eq!(s.ichiran.ichiran_path, FilePath::new("x/ichiran-cli.exe"));
        assert_eq!(s.ichiran.ichiran_pool_size, 3);
        assert_eq!(s.ichiran.db_path, FolderPath::new("x/data"));
        assert!(s.window.transparent && s.window.on_top);
        assert_eq!(s.window.text_scale, 1.5);
        assert_eq!(s.interface.ruby_text_type, RubyTextType::Romaji);
        assert!(!s.interface.more_variants);
        assert_eq!(s.translation.translator_type, TranslatorType::Realtime);
        assert!(s.translation.openai_api_key == Secret("key".into()));
        assert_eq!(s.tts.auto_tts_regex.as_deref(), Some("^(.*)$"));
        assert_eq!(s.text.regex_replace, "b");
        assert!(!s.watch_clipboard);
        assert_eq!(s.vndb_active_id.as_deref(), Some("v17"));
        assert_eq!(s.chat.max_context_tokens, ContextLimit([10, 20]));
    }

    #[test]
    fn round_trips() {
        let s: Settings = toml::from_str(FLAT).unwrap();
        let written = toml::to_string(&s).unwrap();
        let back: Settings = toml::from_str(&written).unwrap();
        assert_eq!(toml::to_string(&back).unwrap(), written);
        assert!(
            !written.contains("[ichiran]"),
            "sections stay flat:\n{written}"
        );
    }
}
