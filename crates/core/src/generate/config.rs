use serde::Deserialize;
use std::collections::HashMap;

use super::error::GenerateError;

// Embed default TOML configs at compile time
const IMAGE_CONFIG: &str = include_str!("../../../../config/image.toml");
const EDIT_CONFIG: &str = include_str!("../../../../config/edit.toml");
const RESTORE_CONFIG: &str = include_str!("../../../../config/restore.toml");
const ICON_CONFIG: &str = include_str!("../../../../config/icon.toml");
const PATTERN_CONFIG: &str = include_str!("../../../../config/pattern.toml");
const STORY_CONFIG: &str = include_str!("../../../../config/story.toml");
const DIAGRAM_CONFIG: &str = include_str!("../../../../config/diagram.toml");

// ---------------------------------------------------------------------------
// Image config
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ImageConfig {
    pub defaults: ImageDefaults,
    pub prompt: ImagePrompt,
    pub variations: HashMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct ImageDefaults {
    pub count: u8,
    pub format: String,
}

#[derive(Debug, Deserialize)]
pub struct ImagePrompt {
    pub style_suffix: String,
}

pub fn load_image_config() -> Result<ImageConfig, GenerateError> {
    toml::from_str(IMAGE_CONFIG)
        .map_err(|e| GenerateError::ConfigError(format!("image.toml: {e}")))
}

// ---------------------------------------------------------------------------
// Edit config
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct EditConfig {
    pub defaults: EditDefaults,
    pub prompt: EditPrompt,
}

#[derive(Debug, Deserialize)]
pub struct EditDefaults {
    pub format: String,
}

#[derive(Debug, Deserialize)]
pub struct EditPrompt {
    pub filename_prefix: String,
}

pub fn load_edit_config() -> Result<EditConfig, GenerateError> {
    toml::from_str(EDIT_CONFIG)
        .map_err(|e| GenerateError::ConfigError(format!("edit.toml: {e}")))
}

// ---------------------------------------------------------------------------
// Restore config
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RestoreConfig {
    pub defaults: RestoreDefaults,
    pub prompt: RestorePrompt,
}

#[derive(Debug, Deserialize)]
pub struct RestoreDefaults {
    pub format: String,
}

#[derive(Debug, Deserialize)]
pub struct RestorePrompt {
    pub filename_prefix: String,
}

pub fn load_restore_config() -> Result<RestoreConfig, GenerateError> {
    toml::from_str(RESTORE_CONFIG)
        .map_err(|e| GenerateError::ConfigError(format!("restore.toml: {e}")))
}

// ---------------------------------------------------------------------------
// Icon config
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct IconConfig {
    pub defaults: IconDefaults,
    pub prompt: IconPrompt,
}

#[derive(Debug, Deserialize)]
pub struct IconDefaults {
    pub sizes: Vec<u32>,
    pub r#type: String,
    pub style: String,
    pub background: String,
    pub corners: String,
    pub format: String,
}

#[derive(Debug, Deserialize)]
pub struct IconPrompt {
    pub base: String,
    pub app_icon_suffix: String,
    pub background_suffix: String,
    pub tail: String,
}

pub fn load_icon_config() -> Result<IconConfig, GenerateError> {
    toml::from_str(ICON_CONFIG)
        .map_err(|e| GenerateError::ConfigError(format!("icon.toml: {e}")))
}

// ---------------------------------------------------------------------------
// Pattern config
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PatternConfig {
    pub defaults: PatternDefaults,
    pub prompt: PatternPrompt,
}

#[derive(Debug, Deserialize)]
pub struct PatternDefaults {
    pub r#type: String,
    pub style: String,
    pub density: String,
    pub colors: String,
    pub size: String,
    pub format: String,
}

#[derive(Debug, Deserialize)]
pub struct PatternPrompt {
    pub base: String,
    pub seamless_suffix: String,
    pub tail: String,
}

pub fn load_pattern_config() -> Result<PatternConfig, GenerateError> {
    toml::from_str(PATTERN_CONFIG)
        .map_err(|e| GenerateError::ConfigError(format!("pattern.toml: {e}")))
}

// ---------------------------------------------------------------------------
// Story config
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct StoryConfig {
    pub defaults: StoryDefaults,
    pub prompt: StoryPromptConfig,
    pub type_context: HashMap<String, String>,
    pub transitions: StoryTransitions,
}

#[derive(Debug, Deserialize)]
pub struct StoryDefaults {
    pub steps: u8,
    pub r#type: String,
    pub style: String,
    pub transition: String,
    pub format: String,
}

#[derive(Debug, Deserialize)]
pub struct StoryPromptConfig {
    pub base: String,
}

#[derive(Debug, Deserialize)]
pub struct StoryTransitions {
    pub template: String,
}

pub fn load_story_config() -> Result<StoryConfig, GenerateError> {
    toml::from_str(STORY_CONFIG)
        .map_err(|e| GenerateError::ConfigError(format!("story.toml: {e}")))
}

// ---------------------------------------------------------------------------
// Diagram config
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DiagramConfig {
    pub defaults: DiagramDefaults,
    pub prompt: DiagramPrompt,
}

#[derive(Debug, Deserialize)]
pub struct DiagramDefaults {
    pub r#type: String,
    pub style: String,
    pub layout: String,
    pub complexity: String,
    pub colors: String,
    pub annotations: String,
    pub format: String,
}

#[derive(Debug, Deserialize)]
pub struct DiagramPrompt {
    pub base: String,
    pub detail: String,
    pub annotation: String,
    pub tail: String,
}

pub fn load_diagram_config() -> Result<DiagramConfig, GenerateError> {
    toml::from_str(DIAGRAM_CONFIG)
        .map_err(|e| GenerateError::ConfigError(format!("diagram.toml: {e}")))
}
