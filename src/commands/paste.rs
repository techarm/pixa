//! `pixa paste` — save the OS clipboard image to a file or stdout.
//!
//! On macOS, when the clipboard holds real PNG bytes and the output target
//! is PNG, the bytes are written verbatim (byte-passthrough, preserving any
//! encoder settings and surviving metadata). In every other case the image
//! is decoded via arboard and re-encoded to the target format.

use std::io::{Cursor, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use image::DynamicImage;

use super::style::{arrow, green, ok_mark, red};
use super::{ensure_parent, format_size};
use pixa::ImageFormat;

/// JPEG quality for `paste` — tuned higher than `compress` (75) because
/// `paste` is about preserving the clipboard contents, not shrinking.
const PASTE_JPEG_QUALITY: u8 = 90;
/// WebP quality for `paste`, mirroring `convert`'s 90.
const PASTE_WEBP_QUALITY: f32 = 90.0;

#[derive(Args)]
pub struct PasteArgs {
    /// Output file path, or `-` for stdout.
    pub output: String,
    /// Force output format (png, jpg, jpeg, webp, bmp, gif, tiff).
    /// Inferred from the output extension when omitted.
    #[arg(long)]
    pub format: Option<String>,
}

enum Target {
    File(PathBuf),
    Stdout,
}

pub fn run(args: PasteArgs) -> Result<()> {
    let (format, target) = resolve_format_and_target(&args)?;

    // Byte-passthrough path 1: user copied a FILE (Cmd+C in Finder).
    // When the output extension matches the source file's extension
    // and no --format override is in play, copy the source bytes
    // verbatim — this preserves metadata (EXIF, ICC) that every other
    // path would strip. When the extensions differ, fall through to
    // opening the file and re-encoding to the target format (still
    // lossless when possible, and far better than a TIFF preview
    // routed through arboard).
    if args.format.is_none()
        && let Some(src_path) = pixa::clipboard::read_file_url()?
    {
        if let Target::File(out_path) = &target
            && extensions_match(&src_path, out_path)
        {
            let bytes = std::fs::read(&src_path)
                .with_context(|| format!("Failed to read: {}", src_path.display()))?;
            return write_target(&target, &bytes);
        }
        let img = image::open(&src_path)
            .with_context(|| format!("Failed to open: {}", src_path.display()))?;
        let bytes = encode(&img, format)?;
        return write_target(&target, &bytes);
    }

    // Byte-passthrough path 2: PNG target, no --format override, and
    // the clipboard has native PNG bytes (e.g. Cmd+C from a browser).
    // Skips decode + re-encode entirely.
    if matches!(format, ImageFormat::Png)
        && args.format.is_none()
        && let Some(bytes) = pixa::clipboard::read_native_png()?
    {
        return write_target(&target, &bytes);
    }

    let img = pixa::clipboard::read_image()?;
    let bytes = encode(&img, format)?;
    write_target(&target, &bytes)
}

/// True if `src` and `dst` resolve to the same normalized image
/// extension (jpg == jpeg, case-insensitive). Used to decide whether
/// paste can copy file bytes verbatim.
fn extensions_match(src: &std::path::Path, dst: &std::path::Path) -> bool {
    let norm = |p: &std::path::Path| {
        p.extension()
            .and_then(|e| e.to_str())
            .and_then(ImageFormat::from_extension)
    };
    match (norm(src), norm(dst)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn resolve_format_and_target(args: &PasteArgs) -> Result<(ImageFormat, Target)> {
    if args.output == "-" {
        let fmt_str = args
            .format
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--format is required when writing to stdout"))?;
        let fmt = ImageFormat::from_extension(fmt_str)
            .ok_or_else(|| anyhow::anyhow!("Unsupported format: {fmt_str}"))?;
        return Ok((fmt, Target::Stdout));
    }

    let path = PathBuf::from(&args.output);
    let fmt = if let Some(override_ext) = &args.format {
        ImageFormat::from_extension(override_ext)
            .ok_or_else(|| anyhow::anyhow!("Unsupported format: {override_ext}"))?
    } else {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        ImageFormat::from_extension(ext).ok_or_else(|| {
            anyhow::anyhow!(
                "Cannot infer format from extension: {} (use --format)",
                path.display()
            )
        })?
    };
    Ok((fmt, Target::File(path)))
}

fn write_target(target: &Target, bytes: &[u8]) -> Result<()> {
    match target {
        Target::Stdout => {
            std::io::stdout()
                .lock()
                .write_all(bytes)
                .context("Failed to write to stdout")?;
        }
        Target::File(path) => {
            ensure_parent(path)?;
            std::fs::write(path, bytes)
                .with_context(|| format!("Failed to write: {}", path.display()))?;
            println!(
                "{} @clipboard {} {}  {}",
                ok_mark(),
                arrow(),
                green(&path.display().to_string()),
                red(&format_size(bytes.len() as u64)),
            );
        }
    }
    Ok(())
}

fn encode(img: &DynamicImage, format: ImageFormat) -> Result<Vec<u8>> {
    match format {
        ImageFormat::WebP => encode_webp(img),
        ImageFormat::Jpeg => encode_jpeg(img),
        other => encode_via_image_crate(img, other),
    }
}

fn encode_webp(img: &DynamicImage) -> Result<Vec<u8>> {
    let mem = if img.color().has_alpha() {
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        webp::Encoder::from_rgba(rgba.as_raw(), w, h).encode(PASTE_WEBP_QUALITY)
    } else {
        let rgb = img.to_rgb8();
        let (w, h) = (rgb.width(), rgb.height());
        webp::Encoder::from_rgb(rgb.as_raw(), w, h).encode(PASTE_WEBP_QUALITY)
    };
    Ok(mem.to_vec())
}

fn encode_jpeg(img: &DynamicImage) -> Result<Vec<u8>> {
    // JPEG has no alpha; arboard returns RGBA so always flatten.
    let rgb = img.to_rgb8();
    let mut buf = Vec::new();
    let mut encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, PASTE_JPEG_QUALITY);
    encoder.encode(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        image::ExtendedColorType::Rgb8,
    )?;
    Ok(buf)
}

fn encode_via_image_crate(img: &DynamicImage, format: ImageFormat) -> Result<Vec<u8>> {
    let image_fmt = match format {
        ImageFormat::Png => image::ImageFormat::Png,
        ImageFormat::Bmp => image::ImageFormat::Bmp,
        ImageFormat::Gif => image::ImageFormat::Gif,
        ImageFormat::Tiff => image::ImageFormat::Tiff,
        ImageFormat::WebP | ImageFormat::Jpeg => {
            unreachable!("webp and jpeg are handled by dedicated encoders")
        }
    };
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), image_fmt)?;
    Ok(buf)
}
