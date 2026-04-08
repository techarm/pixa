use anyhow::Result;
use clap::Args;
use pixa::compress::{compress_image, CompressOptions};
use std::path::PathBuf;

use super::style::{arrow, bold, dim, fail_mark, green, ok_mark, red, yellow};
use super::{collect_inputs, ensure_parent, format_size, mirror_path};

#[derive(Args)]
pub struct CompressArgs {
    /// Input image file or directory
    pub input: PathBuf,
    /// Output file (single input) or directory (recursive)
    #[arg(short, long)]
    pub output: PathBuf,
    /// Recursively process directories
    #[arg(short, long)]
    pub recursive: bool,
    /// JPEG / WebP quality (1-100)
    #[arg(short, long, default_value = "80")]
    pub quality: u8,
    /// Maximum width (preserves aspect ratio)
    #[arg(long)]
    pub max_width: Option<u32>,
    /// Maximum height (preserves aspect ratio)
    #[arg(long)]
    pub max_height: Option<u32>,
    /// Keep metadata (EXIF is stripped by default)
    #[arg(long)]
    pub keep_metadata: bool,
}

pub fn run(args: CompressArgs) -> Result<()> {
    let opts = CompressOptions {
        jpeg_quality: args.quality,
        png_level: 4,
        webp_quality: args.quality,
        max_width: args.max_width,
        max_height: args.max_height,
        strip_metadata: !args.keep_metadata,
    };

    let inputs = collect_inputs(&args.input, args.recursive)?;
    if inputs.is_empty() {
        println!("{} No images found.", yellow("!"));
        return Ok(());
    }

    let single_file = inputs.len() == 1 && !args.input.is_dir();

    if single_file {
        let result = compress_image(&inputs[0], &args.output, &opts)?;
        let savings = format!("-{:.1}%", result.savings_percent);
        println!(
            "{} {} {} {}  {} {} {}  {}",
            ok_mark(),
            bold(&inputs[0].display().to_string()),
            arrow(),
            bold(&args.output.display().to_string()),
            format_size(result.original_size),
            arrow(),
            format_size(result.compressed_size),
            green(&savings),
        );
        return Ok(());
    }

    let input_root = args.input.as_path();
    let mut success = 0;
    let mut failed = 0;
    let mut total_orig = 0u64;
    let mut total_comp = 0u64;

    for input in &inputs {
        let out_path = mirror_path(input, input_root, Some(&args.output));
        if let Err(e) = ensure_parent(&out_path) {
            eprintln!("{} {}: {e}", fail_mark(), input.display());
            failed += 1;
            continue;
        }
        match compress_image(input, &out_path, &opts) {
            Ok(r) => {
                success += 1;
                total_orig += r.original_size;
                total_comp += r.compressed_size;
                println!(
                    "{} {} {}",
                    ok_mark(),
                    input.display(),
                    dim(&format!(
                        "{} → {} (-{:.1}%)",
                        format_size(r.original_size),
                        format_size(r.compressed_size),
                        r.savings_percent
                    )),
                );
            }
            Err(e) => {
                failed += 1;
                eprintln!("{} {}: {}", fail_mark(), input.display(), red(&e.to_string()));
            }
        }
    }

    print_summary(success, failed);
    if total_orig > 0 {
        let pct = (1.0 - total_comp as f64 / total_orig as f64) * 100.0;
        println!(
            "{} {} {} {}  {}",
            dim("total"),
            format_size(total_orig),
            arrow(),
            format_size(total_comp),
            green(&format!("-{pct:.1}%")),
        );
    }
    Ok(())
}

fn print_summary(ok: u32, failed: u32) {
    let parts = [
        (ok, "ok", green as fn(&str) -> String),
        (failed, "failed", red as fn(&str) -> String),
    ];
    let msg: Vec<String> = parts
        .iter()
        .filter(|(n, _, _)| *n > 0)
        .map(|(n, label, col)| col(&format!("{n} {label}")))
        .collect();
    println!("\n{}  {}", bold("Summary"), msg.join(", "));
}
