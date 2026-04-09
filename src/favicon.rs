//! Favicon and icon set generation from any source image.
//!
//! Takes a source image and generates a complete web-ready icon set:
//! favicon.ico (multi-resolution), PNG icons at standard sizes,
//! Apple Touch Icon, and Android Chrome icons.

use image::{DynamicImage, GenericImageView};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::info;

#[derive(Error, Debug)]
pub enum FaviconError {
    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("PNG optimization error: {0}")]
    PngOptimize(String),
    #[error("Input image too small: {0}x{1} (need at least 16x16)")]
    TooSmall(u32, u32),
}

/// Options for favicon generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaviconOptions {
    /// PNG optimization level (0-6, default: 4)
    pub png_level: u8,
}

impl Default for FaviconOptions {
    fn default() -> Self {
        Self { png_level: 4 }
    }
}

/// Specification for a single icon in the generated set
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IconSpec {
    pub filename: String,
    pub width: u32,
    pub height: u32,
    pub purpose: String,
}

/// Result of favicon set generation
#[derive(Debug, Serialize, Deserialize)]
pub struct FaviconResult {
    pub generated_files: Vec<PathBuf>,
    pub html_snippet: String,
    pub total_size: u64,
    pub icons: Vec<IconSpec>,
}

/// PNG icon specifications: (filename, width, height, purpose)
const FAVICON_SPECS: &[(&str, u32, u32, &str)] = &[
    ("favicon-16x16.png", 16, 16, "favicon"),
    ("favicon-32x32.png", 32, 32, "favicon"),
    ("apple-touch-icon.png", 180, 180, "apple-touch-icon"),
    ("android-chrome-192x192.png", 192, 192, "android-chrome"),
    ("android-chrome-512x512.png", 512, 512, "android-chrome"),
];

/// Sizes embedded in the multi-resolution favicon.ico
const ICO_SIZES: &[u32] = &[16, 32, 48];

/// Generate a complete favicon set from an image file.
pub fn generate_favicon_set(
    input: &Path,
    output_dir: &Path,
    opts: &FaviconOptions,
) -> Result<FaviconResult, FaviconError> {
    let img = image::open(input)?;
    generate_favicon_set_from_image(&img, output_dir, opts)
}

/// Generate a complete favicon set from a DynamicImage.
pub fn generate_favicon_set_from_image(
    img: &DynamicImage,
    output_dir: &Path,
    opts: &FaviconOptions,
) -> Result<FaviconResult, FaviconError> {
    let (w, h) = img.dimensions();
    if w < 16 || h < 16 {
        return Err(FaviconError::TooSmall(w, h));
    }

    std::fs::create_dir_all(output_dir)?;

    // Crop to square first (center crop)
    let square = crop_to_square(img);

    let mut generated_files = Vec::new();
    let mut icons = Vec::new();
    let mut total_size = 0u64;

    // 1. Generate multi-resolution favicon.ico
    let ico_path = output_dir.join("favicon.ico");
    let ico_bytes = build_multi_resolution_ico(&square, ICO_SIZES, opts.png_level)?;
    std::fs::write(&ico_path, &ico_bytes)?;
    let ico_size = ico_bytes.len() as u64;
    total_size += ico_size;
    info!(
        "Generated: favicon.ico ({} bytes, {} sizes)",
        ico_size,
        ICO_SIZES.len()
    );
    generated_files.push(ico_path);
    icons.push(IconSpec {
        filename: "favicon.ico".to_string(),
        width: 48,
        height: 48,
        purpose: "favicon".to_string(),
    });

    // 2. Generate individual PNG icons
    for &(filename, width, height, purpose) in FAVICON_SPECS {
        let resized = square.resize_exact(width, height, image::imageops::FilterType::Lanczos3);
        let png_bytes = encode_and_optimize_png(&resized, opts.png_level)?;
        let path = output_dir.join(filename);
        std::fs::write(&path, &png_bytes)?;
        let size = png_bytes.len() as u64;
        total_size += size;
        info!(
            "Generated: {} ({}x{}, {} bytes)",
            filename, width, height, size
        );
        generated_files.push(path);
        icons.push(IconSpec {
            filename: filename.to_string(),
            width,
            height,
            purpose: purpose.to_string(),
        });
    }

    let html_snippet = build_html_snippet();

    Ok(FaviconResult {
        generated_files,
        html_snippet,
        total_size,
        icons,
    })
}

