use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use pixa_core::{
    compress::{compress_image, CompressOptions},
    convert::convert_image,
    info::get_image_info,
    prompt::{self, PromptLanguage, PromptOptions, Provider},
    watermark::{WatermarkEngine, WatermarkSize},
};
use std::path::PathBuf;
use tracing::info;

#[derive(Parser)]
#[command(name = "pixa", version, about = "Image processing toolkit")]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Remove Gemini watermark from images
    #[command(alias = "rw")]
    RemoveWatermark {
        /// Input image file
        input: PathBuf,
        /// Output image file (default: overwrites input)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Force watermark size (auto-detect if omitted)
        #[arg(long, value_parser = ["small", "large"])]
        force_size: Option<String>,
        /// Run detection first and skip if no watermark found
        #[arg(long)]
        detect: bool,
        /// Detection confidence threshold (0.0-1.0)
        #[arg(long, default_value = "0.35")]
        threshold: f32,
    },

    /// Detect if a Gemini watermark is present
    Detect {
        /// Input image file
        input: PathBuf,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Compress/optimize an image
    Compress {
        /// Input image file
        input: PathBuf,
        /// Output image file
        #[arg(short, long)]
        output: PathBuf,
        /// JPEG quality (1-100)
        #[arg(short, long, default_value = "80")]
        quality: u8,
        /// Maximum width (preserves aspect ratio)
        #[arg(long)]
        max_width: Option<u32>,
        /// Maximum height (preserves aspect ratio)
        #[arg(long)]
        max_height: Option<u32>,
        /// Strip metadata
        #[arg(long, default_value = "true")]
        strip_metadata: bool,
    },

    /// Convert image format
    Convert {
        /// Input image file
        input: PathBuf,
        /// Output image file (format determined by extension)
        output: PathBuf,
    },

    /// Display image information
    Info {
        /// Input image file
        input: PathBuf,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Batch process images in a directory
    Batch {
        /// Input directory
        input_dir: PathBuf,
        /// Output directory
        #[arg(short, long)]
        output_dir: PathBuf,
        /// Operation to perform
        #[arg(short, long, value_parser = ["remove-watermark", "compress", "convert"])]
        operation: String,
        /// Target format for convert operation
        #[arg(long)]
        format: Option<String>,
        /// JPEG quality for compress operation
        #[arg(short, long, default_value = "80")]
        quality: u8,
    },

    /// Generate Nanobanana-optimized image generation prompts via local AI CLI
    Prompt {
        /// Text description (e.g., "猫が宇宙で浮いてる絵")
        description: Option<String>,
        /// Reference image path
        #[arg(short, long)]
        image: Option<PathBuf>,
        /// AI provider to use
        #[arg(short, long, value_parser = ["claude", "gemini"])]
        provider: Option<String>,
        /// Art style (e.g., "anime", "photorealistic", "watercolor")
        #[arg(short, long)]
        style: Option<String>,
        /// Aspect ratio (e.g., "16:9", "1:1")
        #[arg(long)]
        ratio: Option<String>,
        /// Number of prompt variations to generate
        #[arg(short = 'n', long, default_value = "1")]
        variations: u8,
        /// Additional instructions
        #[arg(long)]
        extra: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// List available AI providers
        #[arg(long)]
        list_providers: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging
    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .with_target(false)
        .init();

    match cli.command {
        Commands::RemoveWatermark {
            input,
            output,
            force_size,
            detect,
            threshold,
        } => {
            let engine = WatermarkEngine::new()?;
            let mut img = image::open(&input)
                .with_context(|| format!("Failed to open: {}", input.display()))?;

            let size = force_size.map(|s| match s.as_str() {
                "small" => WatermarkSize::Small,
                _ => WatermarkSize::Large,
            });

            if detect {
                let result = engine.detect_watermark(&img, size);
                if !result.detected && result.confidence < threshold {
                    println!(
                        "No watermark detected (confidence: {:.0}%), skipping.",
                        result.confidence * 100.0
                    );
                    return Ok(());
                }
                println!(
                    "Watermark detected (confidence: {:.0}%), removing...",
                    result.confidence * 100.0
                );
            }

            engine.remove_watermark(&mut img, size)?;

            let out_path = output.unwrap_or_else(|| input.clone());
            img.save(&out_path)
                .with_context(|| format!("Failed to save: {}", out_path.display()))?;
            println!("Watermark removed: {}", out_path.display());
        }

        Commands::Detect { input, json } => {
            let engine = WatermarkEngine::new()?;
            let img = image::open(&input)?;
            let result = engine.detect_watermark(&img, None);

            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("File: {}", input.display());
                println!(
                    "Watermark: {}",
                    if result.detected {
                        "DETECTED"
                    } else {
                        "Not detected"
                    }
                );
                println!("Confidence: {:.1}%", result.confidence * 100.0);
                println!("Size: {:?}", result.size);
                println!("Spatial score: {:.3}", result.spatial_score);
                println!("Gradient score: {:.3}", result.gradient_score);
                println!("Variance score: {:.3}", result.variance_score);
            }
        }

        Commands::Compress {
            input,
            output,
            quality,
            max_width,
            max_height,
            strip_metadata,
        } => {
            let opts = CompressOptions {
                jpeg_quality: quality,
                png_level: 4,
                webp_quality: quality,
                max_width,
                max_height,
                strip_metadata,
            };
            let result = compress_image(&input, &output, &opts)?;
            println!("Compressed: {} -> {}", input.display(), output.display());
            println!(
                "Size: {} -> {} ({:.1}% savings)",
                format_size(result.original_size),
                format_size(result.compressed_size),
                result.savings_percent
            );
        }

        Commands::Convert { input, output } => {
            convert_image(&input, &output)?;
            println!("Converted: {} -> {}", input.display(), output.display());
        }

        Commands::Info { input, json } => {
            let info = get_image_info(&input)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&info)?);
            } else {
                println!("File:       {}", info.file_name);
                println!("Format:     {}", info.format);
                println!("Size:       {} ({})", info.file_size_human, info.file_size);
                println!("Dimensions: {}×{}", info.width, info.height);
                println!("Pixels:     {}", info.pixel_count);
                println!("Color:      {}", info.color_type);
                println!("Bit depth:  {}", info.bit_depth);
                println!("Alpha:      {}", info.has_alpha);
                println!("SHA-256:    {}", info.sha256);
                if let Some(exif) = &info.exif {
                    println!("\nEXIF ({} fields):", exif.len());
                    let mut entries: Vec<_> = exif.iter().collect();
                    entries.sort_by_key(|(k, _)| k.clone());
                    for (key, value) in entries {
                        println!("  {key}: {value}");
                    }
                }
            }
        }

        Commands::Batch {
            input_dir,
            output_dir,
            operation,
            format,
            quality,
        } => {
            std::fs::create_dir_all(&output_dir)?;
            let entries = collect_images(&input_dir)?;
            println!("Found {} images in {}", entries.len(), input_dir.display());

            let engine = if operation == "remove-watermark" {
                Some(WatermarkEngine::new()?)
            } else {
                None
            };

            let mut success = 0;
            let mut failed = 0;

            for entry in &entries {
                let file_name = entry.file_name().unwrap();
                let mut out_path = output_dir.join(file_name);

                // Change extension for convert operation
                if operation == "convert" {
                    if let Some(fmt) = &format {
                        out_path.set_extension(fmt);
                    }
                }

                let result: Result<()> = match operation.as_str() {
                    "remove-watermark" => {
                        let engine = engine.as_ref().unwrap();
                        let mut img = image::open(entry)?;
                        engine.remove_watermark(&mut img, None)?;
                        img.save(&out_path).map_err(Into::into)
                    }
                    "compress" => {
                        let opts = CompressOptions {
                            jpeg_quality: quality,
                            webp_quality: quality,
                            ..Default::default()
                        };
                        compress_image(entry, &out_path, &opts)
                            .map(|_| ())
                            .map_err(Into::into)
                    }
                    "convert" => convert_image(entry, &out_path).map_err(Into::into),
                    _ => unreachable!(),
                };

                match result {
                    Ok(()) => {
                        success += 1;
                        info!("OK: {}", file_name.to_string_lossy());
                    }
                    Err(e) => {
                        failed += 1;
                        eprintln!("FAIL: {}: {e}", file_name.to_string_lossy());
                    }
                }
            }

            println!("\nDone: {success} succeeded, {failed} failed");
        }

        Commands::Prompt {
            description,
            image,
            provider,
            style,
            ratio,
            variations,
            extra,
            json,
            list_providers,
        } => {
            if list_providers {
                let available = prompt::detect_available_providers();
                if available.is_empty() {
                    println!("利用可能な AI CLI が見つかりません。");
                    println!("以下のいずれかをインストールしてください:");
                    println!("  - claude: https://docs.anthropic.com/en/docs/claude-code");
                    println!("  - gemini: https://github.com/google-gemini/gemini-cli");
                } else {
                    println!("利用可能なプロバイダー:");
                    for p in &available {
                        println!("  ✓ {}", p.display_name());
                    }
                }
                return Ok(());
            }

            // 入力チェック
            if description.is_none() && image.is_none() {
                anyhow::bail!(
                    "テキスト指示か参考画像のどちらかを指定してください。\n\
                     例: pixa prompt \"猫が宇宙で浮いてる\"\n\
                     例: pixa prompt --image ref.jpg\n\
                     例: pixa prompt \"サイバーパンク風\" --image ref.jpg"
                );
            }

            // プロバイダー決定
            let provider: Provider = if let Some(p) = provider {
                p.parse().map_err(|e: String| anyhow::anyhow!(e))?
            } else {
                // 自動検出: 最初に見つかったものを使用
                let available = prompt::detect_available_providers();
                *available.first().ok_or_else(|| {
                    anyhow::anyhow!(
                        "AI CLI が見つかりません。--provider で指定するか、claude / gemini CLI をインストールしてください。"
                    )
                })?
            };

            let opts = PromptOptions {
                description,
                reference_image: image,
                style,
                aspect_ratio: ratio,
                extra_instructions: extra,
                variations,
                language: PromptLanguage::English,
            };

            let result = prompt::generate_prompt(provider, &opts)?;

            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                if result.prompts.len() == 1 {
                    println!("{}", result.prompts[0]);
                } else {
                    for (i, p) in result.prompts.iter().enumerate() {
                        if i > 0 {
                            println!();
                        }
                        println!("--- Variation {} ---", i + 1);
                        println!("{p}");
                    }
                }
            }
        }
    }

    Ok(())
}

fn collect_images(dir: &PathBuf) -> Result<Vec<PathBuf>> {
    let extensions = ["jpg", "jpeg", "png", "webp", "bmp", "gif", "tiff", "tif"];
    let mut files = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if extensions.contains(&ext.to_lowercase().as_str()) {
                    files.push(path);
                }
            }
        }
    }

    files.sort();
    Ok(files)
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
