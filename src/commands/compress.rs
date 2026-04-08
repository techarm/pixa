use anyhow::Result;
use clap::Args;
use pixa::compress::compress_image;
use std::path::{Path, PathBuf};

use super::style::{arrow, bold, dim, fail_mark, green, ok_mark, red, skip_mark, yellow};
use super::{collect_inputs, ensure_parent, format_size, mirror_path};

#[derive(Args)]
pub struct CompressArgs {
    /// Input image file or directory
    pub input: PathBuf,
    /// Output file or directory. If omitted, writes alongside the
    /// input with a `.min` suffix (file) or to a sibling
    /// `<input>.min` directory (directory).
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Recursively process directories
    #[arg(short, long)]
    pub recursive: bool,
    /// Resize so the longest edge is at most this many pixels
    /// (preserves aspect ratio). Useful for shrinking AI-generated
    /// 4K images down to web-friendly sizes.
    #[arg(long, value_name = "PIXELS")]
    pub max: Option<u32>,
}

pub fn run(args: CompressArgs) -> Result<()> {
    let inputs = collect_inputs(&args.input, args.recursive)?;
    if inputs.is_empty() {
        println!("{} No images found.", yellow("!"));
        return Ok(());
    }

    let single_file = inputs.len() == 1 && !args.input.is_dir();

    if single_file {
        let out_path = args
            .output
            .clone()
            .unwrap_or_else(|| default_file_output(&inputs[0]));
        ensure_parent(&out_path)?;
        process_one(&inputs[0], &out_path, args.max)?;
        return Ok(());
    }

    // Directory mode.
    let output_root = args
        .output
        .clone()
        .unwrap_or_else(|| default_dir_output(&args.input));
    let input_root = args.input.as_path();

    let mut ok = 0u32;
    let mut failed = 0u32;
    let mut total_orig = 0u64;
    let mut total_comp = 0u64;

    for input in &inputs {
        let out_path = mirror_path(input, input_root, Some(&output_root));
        if let Err(e) = ensure_parent(&out_path) {
            eprintln!("{} {}: {e}", fail_mark(), input.display());
            failed += 1;
            continue;
        }
        match compress_image(input, &out_path, args.max) {
            Ok(r) => {
                ok += 1;
                total_orig += r.original_size;
                total_comp += r.compressed_size;
                print_line(input, &out_path, &r);
            }
            Err(e) => {
                failed += 1;
                eprintln!(
                    "{} {}: {}",
                    fail_mark(),
                    input.display(),
                    red(&e.to_string())
                );
            }
        }
    }

    print_summary(ok, failed);
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

fn process_one(input: &Path, output: &Path, max_edge: Option<u32>) -> Result<()> {
    match compress_image(input, output, max_edge) {
        Ok(r) => {
            print_line(input, output, &r);
            Ok(())
        }
        Err(e) => {
            eprintln!("{} {}: {}", fail_mark(), input.display(), red(&e.to_string()));
            Err(anyhow::anyhow!(e))
        }
    }
}

fn print_line(input: &Path, output: &Path, r: &pixa::compress::CompressResult) {
    if r.kept_original {
        println!(
            "{} {} {} {}  {}  {}",
            skip_mark(),
            input.display(),
            arrow(),
            output.display(),
            format_size(r.original_size),
            dim("(already optimal, kept original)"),
        );
    } else {
        println!(
            "{} {} {} {}  {} {} {}  {}",
            ok_mark(),
            bold(&input.display().to_string()),
            arrow(),
            bold(&output.display().to_string()),
            format_size(r.original_size),
            arrow(),
            format_size(r.compressed_size),
            green(&format!("-{:.1}%", r.savings_percent)),
        );
    }
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

/// `photo.jpg` → `photo.min.jpg`
fn default_file_output(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".to_string());
    let ext = input
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();
    let name = if ext.is_empty() {
        format!("{stem}.min")
    } else {
        format!("{stem}.min.{ext}")
    };
    input.with_file_name(name)
}

/// `./photos/` → `./photos.min/`
fn default_dir_output(input: &Path) -> PathBuf {
    let name = input
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "out".to_string());
    input.with_file_name(format!("{name}.min"))
}
