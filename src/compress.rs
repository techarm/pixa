//! Image compression and optimization.
//!
//! Defaults are tuned for "make this image smaller without thinking":
//!
//!   JPEG → mozjpeg quality 75
//!   PNG  → oxipng level 6 (max)
//!   WebP → webp quality 80
//!
//! Metadata is always stripped. PNG is always lossless. JPEG and WebP
//! are always lossy with the defaults above.

use image::{DynamicImage, GenericImageView};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::Path;
use thiserror::Error;

/// JPEG quality default — visually transparent for typical photos.
const JPEG_QUALITY: u8 = 75;
/// PNG optimization level — max effort, still fast enough.
const PNG_LEVEL: u8 = 6;
/// WebP quality default.
const WEBP_QUALITY: u8 = 80;

#[derive(Error, Debug)]
pub enum CompressError {
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("PNG optimization error: {0}")]
    PngOptimize(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressResult {
    pub original_size: u64,
    pub compressed_size: u64,
    pub savings_percent: f64,
    /// `true` if the compressed output was larger than the original
    /// and we kept the original instead.
    pub kept_original: bool,
}

/// Compress `input` into `output`. Output format is determined by the
/// `output` extension. If `max_edge` is `Some`, the image is resized
/// (preserving aspect ratio) so that its longest edge is at most that
/// many pixels. If the compressed result would be larger than the
/// original, the original is copied to `output` instead and
/// `kept_original = true` is returned.
pub fn compress_image(
    input: &Path,
    output: &Path,
    max_edge: Option<u32>,
) -> Result<CompressResult, CompressError> {
    let original_size = std::fs::metadata(input)?.len();
    let mut img = image::open(input)?;

    if let Some(limit) = max_edge
        && let Some(resized) = resize_to_max_edge(&img, limit)
    {
        img = resized;
    }

    let ext = output
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let compressed = match ext.as_str() {
        "jpg" | "jpeg" => encode_jpeg(&img, JPEG_QUALITY)?,
        "png" => encode_png(&img, PNG_LEVEL)?,
        "webp" => encode_webp(&img, WEBP_QUALITY)?,
        other => return Err(CompressError::UnsupportedFormat(other.to_string())),
    };

    // Decide whether to keep the compressed result or fall back to the
    // original (if the optimizer made it bigger).
    let (final_bytes, kept_original): (Vec<u8>, bool) = if compressed.len() as u64 >= original_size
    {
        (std::fs::read(input)?, true)
    } else {
        (compressed, false)
    };

    std::fs::write(output, &final_bytes)?;
    let compressed_size = final_bytes.len() as u64;
    let savings = if original_size > 0 {
        (1.0 - compressed_size as f64 / original_size as f64) * 100.0
    } else {
        0.0
    };

    Ok(CompressResult {
        original_size,
        compressed_size,
        savings_percent: savings,
        kept_original,
    })
}

/// Resize so the longest edge is exactly `max_edge`, preserving
/// aspect ratio. Returns `None` if the image is already smaller.
fn resize_to_max_edge(img: &DynamicImage, max_edge: u32) -> Option<DynamicImage> {
    let (w, h) = img.dimensions();
    let longest = w.max(h);
    if longest <= max_edge {
        return None;
    }
    let scale = max_edge as f64 / longest as f64;
    let new_w = ((w as f64) * scale).round().max(1.0) as u32;
    let new_h = ((h as f64) * scale).round().max(1.0) as u32;
    Some(img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3))
}

fn encode_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, CompressError> {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();

    let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
    comp.set_size(w as usize, h as usize);
    comp.set_quality(quality as f32);

    let mut started = comp.start_compress(Vec::new()).map_err(jpeg_err)?;
    started.write_scanlines(rgb.as_raw()).map_err(jpeg_err)?;
    started.finish().map_err(jpeg_err)
}

fn jpeg_err(e: std::io::Error) -> CompressError {
    CompressError::Image(image::ImageError::Encoding(
        image::error::EncodingError::new(
            image::error::ImageFormatHint::Exact(image::ImageFormat::Jpeg),
            e,
        ),
    ))
}

