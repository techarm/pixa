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

use image::DynamicImage;
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
/// `output` extension. If the compressed result would be larger than
/// the original, the original is copied to `output` instead and
/// `kept_original = true` is returned.
pub fn compress_image(input: &Path, output: &Path) -> Result<CompressResult, CompressError> {
    let original_size = std::fs::metadata(input)?.len();
    let img = image::open(input)?;

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
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let encoder = webp::Encoder::from_rgb(rgb.as_raw(), w, h);
    let mem = encoder.encode(quality as f32);
    Ok(mem.to_vec())
}
