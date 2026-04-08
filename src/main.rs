mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};

use commands::{compress, convert, detect, favicon, info, remove_watermark};

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
    RemoveWatermark(remove_watermark::RemoveWatermarkArgs),

    /// Detect if a Gemini watermark is present
    Detect(detect::DetectArgs),

    /// Compress/optimize an image or directory
    Compress(compress::CompressArgs),

    /// Convert image format
    Convert(convert::ConvertArgs),

    /// Display image information
    Info(info::InfoArgs),

    /// Generate a web-ready favicon/icon set from any image
    Favicon(favicon::FaviconArgs),
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let filter = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .with_target(false)
        .init();

    match cli.command {
        Commands::RemoveWatermark(a) => remove_watermark::run(a),
        Commands::Detect(a) => detect::run(a),
        Commands::Compress(a) => compress::run(a),
        Commands::Convert(a) => convert::run(a),
        Commands::Info(a) => info::run(a),
        Commands::Favicon(a) => favicon::run(a),
    }
}
