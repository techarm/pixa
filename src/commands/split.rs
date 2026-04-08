use anyhow::{Context, Result};
use clap::Args;
use pixa::split::{self, SplitOptions};
use std::path::PathBuf;

use super::style::{arrow, bold, cyan, dim, fail_mark, ok_mark};
use super::{ensure_parent, format_size};

#[derive(Args)]
pub struct SplitArgs {
    /// Input sheet image (objects on a single-color background)
    pub input: PathBuf,
    /// Output directory for the cropped objects
    #[arg(short, long)]
    pub output: PathBuf,
    /// Comma-separated names for each object (also used as the
    /// expected count, which enables re-splitting near-touching objects)
    #[arg(long, value_delimiter = ',')]
    pub names: Vec<String>,
    /// Pixels of background padding around each crop
    #[arg(long, default_value = "0")]
    pub padding: u32,
    /// Always write a `<basename>-preview.png` next to the input
    #[arg(long)]
    pub preview: bool,
}

pub fn run(args: SplitArgs) -> Result<()> {
    let img = image::open(&args.input)
        .with_context(|| format!("Failed to open: {}", args.input.display()))?;

    let opts = SplitOptions {
        padding: args.padding,
        expected_count: if args.names.is_empty() {
            None
        } else {
            Some(args.names.len())
        },
        ..Default::default()
    };

    let result = match split::detect_objects(&img, &opts) {
        Ok(r) => r,
        Err(e) => {
            // Auto-write preview on failure to help diagnosis.
            let preview_path = preview_path(&args.input);
            // Run a no-expectation pass purely for visualization.
            if let Ok(diag) = split::detect_objects(&img, &SplitOptions::default()) {
                let _ = split::write_preview(&img, &diag, &preview_path);
                eprintln!("{} {}", fail_mark(), e);
                eprintln!("  preview written: {}", preview_path.display());
                eprintln!(
                    "  hint: try --padding or pass --names to enable re-split"
                );
            } else {
                eprintln!("{} {}", fail_mark(), e);
            }
            std::process::exit(1);
        }
    };

    let bg_hex = format!(
        "#{:02x}{:02x}{:02x}",
        result.background[0], result.background[1], result.background[2]
    );
    println!("{} background  {}", ok_mark(), cyan(&bg_hex));
    let count = result.objects.len();
    if result.resplit_used {
        println!(
            "{} detected    {} {}",
            ok_mark(),
            bold(&format!("{count}")),
            dim("(re-split to match --names)")
        );
    } else {
        println!("{} detected    {}", ok_mark(), bold(&format!("{count}")));
    }
    println!();

    // Build names: provided or numbered
    let names: Vec<String> = if args.names.is_empty() {
        (1..=count).map(|i| format!("{i}")).collect()
    } else {
        args.names.clone()
    };

    // All outputs are uniformly sized to the largest detected bbox by
    // padding the smaller crops with the background color (so we never
    // accidentally include neighboring characters).
    let (max_w, max_h) = split::max_dimensions(&result.objects);

    let name_width = names.iter().map(|s| s.chars().count()).max().unwrap_or(1);
    let median_w = median_width(&result.objects);
    for (name, obj) in names.iter().zip(result.objects.iter()) {
        let pad = " ".repeat(name_width - name.chars().count());
        let coord = format!("({:>4}, {:>4})", obj.x, obj.y);
        let detected = format!("{}×{}", obj.w, obj.h);
        let marker = if obj.w as f64 > median_w * 1.15 {
            dim("(wider)")
        } else if (obj.w as f64) < median_w * 0.85 {
            dim("(narrower)")
        } else {
            String::new()
        };
        println!(
            "  {bold}{pad}  {coord}  detected {:>9}  {}",
            dim(&detected),
            marker,
            bold = bold(name),
        );
    }
    println!(
        "\n{} all outputs padded to {}",
        dim("output size:"),
        bold(&format!("{max_w}×{max_h}"))
    );
    println!();

    // Save crops
    std::fs::create_dir_all(&args.output)
        .with_context(|| format!("Failed to create output dir: {}", args.output.display()))?;

    let mut total_size = 0u64;
    let mut saved_paths = Vec::new();
    for (name, obj) in names.iter().zip(result.objects.iter()) {
        let cropped = split::crop_padded(&img, obj, max_w, max_h, result.background);
        let path = args.output.join(format!("{name}.png"));
        ensure_parent(&path)?;
        cropped
            .save(&path)
            .with_context(|| format!("Failed to save: {}", path.display()))?;
        if let Ok(meta) = std::fs::metadata(&path) {
            total_size += meta.len();
        }
        saved_paths.push(path);
    }

    println!(
        "saved to {} {}",
        bold(&args.output.display().to_string()),
        dim(&format!("({} files, {})", saved_paths.len(), format_size(total_size))),
    );
    for p in &saved_paths {
        println!("  {} {}", ok_mark(), p.display());
    }

    if args.preview {
        let preview = preview_path(&args.input);
        split::write_preview(&img, &result, &preview)
            .with_context(|| format!("Failed to write preview: {}", preview.display()))?;
        println!("\npreview {} {}", arrow(), preview.display());
    }

    Ok(())
}

fn preview_path(input: &std::path::Path) -> PathBuf {
    let parent = input.parent().unwrap_or(std::path::Path::new("."));
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".to_string());
    parent.join(format!("{stem}-preview.png"))
}

fn median_width(objs: &[split::DetectedObject]) -> f64 {
    let mut widths: Vec<u32> = objs.iter().map(|o| o.w).collect();
    widths.sort_unstable();
    if widths.is_empty() {
        0.0
    } else {
        widths[widths.len() / 2] as f64
    }
}
