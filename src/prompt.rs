//! Nanobanana 向けプロンプト生成
//!
//! ローカルの claude / gemini CLI を使用して、
//! Gemini Nanobanana の画像生成に最適化されたプロンプトを生成する。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;
use tracing::{debug, info, warn};

#[derive(Error, Debug)]
pub enum PromptError {
    #[error("CLI '{0}' が見つかりません。インストールされているか確認してください。")]
    CliNotFound(String),
    #[error("CLI 実行エラー: {0}")]
    CliExecution(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("画像ファイルが見つかりません: {0}")]
    ImageNotFound(String),
    #[error("プロバイダーが指定されていません。利用可能: {0:?}")]
    NoProvider(Vec<String>),
}

/// AI プロバイダー
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Claude,
    Gemini,
}

impl Provider {
    pub fn cli_name(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Gemini => "gemini",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Claude => "Claude (claude)",
            Self::Gemini => "Gemini (gemini)",
        }
    }
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.cli_name())
    }
}

impl std::str::FromStr for Provider {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "claude" => Ok(Self::Claude),
            "gemini" => Ok(Self::Gemini),
            _ => Err(format!("Unknown provider: {s}. Use 'claude' or 'gemini'.")),
        }
    }
}

/// プロンプト生成オプション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptOptions {
    /// テキストによる指示（例: "猫が宇宙で浮いてる絵"）
    pub description: Option<String>,
    /// 参考画像のパス
    pub reference_image: Option<PathBuf>,
    /// スタイル指定（例: "anime", "photorealistic", "watercolor"）
    pub style: Option<String>,
    /// アスペクト比（例: "16:9", "1:1", "9:16"）
    pub aspect_ratio: Option<String>,
    /// 追加の制約・要望
    pub extra_instructions: Option<String>,
    /// 生成するプロンプトのバリエーション数
    pub variations: u8,
    /// 言語（プロンプト出力言語）
    pub language: PromptLanguage,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptLanguage {
    English,
    Japanese,
}

impl Default for PromptOptions {
    fn default() -> Self {
        Self {
            description: None,
            reference_image: None,
            style: None,
            aspect_ratio: None,
            extra_instructions: None,
            variations: 1,
            language: PromptLanguage::English,
        }
    }
}

/// プロンプト生成結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptResult {
    /// 生成されたプロンプト（複数バリエーション）
    pub prompts: Vec<String>,
    /// 使用したプロバイダー
    pub provider: String,
    /// 元の入力情報
    pub input_summary: String,
}

/// 利用可能な CLI ツールを検出
pub fn detect_available_providers() -> Vec<Provider> {
    let mut available = Vec::new();

    for provider in [Provider::Claude, Provider::Gemini] {
        if is_cli_available(provider.cli_name()) {
            available.push(provider);
            debug!("{} CLI detected", provider.display_name());
        } else {
            debug!("{} CLI not found", provider.display_name());
        }
    }

    info!(
        "Available providers: {:?}",
        available.iter().map(|p| p.cli_name()).collect::<Vec<_>>()
    );
    available
}

