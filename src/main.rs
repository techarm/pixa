mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};

use commands::{compress, convert, detect, favicon, info, remove_watermark, split};

const EXAMPLES: &str = "\
Examples:
  pixa remove-watermark image.jpg -o clean.jpg
  pixa remove-watermark ./photos -r -o ./cleaned --if-detected
  pixa detect image.jpg
  pixa compress input.png -o output.png -q 80
  pixa compress ./photos -r -o ./out -q 85
  pixa convert photo.png photo.webp
  pixa convert ./photos ./out -r --format webp
  pixa info photo.jpg
  pixa favicon logo.png -o ./favicon
  pixa split sheet.png -o ./out --names neutral,happy,thinking,surprised,sad
";

#[derive(Parser)]
#[command(
    name = "pixa",
    version,
    about = "A fast image processing toolkit",
    after_help = EXAMPLES,
)]
struct Cli {
    /// Enable verbose (debug) logging
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Remove Gemini watermark from an image or directory
    #[command(alias = "rw")]
    RemoveWatermark(remove_watermark::RemoveWatermarkArgs),

    /// Detect whether a Gemini watermark is present
    Detect(detect::DetectArgs),

    /// Compress / optimize images (MozJPEG, OxiPNG, WebP)
    Compress(compress::CompressArgs),

    /// Convert between image formats
    Convert(convert::ConvertArgs),

    /// Show image metadata and dimensions
    Info(info::InfoArgs),

    /// Generate a web-ready favicon/icon set from an image
    Favicon(favicon::FaviconArgs),

    /// Auto-detect and crop individual objects from a sheet image
    Split(split::SplitArgs),
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let filter = if cli.verbose { "debug" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .with_target(false)
        .without_time()
        .with_writer(std::io::stderr)
        .init();

    match cli.command {
        Commands::RemoveWatermark(a) => remove_watermark::run(a),
        Commands::Detect(a) => detect::run(a),
        Commands::Compress(a) => compress::run(a),
        Commands::Convert(a) => convert::run(a),
        Commands::Info(a) => info::run(a),
        Commands::Favicon(a) => favicon::run(a),
        Commands::Split(a) => split::run(a),
    }
}
