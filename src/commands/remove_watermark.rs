use anyhow::{Context, Result};
use clap::Args;
use pixa::watermark::{WatermarkEngine, WatermarkSize};
use std::path::{Path, PathBuf};

use super::style::{arrow, bold, dim, fail_mark, green, ok_mark, red, skip_mark, yellow};
use super::{collect_inputs, ensure_parent, mirror_path};

#[derive(Args)]
pub struct RemoveWatermarkArgs {
    /// Input image file or directory
    pub input: PathBuf,
    /// Output file or directory (defaults to overwriting input)
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Recursively process directories
    #[arg(short, long)]
    pub recursive: bool,
    /// Force watermark size (auto-detect if omitted)
    #[arg(long, value_enum)]
    pub force_size: Option<SizeArg>,
    /// Run detection first and skip images with no watermark
    #[arg(long)]
    pub if_detected: bool,
    /// Detection confidence threshold (0.0-1.0)
    #[arg(long, default_value = "0.35")]
    pub threshold: f32,
}

#[derive(Copy, Clone, clap::ValueEnum)]
pub enum SizeArg {
    Small,
    Large,
}

impl From<SizeArg> for WatermarkSize {
    fn from(a: SizeArg) -> Self {
        match a {
            SizeArg::Small => WatermarkSize::Small,
            SizeArg::Large => WatermarkSize::Large,
        }
    }
}

pub fn run(args: RemoveWatermarkArgs) -> Result<()> {
    let engine = WatermarkEngine::new()?;
    let size = args.force_size.map(Into::into);
    let inputs = collect_inputs(&args.input, args.recursive)?;

    if inputs.is_empty() {
        println!("{} No images found.", yellow("!"));
        return Ok(());
    }

    let input_root = if args.input.is_dir() {
        args.input.as_path()
    } else {
        args.input.parent().unwrap_or(args.input.as_path())
    };

    let mut ok = 0u32;
    let mut skipped = 0u32;
    let mut failed = 0u32;

    for input in &inputs {
        let out_path = if inputs.len() == 1 && !args.input.is_dir() {
            args.output.clone().unwrap_or_else(|| input.clone())
        } else {
            mirror_path(input, input_root, args.output.as_deref())
        };

        match process_one(&engine, input, &out_path, size, args.if_detected, args.threshold) {
            Ok(true) => {
                ok += 1;
                println!(
                    "{} {} {} {}",
                    ok_mark(),
                    input.display(),
                    arrow(),
                    dim(&out_path.display().to_string())
                );
            }
            Ok(false) => {
                skipped += 1;
                println!("{} {} {}", skip_mark(), input.display(), dim("(no watermark)"));
            }
            Err(e) => {
                failed += 1;
                eprintln!("{} {}: {}", fail_mark(), input.display(), red(&e.to_string()));
            }
        }
    }

    if inputs.len() > 1 {
        let parts = [
            (ok, "ok", green as fn(&str) -> String),
            (skipped, "skipped", yellow as fn(&str) -> String),
            (failed, "failed", red as fn(&str) -> String),
        ];
        let msg: Vec<String> = parts
            .iter()
            .filter(|(n, _, _)| *n > 0)
            .map(|(n, label, col)| col(&format!("{n} {label}")))
            .collect();
        println!("\n{}  {}", bold("Summary"), msg.join(", "));
    }
    Ok(())
}

fn process_one(
    engine: &WatermarkEngine,
    input: &Path,
    output: &Path,
    size: Option<WatermarkSize>,
    detect_first: bool,
    threshold: f32,
) -> Result<bool> {
    let mut img =
        image::open(input).with_context(|| format!("Failed to open: {}", input.display()))?;

    if detect_first {
        let result = engine.detect_watermark(&img, size);
        if !result.detected && result.confidence < threshold {
            return Ok(false);
        }
    }

    engine.remove_watermark(&mut img, size)?;
    ensure_parent(output)?;
    img.save(output)
        .with_context(|| format!("Failed to save: {}", output.display()))?;
    Ok(true)
}
