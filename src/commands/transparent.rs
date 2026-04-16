use anyhow::{Context, Result};
use clap::Args;
use image::DynamicImage;
use pixa::transparent::{self, TransparentOptions};
use std::path::{Path, PathBuf};

use super::ImageSource;
use super::style::{arrow, bold, cyan, dim, fail_mark, green, ok_mark, red, yellow};
use super::{
    collect_inputs, ensure_parent, format_size, guard_clipboard_not_directory, mirror_path,
};

#[derive(Args)]
pub struct TransparentArgs {
    /// Input image file or directory, or @clipboard to read from the OS clipboard
    pub input: PathBuf,
    /// Output file or directory. Defaults to `<input>.transparent.png`
    /// (file) or `<input>.transparent/` (directory).
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Recursively process directories
    #[arg(short, long)]
    pub recursive: bool,
    /// Background color to key out, as `#RRGGBB` or `RRGGBB`. If
    /// omitted, the color is auto-detected from the image's corner
    /// patches.
    #[arg(long)]
    pub bg: Option<String>,
    /// RGB-space distance from the detected background colour at or
    /// below which a pixel is treated as background. Wider picks up
    /// more of the AA contamination ring at the subject's outer edge;
    /// too wide and pastel/near-bg subject regions start dissolving.
    #[arg(long, default_value = "200")]
    pub tolerance: f64,
    /// Enable channel-based spill suppression on the edge band:
    /// neutralises bg-colour contamination on AA edges while keeping
    /// alpha and interior pixels untouched. Lets you use prettier AI
    /// prompts (softer outlines) without a visible pink/magenta ring.
    #[arg(long)]
    pub despill: bool,
    /// Edge-band radius (in pixels) for `--despill`. Ignored otherwise.
    #[arg(long, default_value = "3", requires = "despill")]
    pub despill_band: u32,
    /// Morphologically erode the opaque region by this many pixels
    /// after flood. Useful when the AA contamination ring is too
    /// wide to clean up cosmetically and the silhouette tolerating a
    /// slight shrink is acceptable.
    #[arg(long, default_value = "0")]
    pub shrink: u32,
}

pub fn run(args: TransparentArgs) -> Result<()> {
    let bg_override = match &args.bg {
        Some(s) => Some(transparent::parse_hex_color(s).ok_or_else(|| {
            anyhow::anyhow!("Invalid --bg color (expected #RRGGBB or RRGGBB): {s}")
        })?),
        None => None,
    };

    let source = ImageSource::parse(&args.input);
    guard_clipboard_not_directory(&source, args.recursive)?;

    if source.is_clipboard() {
        let out_path = args
            .output
            .clone()
            .ok_or_else(|| anyhow::anyhow!("--output is required when input is @clipboard"))?;
        let out_path = force_png(out_path);
        let img = source.load_image()?;
        let report = process_dynamic(
            &img,
            &out_path,
            bg_override,
            args.tolerance,
            args.despill,
            args.despill_band,
            args.shrink,
        )?;
        print_clipboard_report(&out_path, &report);
        return Ok(());
    }

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
            args.despill,
            args.despill_band,
            args.shrink,
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
    opaque_pixels: u64,
    in_size: u64,
    out_size: u64,
}

#[allow(clippy::too_many_arguments)]
fn process_one(
    input: &Path,
    output: &Path,
    bg: Option<[u8; 3]>,
    tolerance: f64,
    despill: bool,
    despill_band: u32,
    shrink: u32,
) -> Result<Report> {
    let img = image::open(input).with_context(|| format!("Failed to open: {}", input.display()))?;

    let opts = TransparentOptions {
        background: bg,
        tolerance,
        despill,
        despill_band,
        shrink,
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
        opaque_pixels: result.opaque_pixels,
        in_size,
        out_size,
    })
}

#[allow(clippy::too_many_arguments)]
fn process_dynamic(
    img: &DynamicImage,
    output: &Path,
    bg: Option<[u8; 3]>,
    tolerance: f64,
    despill: bool,
    despill_band: u32,
    shrink: u32,
) -> Result<Report> {
    let opts = TransparentOptions {
        background: bg,
        tolerance,
        despill,
        despill_band,
        shrink,
    };
    let (rgba, result) = transparent::apply_transparency(img, &opts)
        .context("Failed to key out background from @clipboard")?;

    ensure_parent(output)?;
    rgba.save(output)
        .with_context(|| format!("Failed to save: {}", output.display()))?;

    let out_size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    Ok(Report {
        background: result.background,
        transparent_pixels: result.transparent_pixels,
        opaque_pixels: result.opaque_pixels,
        in_size: 0,
        out_size,
    })
}

fn print_clipboard_report(output: &Path, r: &Report) {
    let bg_hex = format!(
        "#{:02x}{:02x}{:02x}",
        r.background[0], r.background[1], r.background[2]
    );
    let total = r.transparent_pixels + r.opaque_pixels;
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
        "{} opaque       {} {}",
        ok_mark(),
        red(&format!("{:.1}%", pct(r.opaque_pixels))),
        dim(&format!("({} px)", r.opaque_pixels)),
    );
    println!(
        "\nsaved to {} {}",
        green(&output.display().to_string()),
        dim(&format!("(@clipboard → {})", format_size(r.out_size))),
    );
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
    let total = r.transparent_pixels + r.opaque_pixels;
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

/// Produce the output path for a given input. Transparency requires
/// an alpha channel, so the returned path always has a `.png`
/// extension — any other extension on `--output` is coerced.
///
/// - If `--output` points at a file (single-input case), use it with
///   the extension forced to `.png`.
/// - If `--output` points at a directory (or batch mode), mirror the
///   input's relative location under it with extension forced to
///   `.png`.
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