/// CLI が利用可能か確認
fn is_cli_available(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// プロンプトを生成
pub fn generate_prompt(
    provider: Provider,
    options: &PromptOptions,
) -> Result<PromptResult, PromptError> {
    // CLI の存在確認
    if !is_cli_available(provider.cli_name()) {
        return Err(PromptError::CliNotFound(provider.cli_name().to_string()));
    }

    // 参考画像の存在確認
    if let Some(ref img_path) = options.reference_image {
        if !img_path.exists() {
            return Err(PromptError::ImageNotFound(img_path.display().to_string()));
        }
    }

    // メタプロンプト構築
    let meta_prompt = build_meta_prompt(options);
    info!("Generating prompt via {} CLI...", provider.cli_name());
    debug!("Meta-prompt:\n{}", meta_prompt);

    // CLI 実行
    let output = execute_cli(provider, &meta_prompt, options.reference_image.as_deref())?;

    // 結果のパース
    let prompts = parse_prompt_output(&output, options.variations);

    let input_summary = build_input_summary(options);

    Ok(PromptResult {
        prompts,
        provider: provider.cli_name().to_string(),
        input_summary,
    })
}

/// Nanobanana 向けに最適化されたメタプロンプトを構築
fn build_meta_prompt(options: &PromptOptions) -> String {
    let lang_instruction = match options.language {
        PromptLanguage::English => "Output the prompt(s) in English.",
        PromptLanguage::Japanese => "Output the prompt(s) in English. (Nanobanana works best with English prompts even for Japanese users)",
    };

    let variation_instruction = if options.variations > 1 {
        format!(
            "Generate exactly {} different prompt variations. Separate each with a line containing only '---'.",
            options.variations
        )
    } else {
        "Generate exactly 1 prompt.".to_string()
    };

    let mut parts = Vec::new();

    parts.push(format!(
        r#"You are an expert prompt engineer for Google Gemini's image generation (Nanobanana / Gemini image generation).

Your task: Generate high-quality image generation prompts optimized for Gemini's Nanobanana model.

Rules for good Nanobanana prompts:
- Be specific and descriptive about the subject, composition, lighting, and mood
- Include art style, medium, or photography terms when relevant (e.g., "digital painting", "35mm film photography", "Studio Ghibli style")
- Describe spatial relationships clearly ("in the foreground", "seen from above")
- Include atmosphere/mood descriptors ("warm golden hour light", "moody cyberpunk neon")
- Mention color palette when it matters
- Keep prompts between 1-3 sentences for best results
- Avoid negative prompts (Nanobanana doesn't support them well)
- Do NOT include any prefix like "prompt:" or numbering — just output the raw prompt text

{lang_instruction}
{variation_instruction}"#
    ));

    // テキスト指示
    if let Some(ref desc) = options.description {
        parts.push(format!("\nUser's description: \"{desc}\""));
    }

    // 参考画像がある場合
    if options.reference_image.is_some() {
        parts.push(
            "\nA reference image is attached. Analyze its visual style, composition, color palette, and subject matter. Generate a prompt that would recreate a similar image.".to_string()
        );
        if options.description.is_some() {
            parts.push(
                "Combine the user's text description with visual elements from the reference image.".to_string()
            );
        }
    }

    // スタイル指定
    if let Some(ref style) = options.style {
        parts.push(format!("\nDesired style: {style}"));
    }

    // アスペクト比
    if let Some(ref ratio) = options.aspect_ratio {
        parts.push(format!(
            "\nTarget aspect ratio: {ratio} — frame the composition accordingly."
        ));
    }

    // 追加指示
    if let Some(ref extra) = options.extra_instructions {
        parts.push(format!("\nAdditional requirements: {extra}"));
    }

    parts.push(
        "\nNow generate the prompt(s). Output ONLY the prompt text, nothing else.".to_string(),
    );

    parts.join("\n")
}

/// CLI を実行してプロンプトを生成
fn execute_cli(
    provider: Provider,
    meta_prompt: &str,
    reference_image: Option<&Path>,
) -> Result<String, PromptError> {
    let mut cmd = Command::new(provider.cli_name());

    match provider {
        Provider::Claude => {
            // claude -p "prompt" (print mode, no interactive)
            cmd.arg("-p").arg(meta_prompt);

            // 参考画像がある場合はファイルとして渡す
            if let Some(img_path) = reference_image {
                // Claude Code CLI: claude -p "prompt" image.jpg
                cmd.arg(img_path);
            }
        }
        Provider::Gemini => {
            // gemini -p "prompt"  (prompt mode)
            cmd.arg("-p").arg(meta_prompt);

            // 参考画像がある場合
            if let Some(img_path) = reference_image {
                // gemini CLI: gemini -p "prompt" -i image.jpg
                cmd.arg("-i").arg(img_path);
            }
        }
    }

    debug!("Executing: {:?}", cmd);

    let output = cmd.output().map_err(|e| {
        PromptError::CliExecution(format!(
            "Failed to execute {} CLI: {}",
            provider.cli_name(),
            e
        ))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("{} CLI exited with error: {}", provider.cli_name(), stderr);
        return Err(PromptError::CliExecution(format!(
            "{} CLI error (exit code {:?}): {}",
            provider.cli_name(),
            output.status.code(),
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    debug!("CLI output ({} bytes)", stdout.len());

    Ok(stdout)
}

/// CLI 出力からプロンプトをパース
fn parse_prompt_output(output: &str, expected_count: u8) -> Vec<String> {
    let trimmed = output.trim();

    if expected_count <= 1 {
        // 単一プロンプト: 出力全体をクリーンアップして返す
        return vec![clean_prompt_text(trimmed)];
    }

    // 複数バリエーション: --- で分割
    let parts: Vec<String> = trimmed
        .split("\n---\n")
        .map(|s| clean_prompt_text(s.trim()))
        .filter(|s| !s.is_empty())
        .collect();

    if parts.is_empty() {
        // フォールバック: 空行で分割を試みる
        let parts: Vec<String> = trimmed
            .split("\n\n")
            .map(|s| clean_prompt_text(s.trim()))
            .filter(|s| !s.is_empty())
            .collect();
        if parts.len() > 1 {
            return parts;
        }
        // それでもダメなら全体を1つとして返す
        return vec![clean_prompt_text(trimmed)];
    }

    parts
}

/// プロンプトテキストをクリーンアップ
fn clean_prompt_text(text: &str) -> String {
    let text = text.trim();

    // よくある接頭辞を除去
    let prefixes = [
        "Prompt:",
        "prompt:",
        "PROMPT:",
        "Prompt 1:",
        "Prompt 2:",
        "Prompt 3:",
        "1.",
        "2.",
        "3.",
        "1)",
        "2)",
        "3)",
        "- ",
        "* ",
        "```",
        "\"",
    ];

    let mut result = text.to_string();
    for prefix in prefixes {
        if result.starts_with(prefix) {
            result = result[prefix.len()..].trim_start().to_string();
        }
    }

    // 末尾の ``` や " を除去
    for suffix in ["```", "\""] {
        if result.ends_with(suffix) {
            result = result[..result.len() - suffix.len()].trim_end().to_string();
        }
    }

    result
}

/// 入力情報のサマリーを生成
fn build_input_summary(options: &PromptOptions) -> String {
    let mut parts = Vec::new();

    if let Some(ref desc) = options.description {
        parts.push(format!("description=\"{desc}\""));
    }
    if let Some(ref img) = options.reference_image {
        parts.push(format!("image={}", img.display()));
    }
    if let Some(ref style) = options.style {
        parts.push(format!("style={style}"));
    }
    if let Some(ref ratio) = options.aspect_ratio {
        parts.push(format!("ratio={ratio}"));
    }

    if parts.is_empty() {
        "no input".to_string()
    } else {
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_from_str() {
        assert_eq!("claude".parse::<Provider>().unwrap(), Provider::Claude);
        assert_eq!("gemini".parse::<Provider>().unwrap(), Provider::Gemini);
        assert_eq!("Claude".parse::<Provider>().unwrap(), Provider::Claude);
        assert!("unknown".parse::<Provider>().is_err());
    }

    #[test]
    fn test_clean_prompt_text() {
        assert_eq!(
            clean_prompt_text("Prompt: A cat in space"),
            "A cat in space"
        );
        assert_eq!(
            clean_prompt_text("1. A beautiful sunset"),
            "A beautiful sunset"
        );
        assert_eq!(clean_prompt_text("\"A dog running\""), "A dog running");
        assert_eq!(clean_prompt_text("```A forest```"), "A forest");
        assert_eq!(clean_prompt_text("  just text  "), "just text");
    }

    #[test]
    fn test_parse_single() {
        let output = "A cat floating in outer space, digital art style, vibrant nebula background";
        let result = parse_prompt_output(output, 1);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_parse_multiple() {
        let output = "A cat in space, digital art\n---\nCosmic feline, nebula backdrop\n---\nSpace cat, anime style";
        let result = parse_prompt_output(output, 3);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_build_meta_prompt_text_only() {
        let opts = PromptOptions {
            description: Some("猫が宇宙で浮いてる".to_string()),
            ..Default::default()
        };
        let prompt = build_meta_prompt(&opts);
        assert!(prompt.contains("Nanobanana"));
        assert!(prompt.contains("猫が宇宙で浮いてる"));
    }

    #[test]
    fn test_build_meta_prompt_with_style() {
        let opts = PromptOptions {
            description: Some("sunset".to_string()),
            style: Some("watercolor".to_string()),
            aspect_ratio: Some("16:9".to_string()),
            variations: 3,
            ..Default::default()
        };
        let prompt = build_meta_prompt(&opts);
        assert!(prompt.contains("watercolor"));
        assert!(prompt.contains("16:9"));
        assert!(prompt.contains("3 different prompt variations"));
    }
}