/// Crop an image to a centered square.
fn crop_to_square(img: &DynamicImage) -> DynamicImage {
    let (w, h) = img.dimensions();
    if w == h {
        return img.clone();
    }
    let side = w.min(h);
    let x = (w - side) / 2;
    let y = (h - side) / 2;
    img.crop_imm(x, y, side, side)
}

/// Encode a DynamicImage to PNG bytes and optimize with oxipng.
fn encode_and_optimize_png(img: &DynamicImage, level: u8) -> Result<Vec<u8>, FaviconError> {
    let mut raw_png = Vec::new();
    img.write_to(&mut Cursor::new(&mut raw_png), image::ImageFormat::Png)?;

    let mut opts = oxipng::Options::from_preset(level);
    opts.strip = oxipng::StripChunks::Safe;

    let optimized = oxipng::optimize_from_memory(&raw_png, &opts)
        .map_err(|e| FaviconError::PngOptimize(e.to_string()))?;

    Ok(optimized)
}

/// Build a multi-resolution ICO file by embedding PNG data for each size.
///
/// ICO format:
/// - Header (6 bytes): reserved(u16), type(u16=1), count(u16)
/// - Directory entries (16 bytes each): width, height, palette, reserved, planes, bpp, size, offset
/// - Image data: raw PNG bytes for each entry
fn build_multi_resolution_ico(
    source: &DynamicImage,
    sizes: &[u32],
    png_level: u8,
) -> Result<Vec<u8>, FaviconError> {
    let count = sizes.len() as u16;

    // Generate optimized PNG bytes for each size
    let mut png_entries: Vec<(u32, Vec<u8>)> = Vec::new();
    for &size in sizes {
        let resized = source.resize_exact(size, size, image::imageops::FilterType::Lanczos3);
        let png_bytes = encode_and_optimize_png(&resized, png_level)?;
        png_entries.push((size, png_bytes));
    }

    // Calculate offsets
    let header_size: u32 = 6;
    let dir_size: u32 = 16 * count as u32;
    let data_start = header_size + dir_size;

    let mut buf: Vec<u8> = Vec::new();

    // ICO Header (6 bytes)
    buf.extend_from_slice(&0u16.to_le_bytes()); // reserved
    buf.extend_from_slice(&1u16.to_le_bytes()); // type = ICO
    buf.extend_from_slice(&count.to_le_bytes()); // image count

    // Directory Entries (16 bytes each)
    let mut current_offset = data_start;
    for (size, png_data) in &png_entries {
        let dim = if *size >= 256 { 0u8 } else { *size as u8 };
        buf.push(dim); // width
        buf.push(dim); // height
        buf.push(0); // color palette
        buf.push(0); // reserved
        buf.extend_from_slice(&1u16.to_le_bytes()); // color planes
        buf.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        buf.extend_from_slice(&(png_data.len() as u32).to_le_bytes()); // data size
        buf.extend_from_slice(&current_offset.to_le_bytes()); // data offset
        current_offset += png_data.len() as u32;
    }

    // Image Data
    for (_, png_data) in &png_entries {
        buf.extend_from_slice(png_data);
    }

    Ok(buf)
}

