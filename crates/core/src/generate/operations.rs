use std::path::PathBuf;

use tracing::{debug, info, warn};

use super::client::GeminiClient;
use super::config;
use super::error::GenerateError;
use super::prompt_builder;
use super::types::*;

/// Default output directory name
const OUTPUT_DIR_NAME: &str = "pixa-output";

/// Generate images from text prompt(s).
/// Supports batch generation with styles and variations.
/// Matches nanobanana generateTextToImage() - imageGenerator.ts:240-354
pub async fn generate_image(
    client: Option<&GeminiClient>,
    request: &ImageRequest,
) -> Result<GenerateResult, GenerateError> {
    let cfg = config::load_image_config()?;
    let prompts = prompt_builder::build_image_prompts(&cfg, request);

    if request.dry_run {
        return Ok(GenerateResult {
            success: true,
            message: format!("Dry run: {} prompt(s) prepared", prompts.len()),
            generated_files: vec![],
            prompts_used: prompts,
            api_calls_made: 0,
        });
    }

    let client = client.ok_or(GenerateError::ApiKeyNotFound)?;

    info!(
        "Generating {} image variation(s) via Gemini API (model: {})",
        prompts.len(),
        client.model_id()
    );

    let output_dir = resolve_output_dir(&request.output_dir)?;
    let mut generated_files = Vec::new();
    let mut api_calls = 0u32;
    let mut first_error: Option<String> = None;

    for (i, prompt) in prompts.iter().enumerate() {
        api_calls += 1;
        debug!("Generating variation {}/{}: {}", i + 1, prompts.len(), prompt);

        match client.generate_from_text(prompt).await {
            Ok(response) => {
                if let Some(image_bytes) = response.image_data {
                    // Use the current prompt for filename if styles/variations applied,
                    // otherwise use the base prompt
                    let filename_source = if !request.styles.is_empty()
                        || !request.variations.is_empty()
                    {
                        prompt.as_str()
                    } else {
                        &request.prompt
                    };
                    let filename =
                        generate_filename(filename_source, request.format, i, &output_dir);
                    let path = output_dir.join(&filename);
                    tokio::fs::write(&path, &image_bytes).await?;
                    info!("Image saved: {}", path.display());
                    generated_files.push(path);
                }
            }
            Err(e) => {
                let msg = e.to_string();
                if first_error.is_none() {
                    first_error = Some(msg.clone());
                }
                warn!("Error generating variation {}: {}", i + 1, msg);

                // Stop immediately on auth errors
                if matches!(e, GenerateError::AuthFailed(_)) {
                    return Err(e);
                }
            }
        }
    }

    if generated_files.is_empty() {
        return Err(GenerateError::NoImageInResponse);
    }

    Ok(GenerateResult {
        success: true,
        message: format!("Generated {} image(s)", generated_files.len()),
        generated_files,
        prompts_used: prompts,
        api_calls_made: api_calls,
    })
}

/// Edit an existing image with a text instruction.
/// Matches nanobanana editImage() - imageGenerator.ts:543-659
pub async fn edit_image(
    client: Option<&GeminiClient>,
    request: &EditRequest,
) -> Result<GenerateResult, GenerateError> {
    let cfg = config::load_edit_config()?;

    if !request.input.exists() {
        return Err(GenerateError::ImageNotFound(
            request.input.display().to_string(),
        ));
    }

    let prompts_used = vec![request.prompt.clone()];

    if request.dry_run {
        return Ok(GenerateResult {
            success: true,
            message: "Dry run: edit prompt prepared".into(),
            generated_files: vec![],
            prompts_used,
            api_calls_made: 0,
        });
    }

    let client = client.ok_or(GenerateError::ApiKeyNotFound)?;

    info!("Editing image: {}", request.input.display());

    let image_bytes = tokio::fs::read(&request.input).await?;
    let mime_type = GeminiClient::detect_mime_type(&request.input);

    let response = client
        .generate_with_image(&request.prompt, &image_bytes, mime_type)
        .await?;

    let output_dir = resolve_output_dir(&request.output_dir)?;
    let mut generated_files = Vec::new();

    if let Some(result_bytes) = response.image_data {
        let filename_source = format!("{}_{}", cfg.prompt.filename_prefix, request.prompt);
        let filename = generate_filename(&filename_source, request.format, 0, &output_dir);
        let path = output_dir.join(&filename);
        tokio::fs::write(&path, &result_bytes).await?;
        info!("Edited image saved: {}", path.display());
        generated_files.push(path);
    } else {
        return Err(GenerateError::NoImageInResponse);
    }

    Ok(GenerateResult {
        success: true,
        message: "Successfully edited image".into(),
        generated_files,
        prompts_used,
        api_calls_made: 1,
    })
}

