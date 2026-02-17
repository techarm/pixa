//! Image compression and optimization

use image::{DynamicImage, GenericImageView};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::Path;
use thiserror::Error;
use tracing::info;

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
    #[error("WebP encoding error: {0}")]
    WebpEncode(String),
}

/// Compression options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressOptions {
    /// JPEG quality (1-100, default: 80)
    pub jpeg_quality: u8,
    /// PNG optimization level (0-6, default: 4)
    pub png_level: u8,
    /// WebP quality (1-100, default: 80)
    pub webp_quality: u8,
    /// Maximum width (optional, preserves aspect ratio)
    pub max_width: Option<u32>,
    /// Maximum height (optional, preserves aspect ratio)
    pub max_height: Option<u32>,
    /// Strip metadata
    pub strip_metadata: bool,
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            jpeg_quality: 80,
            png_level: 4,
            webp_quality: 80,
            max_width: None,
            max_height: None,
            strip_metadata: true,
        }
    }
}

/// Compression result
#[derive(Debug, Serialize, Deserialize)]
pub struct CompressResult {
    pub original_size: u64,
    pub compressed_size: u64,
    pub savings_percent: f64,
    pub output_width: u32,
    pub output_height: u32,
}

/// Compress an image with the given options
pub fn compress_image(
    input: &Path,
    output: &Path,
    opts: &CompressOptions,
) -> Result<CompressResult, CompressError> {
    let original_size = std::fs::metadata(input)?.len();
    let mut img = image::open(input)?;

    // Resize if needed
    if let Some(result) = resize_if_needed(&img, opts.max_width, opts.max_height) {
        img = result;
    }

    let (out_w, out_h) = img.dimensions();
    let ext = output
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "jpg" | "jpeg" => {
            compress_jpeg(&img, output, opts.jpeg_quality)?;
        }
        "png" => {
            compress_png(&img, output, opts.png_level)?;
        }
        "webp" => {
            compress_webp(&img, output, opts.webp_quality)?;
        }
        _ => {
            // Fallback: save with image crate defaults
            img.save(output)?;
        }
    }

    let compressed_size = std::fs::metadata(output)?.len();
    let savings = if original_size > 0 {
        (1.0 - compressed_size as f64 / original_size as f64) * 100.0
    } else {
        0.0
    };

    info!(
        "Compressed: {} -> {} ({:.1}% savings)",
        format_size(original_size),
        format_size(compressed_size),
        savings
    );

    Ok(CompressResult {
        original_size,
        compressed_size,
        savings_percent: savings,
        output_width: out_w,
        output_height: out_h,
    })
}

fn resize_if_needed(
    img: &DynamicImage,
    max_width: Option<u32>,
    max_height: Option<u32>,
) -> Option<DynamicImage> {
    let (w, h) = img.dimensions();
    let mut scale = 1.0f64;

    if let Some(mw) = max_width {
        if w > mw {
            scale = scale.min(mw as f64 / w as f64);
        }
    }
    if let Some(mh) = max_height {
        if h > mh {
            scale = scale.min(mh as f64 / h as f64);
        }
    }

    if scale < 1.0 {
        let new_w = (w as f64 * scale).round() as u32;
        let new_h = (h as f64 * scale).round() as u32;
        info!("Resizing: {}x{} -> {}x{}", w, h, new_w, new_h);
        Some(img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3))
    } else {
        None
    }
}

fn compress_jpeg(img: &DynamicImage, output: &Path, quality: u8) -> Result<(), CompressError> {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();

    let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
    comp.set_size(w as usize, h as usize);
    comp.set_quality(quality as f32);
    comp.set_mem_dest();
    comp.start_compress();

    let raw = rgb.as_raw();
    // Write scanlines
    let stride = w as usize * 3;
    for y in 0..h as usize {
        let row = &raw[y * stride..(y + 1) * stride];
        comp.write_scanlines(row);
    }

    comp.finish_compress();
    let data = comp.data_to_vec().map_err(|e| {
        CompressError::Image(image::ImageError::Encoding(
            image::error::EncodingError::new(
                image::error::ImageFormatHint::Exact(image::ImageFormat::Jpeg),
                e,
            ),
        ))
    })?;

    std::fs::write(output, data)?;
    Ok(())
}

fn compress_png(img: &DynamicImage, output: &Path, level: u8) -> Result<(), CompressError> {
    // First save with image crate
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)?;

    // Then optimize with oxipng
    let mut opts = oxipng::Options::from_preset(level);
    opts.strip = oxipng::StripChunks::Safe; // Remove safe-to-strip metadata

    let optimized = oxipng::optimize_from_memory(&buf, &opts)
        .map_err(|e| CompressError::PngOptimize(e.to_string()))?;

    std::fs::write(output, optimized)?;
    Ok(())
}

fn compress_webp(img: &DynamicImage, output: &Path, quality: u8) -> Result<(), CompressError> {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();

    let encoder = webp::Encoder::from_rgb(rgb.as_raw(), w, h);
    let mem = encoder.encode(quality as f32);

    std::fs::write(output, &*mem)?;
    Ok(())
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
