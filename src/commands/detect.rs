use anyhow::Result;
use clap::Args;
use pixa::watermark::WatermarkEngine;
use std::path::PathBuf;

use super::style::{dim, fail_mark, green, ok_mark, red};

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
        return Ok(());
    }

    let (mark, status) = if result.detected {
        (ok_mark(), red("DETECTED"))
    } else {
        (fail_mark(), green("not detected"))
    };
    let pct = result.confidence * 100.0;

    println!(
        "{} {} {} {} {}",
        mark,
        green(&args.input.display().to_string()),
        dim("·"),
        status,
        dim(&format!("({pct:.1}%)"))
    );
    println!(
        "  {} size={:?} spatial={:.3} gradient={:.3} variance={:.3}",
        dim("scores"),
        result.size,
        result.spatial_score,
        result.gradient_score,
        result.variance_score
    );
    Ok(())
}
