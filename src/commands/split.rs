use anyhow::{Context, Result};
use clap::{Args, ValueEnum};
use image::{DynamicImage, Rgba, RgbaImage};
use pixa::split::{self, PreviewStyle, SplitOptions};
use pixa::transparent;
use std::path::PathBuf;

use super::style::{arrow, cyan, dim, green, ok_mark, red};
use super::{ImageSource, bail_with_hints, ensure_parent, format_size};

#[derive(Args)]
pub struct SplitArgs {
    /// Input sheet image (objects on a single-color background). Use
    /// @clipboard (aliases: @clip, @c) to read from the OS clipboard.
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
    /// What to draw in the preview image
    #[arg(long, value_enum, default_value = "output")]
    pub preview_style: PreviewStyleArg,
    /// Replace the detected background with transparency in each output.
    /// Uses the same chroma-key logic as `pixa transparent`.
    #[arg(long)]
    pub transparent: bool,
    /// RGB distance from the detected background colour at or below
    /// which a pixel is treated as background. Only used with
    /// `--transparent`.
    #[arg(long, default_value = "200", requires = "transparent")]
    pub tolerance: f64,
    /// Enable channel-based spill suppression on the edge band.
    #[arg(long, requires = "transparent")]
    pub despill: bool,
    /// Edge-band radius (pixels) for `--despill`.
    #[arg(long, default_value = "3", requires = "despill")]
    pub despill_band: u32,
    /// Morphologically erode each transparent crop's opaque region by
    /// this many pixels.
    #[arg(long, default_value = "0", requires = "transparent")]
    pub shrink: u32,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum PreviewStyleArg {
    /// Tight per-object detected bbox
    Detected,
    /// Uniform max_w × max_h frame for each output PNG
    Output,
    /// Both, with the output frame as the thicker line
    Both,
}

impl From<PreviewStyleArg> for PreviewStyle {
    fn from(a: PreviewStyleArg) -> Self {
        match a {
            PreviewStyleArg::Detected => PreviewStyle::Detected,
            PreviewStyleArg::Output => PreviewStyle::Output,
            PreviewStyleArg::Both => PreviewStyle::Both,
        }
    }
}

pub fn run(args: SplitArgs) -> Result<()> {
    let source = ImageSource::parse(&args.input);
    let img = source.load_image()?;

    let opts = SplitOptions {
        padding: args.padding,
        expected_count: if args.names.is_empty() {
            None
        } else {
            Some(args.names.len())
        },
    };

    let result = match split::detect_objects(&img, &opts) {
        Ok(r) => r,
        Err(e) => {
            // Auto-write a preview on failure to help diagnosis. Hints
            // are surfaced via the unified error channel so main() can
            // render them next to `error:` in the git-style format.
            let preview_out = preview_path(&source);
            let mut hints: Vec<String> = Vec::new();
            if let Ok(diag) = split::detect_objects(&img, &SplitOptions::default()) {
                if split::write_preview(&img, &diag, PreviewStyle::Detected, &preview_out).is_ok() {
                    hints.push(format!("preview written: {}", preview_out.display()));
                }
                hints.push("try --padding or pass --names to enable re-split".to_string());
            }
            return Err(bail_with_hints(e.to_string(), hints));
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
            red(&format!("{count}")),
            dim("(re-split to match --names)")
        );
    } else {
        println!("{} detected    {}", ok_mark(), red(&format!("{count}")));
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
            "  {name_col}{pad}  {coord_col}  detected {:>9}  {}",
            red(&detected),
            marker,
            name_col = green(name),
            coord_col = dim(&coord),
        );
    }
    println!(
        "\n{} all outputs padded to {}",
        dim("output size:"),
        red(&format!("{max_w}×{max_h}"))
    );
    println!();

    // Save crops
    std::fs::create_dir_all(&args.output)
        .with_context(|| format!("Failed to create output dir: {}", args.output.display()))?;

    let mut total_size = 0u64;
    let mut saved_paths = Vec::new();
    for (name, obj) in names.iter().zip(result.objects.iter()) {
        let cropped = if args.transparent {
            crop_padded_transparent(
                &img,
                obj,
                max_w,
                max_h,
                result.background,
                args.tolerance,
                args.despill,
                args.despill_band,
                args.shrink,
            )
        } else {
            split::crop_padded(&img, obj, max_w, max_h, result.background)
        };
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
        green(&args.output.display().to_string()),
        dim(&format!(
            "({} files, {})",
            saved_paths.len(),
            format_size(total_size)
        )),
    );
    for p in &saved_paths {
        println!("  {} {}", ok_mark(), green(&p.display().to_string()));
    }

    if args.preview {
        let preview = preview_path(&source);
        split::write_preview(&img, &result, args.preview_style.into(), &preview)
            .with_context(|| format!("Failed to write preview: {}", preview.display()))?;
        println!("\npreview {} {}", arrow(), preview.display());
    }

    Ok(())
}

fn preview_path(source: &ImageSource) -> PathBuf {
    match source {
        ImageSource::Clipboard => PathBuf::from("./clipboard-preview.png"),
        ImageSource::Path(input) => {
            let parent = input.parent().unwrap_or(std::path::Path::new("."));
            let stem = input
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "image".to_string());
            parent.join(format!("{stem}-preview.png"))
        }
    }
}

/// Crop `obj` from `img` onto a fully transparent `target_w × target_h`
/// canvas, keying out `background` from the cropped content. Mirrors
/// `split::crop_padded` but produces an RGBA image where the background
/// color is alpha=0 instead of filled.
#[allow(clippy::too_many_arguments)]
fn crop_padded_transparent(
    img: &DynamicImage,
    obj: &split::DetectedObject,
    target_w: u32,
    target_h: u32,
    background: [u8; 3],
    tolerance: f64,
    despill: bool,
    despill_band: u32,
    shrink: u32,
) -> DynamicImage {
    let mut cropped = img.crop_imm(obj.x, obj.y, obj.w, obj.h).to_rgba8();
    transparent::apply_transparency_to_rgba(
        &mut cropped,
        background,
        tolerance,
        despill,
        despill_band,
        shrink,
    );

    let tw = target_w.max(obj.w);
    let th = target_h.max(obj.h);
    let mut canvas = RgbaImage::from_pixel(tw, th, Rgba([0, 0, 0, 0]));
    let off_x = (tw - obj.w) / 2;
    let off_y = (th - obj.h) / 2;
    image::imageops::overlay(&mut canvas, &cropped, off_x as i64, off_y as i64);
    DynamicImage::ImageRgba8(canvas)
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