/// Restore/enhance an image.
/// Uses the same API as edit (nanobanana routes restore through editImage).
pub async fn restore_image(
    client: Option<&GeminiClient>,
    request: &RestoreRequest,
) -> Result<GenerateResult, GenerateError> {
    let cfg = config::load_restore_config()?;

    if !request.input.exists() {
        return Err(GenerateError::ImageNotFound(
            request.input.display().to_string(),
        ));
    }

    let prompts_used = vec![request.prompt.clone()];

    if request.dry_run {
        return Ok(GenerateResult {
            success: true,
            message: "Dry run: restore prompt prepared".into(),
            generated_files: vec![],
            prompts_used,
            api_calls_made: 0,
        });
    }

    let client = client.ok_or(GenerateError::ApiKeyNotFound)?;

    info!("Restoring image: {}", request.input.display());

    let image_bytes = tokio::fs::read(&request.input).await?;
    let mime_type = GeminiClient::detect_mime_type(&request.input);

    let response = client
        .generate_with_image(&request.prompt, &image_bytes, mime_type)
        .await?;

    let output_dir = resolve_output_dir(&request.output_dir)?;
    let mut generated_files = Vec::new();

    if let Some(result_bytes) = response.image_data {
        let filename_source = format!("{}_{}", cfg.prompt.filename_prefix, request.prompt);
        let filename = generate_filename(&filename_source, request.format, 0, &output_dir);
        let path = output_dir.join(&filename);
        tokio::fs::write(&path, &result_bytes).await?;
        info!("Restored image saved: {}", path.display());
        generated_files.push(path);
    } else {
        return Err(GenerateError::NoImageInResponse);
    }

    Ok(GenerateResult {
        success: true,
        message: "Successfully restored image".into(),
        generated_files,
        prompts_used,
        api_calls_made: 1,
    })
}

/// Generate icons in specified sizes.
/// Matches nanobanana generate_icon handler - index.ts:462-475
pub async fn generate_icon(
    client: Option<&GeminiClient>,
    request: &IconRequest,
) -> Result<GenerateResult, GenerateError> {
    let cfg = config::load_icon_config()?;
    let prompt = prompt_builder::build_icon_prompt(&cfg, request);

    let prompts_used = vec![prompt.clone()];

    if request.dry_run {
        return Ok(GenerateResult {
            success: true,
            message: format!(
                "Dry run: icon prompt prepared for {} size(s)",
                request.sizes.len()
            ),
            generated_files: vec![],
            prompts_used,
            api_calls_made: 0,
        });
    }

    let client = client.ok_or(GenerateError::ApiKeyNotFound)?;

    info!(
        "Generating icon: {} size(s) [{:?}]",
        request.sizes.len(),
        request.sizes
    );

    let output_dir = resolve_output_dir(&request.output_dir)?;
    let mut generated_files = Vec::new();
    let mut api_calls = 0u32;

    // Generate one image per size (each size gets its own API call)
    for (i, _size) in request.sizes.iter().enumerate() {
        api_calls += 1;

        match client.generate_from_text(&prompt).await {
            Ok(response) => {
                if let Some(image_bytes) = response.image_data {
                    let filename =
                        generate_filename(&request.prompt, request.format, i, &output_dir);
                    let path = output_dir.join(&filename);
                    tokio::fs::write(&path, &image_bytes).await?;
                    info!("Icon saved: {}", path.display());
                    generated_files.push(path);
                }
            }
            Err(e) => {
                warn!("Error generating icon size {}: {}", i + 1, e);
                if matches!(e, GenerateError::AuthFailed(_)) {
                    return Err(e);
                }
            }
        }
    }

    if generated_files.is_empty() {
        return Err(GenerateError::NoImageInResponse);
    }

    Ok(GenerateResult {
        success: true,
        message: format!("Generated {} icon(s)", generated_files.len()),
        generated_files,
        prompts_used,
        api_calls_made: api_calls,
    })
}

