use anyhow::Result;
use clap::Args;
use pixa::watermark::WatermarkEngine;
use std::path::PathBuf;

#[derive(Args)]
pub struct DetectArgs {
    /// Input image file
    pub input: PathBuf,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: DetectArgs) -> Result<()> {
    let engine = WatermarkEngine::new()?;
    let img = image::open(&args.input)?;
    let result = engine.detect_watermark(&img, None);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("File: {}", args.input.display());
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
    Ok(())
}
