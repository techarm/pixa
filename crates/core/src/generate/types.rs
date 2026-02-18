use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Gemini model selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GeminiModel {
    /// gemini-2.5-flash-image (default)
    Flash,
    /// gemini-3-pro-image-preview
    Pro,
    /// Custom model ID
    Custom(String),
}

impl GeminiModel {
    pub fn model_id(&self) -> &str {
        match self {
            Self::Flash => "gemini-2.5-flash-image",
            Self::Pro => "gemini-3-pro-image-preview",
            Self::Custom(id) => id,
        }
    }
}

impl Default for GeminiModel {
    fn default() -> Self {
        Self::Flash
    }
}

impl std::str::FromStr for GeminiModel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "flash" => Ok(Self::Flash),
            "pro" => Ok(Self::Pro),
            other => Ok(Self::Custom(other.to_string())),
        }
    }
}

impl std::fmt::Display for GeminiModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.model_id())
    }
}

/// Configuration for the Gemini API client
#[derive(Debug, Clone)]
pub struct GeminiConfig {
    pub api_key: String,
    pub model: GeminiModel,
    pub timeout_secs: u64,
}

impl GeminiConfig {
    /// Build config from environment variables.
    /// Priority: PIXA_GEMINI_API_KEY → GEMINI_API_KEY → GOOGLE_API_KEY
    pub fn from_env() -> Result<Self, super::error::GenerateError> {
        let api_key = std::env::var("PIXA_GEMINI_API_KEY")
            .or_else(|_| std::env::var("GEMINI_API_KEY"))
            .or_else(|_| std::env::var("GOOGLE_API_KEY"))
            .map_err(|_| super::error::GenerateError::ApiKeyNotFound)?;

        let model = std::env::var("PIXA_GEMINI_MODEL")
            .ok()
            .and_then(|m| m.parse().ok())
            .unwrap_or_default();

        Ok(Self {
            api_key,
            model,
            timeout_secs: 120,
        })
    }

    /// Build config with explicit model override
    pub fn with_model(mut self, model: GeminiModel) -> Self {
        self.model = model;
        self
    }
}

/// Output image format
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Png,
    Jpeg,
}

impl OutputFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "png" => Ok(Self::Png),
            "jpeg" | "jpg" => Ok(Self::Jpeg),
            other => Err(format!("Unknown format: {other}. Use 'png' or 'jpeg'.")),
        }
    }
}

/// Result from a generation operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateResult {
    pub success: bool,
    pub message: String,
    pub generated_files: Vec<PathBuf>,
    pub prompts_used: Vec<String>,
    pub api_calls_made: u32,
}

// ---------------------------------------------------------------------------
// Request types for each subcommand
// ---------------------------------------------------------------------------

/// Image generation request
#[derive(Debug, Clone)]
pub struct ImageRequest {
    pub prompt: String,
    pub count: u8,
    pub styles: Vec<String>,
    pub variations: Vec<String>,
    pub format: OutputFormat,
    pub output_dir: Option<PathBuf>,
    pub dry_run: bool,
}

/// Image editing request
#[derive(Debug, Clone)]
pub struct EditRequest {
    pub input: PathBuf,
    pub prompt: String,
    pub format: OutputFormat,
    pub output_dir: Option<PathBuf>,
    pub dry_run: bool,
}

/// Image restoration request
#[derive(Debug, Clone)]
pub struct RestoreRequest {
    pub input: PathBuf,
    pub prompt: String,
    pub format: OutputFormat,
    pub output_dir: Option<PathBuf>,
    pub dry_run: bool,
}

/// Icon generation request
#[derive(Debug, Clone)]
pub struct IconRequest {
    pub prompt: String,
    pub sizes: Vec<u32>,
    pub icon_type: String,
    pub style: String,
    pub background: String,
    pub corners: String,
    pub format: OutputFormat,
    pub output_dir: Option<PathBuf>,
    pub dry_run: bool,
}

/// Pattern generation request
#[derive(Debug, Clone)]
pub struct PatternRequest {
    pub prompt: String,
    pub pattern_type: String,
    pub style: String,
    pub density: String,
    pub colors: String,
    pub size: String,
    pub format: OutputFormat,
    pub output_dir: Option<PathBuf>,
    pub dry_run: bool,
}

/// Story/sequence generation request
#[derive(Debug, Clone)]
pub struct StoryRequest {
    pub prompt: String,
    pub steps: u8,
    pub story_type: String,
    pub style: String,
    pub transition: String,
    pub format: OutputFormat,
    pub output_dir: Option<PathBuf>,
    pub dry_run: bool,
}

/// Diagram generation request
#[derive(Debug, Clone)]
pub struct DiagramRequest {
    pub prompt: String,
    pub diagram_type: String,
    pub style: String,
    pub layout: String,
    pub complexity: String,
    pub colors: String,
    pub annotations: String,
    pub format: OutputFormat,
    pub output_dir: Option<PathBuf>,
    pub dry_run: bool,
}

/// Logo generation request
#[derive(Debug, Clone)]
pub struct LogoRequest {
    pub prompt: String,
    pub style: String,
    pub format: OutputFormat,
    pub output_dir: Option<PathBuf>,
    pub dry_run: bool,
}