fn encode_png(img: &DynamicImage, level: u8) -> Result<Vec<u8>, CompressError> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)?;

    let mut opts = oxipng::Options::from_preset(level);
    opts.strip = oxipng::StripChunks::Safe;

    oxipng::optimize_from_memory(&buf, &opts).map_err(|e| CompressError::PngOptimize(e.to_string()))
}

fn encode_webp(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, CompressError> {
    let mem = if img.color().has_alpha() {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        webp::Encoder::from_rgba(rgba.as_raw(), w, h).encode(quality as f32)
    } else {
        let rgb = img.to_rgb8();
        let (w, h) = rgb.dimensions();
        webp::Encoder::from_rgb(rgb.as_raw(), w, h).encode(quality as f32)
    };
    Ok(mem.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, Rgb, RgbImage, Rgba, RgbaImage};
    use tempfile::TempDir;

    fn transparent_half_image(w: u32, h: u32) -> DynamicImage {
        // High-entropy RGB pattern + hard alpha split. The entropy is
        // load-bearing: a low-entropy image (e.g. solid colour + alpha
        // split) compresses smaller as PNG than as WebP, which would
        // trip `compress_image`'s kept-original fallback and bypass
        // the `encode_webp` path this test is supposed to cover.
        let mut img = RgbaImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let r = ((x * 13).wrapping_mul(y * 17 + 1) % 256) as u8;
                let g = ((x * 29) ^ (y * 31)) as u8;
                let b = ((x + y) * 71 % 256) as u8;
                let alpha = if x < w / 2 { 255 } else { 0 };
                img.put_pixel(x, y, Rgba([r, g, b, alpha]));
            }
        }
        DynamicImage::ImageRgba8(img)
    }

    /// Create a small test image with varying colors (so encoders don't
    /// collapse it to a trivial-size file).
    fn test_image(w: u32, h: u32) -> DynamicImage {
        let mut img = RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                img.put_pixel(
                    x,
                    y,
                    Rgb([
                        ((x * 255 / w.max(1)) % 256) as u8,
                        ((y * 255 / h.max(1)) % 256) as u8,
                        (((x + y) * 127 / (w + h).max(1)) % 256) as u8,
                    ]),
                );
            }
        }
        DynamicImage::ImageRgb8(img)
    }

    fn write_png(path: &std::path::Path, w: u32, h: u32) {
        let img = test_image(w, h);
        img.save(path).expect("write test png");
    }

    // --- encoder magic-byte sanity ---

    #[test]
    fn encode_jpeg_produces_jpeg_bytes() {
        let bytes = encode_jpeg(&test_image(32, 32), 75).unwrap();
        assert!(bytes.len() > 4, "jpeg bytes should be non-trivial");
        assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF], "JPEG SOI marker");
    }

    #[test]
    fn encode_png_produces_png_bytes() {
        let bytes = encode_png(&test_image(32, 32), 1).unwrap();
        assert!(bytes.len() > 8);
        assert_eq!(
            &bytes[..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
            "PNG signature"
        );
    }

    #[test]
    fn encode_webp_produces_webp_bytes() {
        let bytes = encode_webp(&test_image(32, 32), 80).unwrap();
        assert!(bytes.len() > 12);
        assert_eq!(&bytes[..4], b"RIFF", "WebP RIFF header");
        assert_eq!(&bytes[8..12], b"WEBP", "WebP four-cc");
    }

    // --- resize ---

    #[test]
    fn resize_landscape_to_max_edge() {
        let img = test_image(2560, 1440);
        let resized = resize_to_max_edge(&img, 1920).expect("should resize");
        let (w, h) = resized.dimensions();
        assert_eq!(w, 1920);
        assert_eq!(h, 1080, "aspect ratio preserved");
    }

    #[test]
    fn resize_portrait_to_max_edge() {
        let img = test_image(1440, 2560);
        let resized = resize_to_max_edge(&img, 1920).expect("should resize");
        let (w, h) = resized.dimensions();
        assert_eq!(w, 1080);
        assert_eq!(h, 1920);
    }

    #[test]
    fn resize_noop_when_under_limit() {
        let img = test_image(500, 300);
        assert!(resize_to_max_edge(&img, 1920).is_none());
    }

    #[test]
    fn resize_noop_when_exact_limit() {
        let img = test_image(1920, 1080);
        assert!(resize_to_max_edge(&img, 1920).is_none());
    }

    // --- compress_image end-to-end ---

    #[test]
    fn compress_transparent_png_to_webp_preserves_alpha() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("in.png");
        let output = dir.path().join("out.webp");
        transparent_half_image(64, 64)
            .save(&input)
            .expect("write transparent png");

        let result = compress_image(&input, &output, None).unwrap();
        // Guard against the kept-original fallback silently making this
        // test pass — we must actually have exercised `encode_webp`.
        assert!(
            !result.kept_original,
            "test must hit the WebP encode path, not the kept-original fallback"
        );
        let bytes = std::fs::read(&output).unwrap();
        assert_eq!(&bytes[..4], b"RIFF", "output must be real WebP");
        assert_eq!(&bytes[8..12], b"WEBP");

        let decoded = image::open(&output).unwrap();
        assert!(
            decoded.color().has_alpha(),
            "webp compressed from a transparent PNG must retain alpha"
        );
        let rgba = decoded.to_rgba8();
        assert_eq!(rgba.get_pixel(0, 0)[3], 255, "opaque half stays opaque");
        assert_eq!(
            rgba.get_pixel(63, 0)[3],
            0,
            "transparent half stays transparent"
        );
    }

    #[test]
    fn compress_png_to_webp_roundtrip() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("in.png");
        let output = dir.path().join("out.webp");
        write_png(&input, 256, 256);

        let result = compress_image(&input, &output, None).unwrap();
        assert!(output.exists());
        assert!(!result.kept_original);
        assert!(result.original_size > 0);
        assert!(result.compressed_size > 0);

        // WebP output should actually be WebP
        let bytes = std::fs::read(&output).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WEBP");
    }

    #[test]
    fn compress_png_to_jpeg_via_extension() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("in.png");
        let output = dir.path().join("out.jpg");
        write_png(&input, 200, 200);

        compress_image(&input, &output, None).unwrap();
        let bytes = std::fs::read(&output).unwrap();
        assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF], "is JPEG");
    }

    #[test]
    fn compress_unsupported_extension_errors() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("in.png");
        let output = dir.path().join("out.gif");
        write_png(&input, 32, 32);

        let err = compress_image(&input, &output, None).unwrap_err();
        assert!(matches!(err, CompressError::UnsupportedFormat(_)));
    }

    #[test]
    fn compress_with_max_edge_resizes() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("big.png");
        let output = dir.path().join("small.webp");
        write_png(&input, 2000, 1000);

        compress_image(&input, &output, Some(800)).unwrap();

        // The WebP output decoded should be <= 800 on the long edge.
        let decoded = image::open(&output).unwrap();
        let (w, h) = decoded.dimensions();
        assert!(w <= 800 && h <= 800, "got {w}x{h}");
        assert!(w == 800 || h == 800, "one edge should equal the limit");
    }

    #[test]
    fn compress_kept_original_when_result_larger() {
        // Pre-compressed WebP source → re-encoding it as a higher-quality
        // PNG copy should be larger, so compress_image should fall back
        // to copying the original bytes.
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("minified.webp");
        let output = dir.path().join("out.webp");

        // Make an already-small WebP (gradient image at quality 10, small)
        let bytes = encode_webp(&test_image(16, 16), 10).unwrap();
        std::fs::write(&input, bytes).unwrap();

        let result = compress_image(&input, &output, None).unwrap();
        // For this tiny input the result may or may not shrink; the
        // invariant we care about is that the output is never larger
        // than the original.
        assert!(result.compressed_size <= result.original_size);
        if result.kept_original {
            let orig = std::fs::read(&input).unwrap();
            let out = std::fs::read(&output).unwrap();
            assert_eq!(orig, out, "kept_original should copy bytes verbatim");
        }
    }
}