/// Generate a seamless pattern/texture.
/// Matches nanobanana generate_pattern handler - index.ts:478-490
pub async fn generate_pattern(
    client: Option<&GeminiClient>,
    request: &PatternRequest,
) -> Result<GenerateResult, GenerateError> {
    let cfg = config::load_pattern_config()?;
    let prompt = prompt_builder::build_pattern_prompt(&cfg, request);

    let prompts_used = vec![prompt.clone()];

    if request.dry_run {
        return Ok(GenerateResult {
            success: true,
            message: "Dry run: pattern prompt prepared".into(),
            generated_files: vec![],
            prompts_used,
            api_calls_made: 0,
        });
    }

    let client = client.ok_or(GenerateError::ApiKeyNotFound)?;

    info!("Generating pattern");

    let output_dir = resolve_output_dir(&request.output_dir)?;
    let response = client.generate_from_text(&prompt).await?;

    let mut generated_files = Vec::new();
    if let Some(image_bytes) = response.image_data {
        let filename = generate_filename(&request.prompt, request.format, 0, &output_dir);
        let path = output_dir.join(&filename);
        tokio::fs::write(&path, &image_bytes).await?;
        info!("Pattern saved: {}", path.display());
        generated_files.push(path);
    } else {
        return Err(GenerateError::NoImageInResponse);
    }

    Ok(GenerateResult {
        success: true,
        message: "Generated pattern".into(),
        generated_files,
        prompts_used,
        api_calls_made: 1,
    })
}

/// Generate a sequence of related images (story/process/tutorial/timeline).
/// Matches nanobanana generateStorySequence() - imageGenerator.ts:402-542
pub async fn generate_story(
    client: Option<&GeminiClient>,
    request: &StoryRequest,
) -> Result<GenerateResult, GenerateError> {
    let cfg = config::load_story_config()?;
    let total_steps = request.steps;

    // Build all step prompts
    let prompts_used: Vec<String> = (1..=total_steps)
        .map(|step| prompt_builder::build_story_step_prompt(&cfg, request, step, total_steps))
        .collect();

    if request.dry_run {
        return Ok(GenerateResult {
            success: true,
            message: format!("Dry run: {total_steps}-step story prompts prepared"),
            generated_files: vec![],
            prompts_used,
            api_calls_made: 0,
        });
    }

    let client = client.ok_or(GenerateError::ApiKeyNotFound)?;

    info!(
        "Generating {}-step {} sequence",
        total_steps, request.story_type
    );

    let output_dir = resolve_output_dir(&request.output_dir)?;
    let mut generated_files = Vec::new();
    let mut api_calls = 0u32;
    let mut first_error: Option<String> = None;

    for (i, step_prompt) in prompts_used.iter().enumerate() {
        let step_number = i as u8 + 1;
        api_calls += 1;
        debug!("Generating step {}/{}: {}", step_number, total_steps, step_prompt);

        match client.generate_from_text(step_prompt).await {
            Ok(response) => {
                if let Some(image_bytes) = response.image_data {
                    // Filename format: {type}_step{N}_{prompt}
                    let filename_source =
                        format!("{}step{}{}", request.story_type, step_number, request.prompt);
                    let filename =
                        generate_filename(&filename_source, request.format, 0, &output_dir);
                    let path = output_dir.join(&filename);
                    tokio::fs::write(&path, &image_bytes).await?;
                    info!("Step {} saved: {}", step_number, path.display());
                    generated_files.push(path);
                } else {
                    warn!("Step {}: no valid image data received", step_number);
                }
            }
            Err(e) => {
                let msg = e.to_string();
                if first_error.is_none() {
                    first_error = Some(msg.clone());
                }
                warn!("Error generating step {}: {}", step_number, msg);
                if matches!(e, GenerateError::AuthFailed(_)) {
                    return Err(e);
                }
            }
        }
    }

    if generated_files.is_empty() {
        return Err(GenerateError::NoImageInResponse);
    }

    let message = if generated_files.len() == total_steps as usize {
        format!(
            "Successfully generated complete {}-step {} sequence",
            total_steps, request.story_type
        )
    } else {
        format!(
            "Generated {} out of {} requested {} steps ({} steps failed)",
            generated_files.len(),
            total_steps,
            request.story_type,
            total_steps as usize - generated_files.len()
        )
    };

    Ok(GenerateResult {
        success: true,
        message,
        generated_files,
        prompts_used,
        api_calls_made: api_calls,
    })
}

