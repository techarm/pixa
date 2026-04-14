use anyhow::{Context, Result};
use clap::Args;
use pixa::transparent::{self, TransparentOptions};
use std::path::{Path, PathBuf};

use super::style::{arrow, bold, cyan, dim, fail_mark, green, ok_mark, red, yellow};
use super::{collect_inputs, ensure_parent, format_size, mirror_path};

#[derive(Args)]
pub struct TransparentArgs {
    /// Input image file or directory
    pub input: PathBuf,
    /// Output file or directory. Defaults to `<input>.transparent.png`
    /// (file) or `<input>.transparent/` (directory).
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Recursively process directories
    #[arg(short, long)]
    pub recursive: bool,
    /// Background color to key out as `#RRGGBB`. If omitted, the color
    /// is auto-detected from the image's corner patches.
    #[arg(long)]
    pub bg: Option<String>,
    /// RGB-space distance floor: pixels within this distance of the
    /// background color are forced to fully transparent. Useful for
    /// JPEG-compressed or slightly noisy backgrounds.
    #[arg(long, default_value = "12")]
    pub tolerance: f64,
    /// Width of the soft anti-aliased edge ring (RGB distance) beyond
    /// `--tolerance`. Pixels inside this ring get per-pixel alpha and
    /// background spill removal; pixels beyond it stay fully opaque.
    #[arg(long, default_value = "90")]
    pub edge_width: f64,
}

pub fn run(args: TransparentArgs) -> Result<()> {
    let bg_override = match &args.bg {
        Some(s) => Some(
            transparent::parse_hex_color(s)
                .ok_or_else(|| anyhow::anyhow!("Invalid --bg color (expected #RRGGBB): {s}"))?,
        ),
        None => None,
    };

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
    let mut failed = 0u32;
    let mut total_in = 0u64;
    let mut total_out = 0u64;

    for input in &inputs {
        let out_path = resolve_output(&args, input, input_root, inputs.len() == 1);

        match process_one(
            input,
            &out_path,
            bg_override,
            args.tolerance,
            args.edge_width,
        ) {
            Ok(report) => {
                ok += 1;
                total_in += report.in_size;
                total_out += report.out_size;
                print_report(input, &out_path, &report, inputs.len() > 1);
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

    if inputs.len() > 1 {
        let parts = [
            (ok, "ok", green as fn(&str) -> String),
            (failed, "failed", red as fn(&str) -> String),
        ];
        let msg: Vec<String> = parts
            .iter()
            .filter(|(n, _, _)| *n > 0)
            .map(|(n, label, col)| col(&format!("{n} {label}")))
            .collect();
        println!(
            "\n{}  {}  {} → {}",
            bold("Summary"),
            msg.join(", "),
            dim(&format_size(total_in)),
            dim(&format_size(total_out)),
        );
    }

    Ok(())
}

struct Report {
    background: [u8; 3],
    transparent_pixels: u64,
    edge_pixels: u64,
    opaque_pixels: u64,
    in_size: u64,
    out_size: u64,
}

fn process_one(
    input: &Path,
    output: &Path,
    bg: Option<[u8; 3]>,
    tolerance: f64,
    edge_width: f64,
) -> Result<Report> {
    let img = image::open(input).with_context(|| format!("Failed to open: {}", input.display()))?;

    let opts = TransparentOptions {
        background: bg,
        tolerance,
        edge_width,
    };
    let (rgba, result) = transparent::apply_transparency(&img, &opts)
        .with_context(|| format!("Failed to key out background: {}", input.display()))?;

    ensure_parent(output)?;
    rgba.save(output)
        .with_context(|| format!("Failed to save: {}", output.display()))?;

    let in_size = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    let out_size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    Ok(Report {
        background: result.background,
        transparent_pixels: result.transparent_pixels,
        edge_pixels: result.edge_pixels,
        opaque_pixels: result.opaque_pixels,
        in_size,
        out_size,
    })
}

fn print_report(input: &Path, output: &Path, r: &Report, batch: bool) {
    if batch {
        println!(
            "{} {} {} {}",
            ok_mark(),
            green(&input.display().to_string()),
            arrow(),
            dim(&output.display().to_string()),
        );
        return;
    }

    let bg_hex = format!(
        "#{:02x}{:02x}{:02x}",
        r.background[0], r.background[1], r.background[2]
    );
    let total = r.transparent_pixels + r.edge_pixels + r.opaque_pixels;
    let pct = |n: u64| {
        if total == 0 {
            0.0
        } else {
            n as f64 / total as f64 * 100.0
        }
    };

    println!("{} background   {}", ok_mark(), cyan(&bg_hex));
    println!(
        "{} transparent  {} {}",
        ok_mark(),
        red(&format!("{:.1}%", pct(r.transparent_pixels))),
        dim(&format!("({} px)", r.transparent_pixels)),
    );
    println!(
        "{} edge         {} {}",
        ok_mark(),
        red(&format!("{:.1}%", pct(r.edge_pixels))),
        dim(&format!("({} px)", r.edge_pixels)),
    );
    println!(
        "{} opaque       {} {}",
        ok_mark(),
        red(&format!("{:.1}%", pct(r.opaque_pixels))),
        dim(&format!("({} px)", r.opaque_pixels)),
    );
    println!(
        "\nsaved to {} {}",
        green(&output.display().to_string()),
        dim(&format!(
            "({} → {})",
            format_size(r.in_size),
            format_size(r.out_size)
        )),
    );
}

/// Produce the output path for a given input.
///
/// - If `--output` points at a file (single-input case), use it as-is.
/// - If `--output` points at a directory (or batch mode), mirror the
///   input's relative location under it and force a `.png` extension.
/// - If `--output` is omitted, write to `<input>.transparent.png`
///   (single file) or mirror into `<input>.transparent/` (directory).
fn resolve_output(
    args: &TransparentArgs,
    input: &Path,
    input_root: &Path,
    single_file: bool,
) -> PathBuf {
    if single_file && !args.input.is_dir() {
        if let Some(out) = &args.output {
            return force_png(out.clone());
        }
        return default_sibling(input);
    }

    let root = match &args.output {
        Some(o) => o.clone(),
        None => default_dir_for(&args.input),
    };
    force_png(mirror_path(input, input_root, Some(&root)))
}

fn default_sibling(input: &Path) -> PathBuf {
    let parent = input.parent().unwrap_or(Path::new("."));
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".to_string());
    parent.join(format!("{stem}.transparent.png"))
}

fn default_dir_for(input_dir: &Path) -> PathBuf {
    let parent = input_dir.parent().unwrap_or(Path::new("."));
    let name = input_dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "images".to_string());
    parent.join(format!("{name}.transparent"))
}

fn force_png(p: PathBuf) -> PathBuf {
    p.with_extension("png")
}