/// Build an HTML snippet for including the generated icons.
fn build_html_snippet() -> String {
    r#"<link rel="icon" type="image/x-icon" href="/favicon.ico">
<link rel="icon" type="image/png" sizes="16x16" href="/favicon-16x16.png">
<link rel="icon" type="image/png" sizes="32x32" href="/favicon-32x32.png">
<link rel="apple-touch-icon" sizes="180x180" href="/apple-touch-icon.png">
<link rel="icon" type="image/png" sizes="192x192" href="/android-chrome-192x192.png">
<link rel="icon" type="image/png" sizes="512x512" href="/android-chrome-512x512.png">"#
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(image::RgbaImage::from_fn(w, h, |x, y| {
            image::Rgba([(x % 256) as u8, (y % 256) as u8, 128u8, 255u8])
        }))
    }

    #[test]
    fn test_crop_to_square_already_square() {
        let img = test_image(100, 100);
        let cropped = crop_to_square(&img);
        assert_eq!(cropped.dimensions(), (100, 100));
    }

    #[test]
    fn test_crop_to_square_landscape() {
        let img = test_image(200, 100);
        let cropped = crop_to_square(&img);
        assert_eq!(cropped.dimensions(), (100, 100));
    }

    #[test]
    fn test_crop_to_square_portrait() {
        let img = test_image(100, 200);
        let cropped = crop_to_square(&img);
        assert_eq!(cropped.dimensions(), (100, 100));
    }

    #[test]
    fn test_encode_and_optimize_png() {
        let img = test_image(64, 64);
        let bytes = encode_and_optimize_png(&img, 2).unwrap();
        // Verify PNG signature
        assert_eq!(&bytes[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[test]
    fn test_build_multi_resolution_ico() {
        let img = test_image(64, 64);
        let ico = build_multi_resolution_ico(&img, &[16, 32, 48], 2).unwrap();
        // Verify ICO header
        assert_eq!(&ico[0..2], &[0, 0]); // reserved
        assert_eq!(&ico[2..4], &[1, 0]); // type = ICO
        assert_eq!(&ico[4..6], &[3, 0]); // 3 images
        // Verify first directory entry width = 16
        assert_eq!(ico[6], 16);
        // Verify second directory entry width = 32
        assert_eq!(ico[6 + 16], 32);
        // Verify third directory entry width = 48
        assert_eq!(ico[6 + 32], 48);
    }

    #[test]
    fn test_generate_favicon_set_creates_all_files() {
        let img = test_image(512, 512);
        let dir = tempfile::tempdir().unwrap();
        let result =
            generate_favicon_set_from_image(&img, dir.path(), &FaviconOptions::default()).unwrap();

        // 1 ICO + 5 PNGs = 6 files
        assert_eq!(result.generated_files.len(), 6);
        assert!(result.html_snippet.contains("favicon.ico"));
        assert!(result.html_snippet.contains("apple-touch-icon"));
        for path in &result.generated_files {
            assert!(path.exists());
        }
        assert_eq!(result.icons.len(), 6);
    }

    #[test]
    fn test_generate_favicon_set_non_square() {
        let img = test_image(800, 400);
        let dir = tempfile::tempdir().unwrap();
        let result =
            generate_favicon_set_from_image(&img, dir.path(), &FaviconOptions::default()).unwrap();
        assert_eq!(result.generated_files.len(), 6);
    }

    #[test]
    fn test_too_small_image_rejected() {
        let img = test_image(8, 8);
        let dir = tempfile::tempdir().unwrap();
        let result = generate_favicon_set_from_image(&img, dir.path(), &FaviconOptions::default());
        assert!(matches!(result, Err(FaviconError::TooSmall(8, 8))));
    }

    #[test]
    fn test_html_snippet_contains_all_links() {
        let snippet = build_html_snippet();
        assert!(snippet.contains("favicon.ico"));
        assert!(snippet.contains("favicon-16x16.png"));
        assert!(snippet.contains("favicon-32x32.png"));
        assert!(snippet.contains("apple-touch-icon.png"));
        assert!(snippet.contains("android-chrome-192x192.png"));
        assert!(snippet.contains("android-chrome-512x512.png"));
    }
}