/// Generate a technical diagram.
/// Matches nanobanana generate_diagram handler - index.ts:510-523
pub async fn generate_diagram(
    client: Option<&GeminiClient>,
    request: &DiagramRequest,
) -> Result<GenerateResult, GenerateError> {
    let cfg = config::load_diagram_config()?;
    let prompt = prompt_builder::build_diagram_prompt(&cfg, request);

    let prompts_used = vec![prompt.clone()];

    if request.dry_run {
        return Ok(GenerateResult {
            success: true,
            message: "Dry run: diagram prompt prepared".into(),
            generated_files: vec![],
            prompts_used,
            api_calls_made: 0,
        });
    }

    let client = client.ok_or(GenerateError::ApiKeyNotFound)?;

    info!("Generating {} diagram", request.diagram_type);

    let output_dir = resolve_output_dir(&request.output_dir)?;
    let response = client.generate_from_text(&prompt).await?;

    let mut generated_files = Vec::new();
    if let Some(image_bytes) = response.image_data {
        let filename = generate_filename(&request.prompt, request.format, 0, &output_dir);
        let path = output_dir.join(&filename);
        tokio::fs::write(&path, &image_bytes).await?;
        info!("Diagram saved: {}", path.display());
        generated_files.push(path);
    } else {
        return Err(GenerateError::NoImageInResponse);
    }

    Ok(GenerateResult {
        success: true,
        message: format!("Generated {} diagram", request.diagram_type),
        generated_files,
        prompts_used,
        api_calls_made: 1,
    })
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

/// Generate a user-friendly filename from a prompt.
/// Matches nanobanana FileHandler.generateFilename() - fileHandler.ts:60-89
fn generate_filename(
    prompt: &str,
    format: OutputFormat,
    index: usize,
    output_dir: &std::path::Path,
) -> String {
    // Create filename from prompt
    let base_name: String = prompt
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || c.is_ascii_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_");

    let base_name = if base_name.is_empty() {
        "generated_image".to_string()
    } else {
        base_name.chars().take(32).collect()
    };

    let ext = format.extension();

    // Check for existing files and add counter if needed
    let mut filename = format!("{base_name}.{ext}");
    let mut counter = if index > 0 { index } else { 1 };

    while output_dir.join(&filename).exists() {
        filename = format!("{base_name}_{counter}.{ext}");
        counter += 1;
    }

    filename
}

/// Resolve output directory, creating it if needed.
fn resolve_output_dir(custom: &Option<PathBuf>) -> Result<PathBuf, GenerateError> {
    let dir = custom
        .clone()
        .unwrap_or_else(|| PathBuf::from(OUTPUT_DIR_NAME));

    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }

    Ok(dir)
}
