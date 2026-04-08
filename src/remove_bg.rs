//! Background removal with multiple strategies:
//!
//! 1. **Chromakey green removal** — For AI-generated logos with green backgrounds.
//!    Uses HSV color space to detect and remove green pixels with anti-aliased edges.
//!
//! 2. **remove.bg API** — Industry-leading deep learning background removal for
//!    arbitrary images. Requires `REMOVEBG_API_KEY` environment variable.
//!
//! 3. **Local flood fill** — Fallback algorithm that samples corner pixels and uses
//!    BFS to remove similar-colored backgrounds. Works best on solid-color backgrounds.

use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{info, warn};

#[derive(Error, Debug)]
pub enum RemoveBgError {
    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("API error: {0}")]
    Api(String),
}

impl From<reqwest::Error> for RemoveBgError {
    fn from(e: reqwest::Error) -> Self {
        RemoveBgError::Api(e.to_string())
    }
}

/// Options for background removal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveBgOptions {
    /// Color distance tolerance for local flood fill (0-100, default: 30)
    pub tolerance: u8,
    /// remove.bg API key (from REMOVEBG_API_KEY env var)
    #[serde(skip)]
    pub api_key: Option<String>,
    /// Force local processing (skip API even if key is available)
    #[serde(default)]
    pub use_local: bool,
}

impl Default for RemoveBgOptions {
    fn default() -> Self {
        Self {
            tolerance: 30,
            api_key: None,
            use_local: false,
        }
    }
}

/// Result of background removal
#[derive(Debug, Serialize, Deserialize)]
pub struct RemoveBgResult {
    pub output_path: PathBuf,
    pub pixels_removed: u64,
    pub total_pixels: u64,
    pub removal_percent: f64,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Remove background from an image file.
///
/// Strategy:
/// 1. If `api_key` is set and `use_local` is false → try remove.bg API
/// 2. On API failure or if no key → fall back to local flood fill
pub async fn remove_background(
    input: &Path,
    output: &Path,
    opts: &RemoveBgOptions,
) -> Result<RemoveBgResult, RemoveBgError> {
    // Try remove.bg API first if key is available and not forced local
    if let Some(api_key) = &opts.api_key {
        if !opts.use_local {
            match remove_background_api(input, output, api_key).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    warn!("remove.bg API failed, falling back to local: {e}");
                }
            }
        }
    } else if !opts.use_local {
        info!("No REMOVEBG_API_KEY set. Using local flood fill. Set REMOVEBG_API_KEY for better results.");
    }

    // Local flood fill fallback
    remove_background_local(input, output, opts.tolerance)
}

/// Remove green (chromakey) background from an image file.
///
/// Designed for AI-generated logos where the prompt requests a green background.
/// Uses HSV color space to detect green pixels and set them transparent,
/// with smooth alpha gradients at edges for anti-aliasing.
/// After removal, auto-trims transparent borders so the logo fills the image.
pub fn remove_green_background_file(
    input: &Path,
    output: &Path,
) -> Result<RemoveBgResult, RemoveBgError> {
    let img = image::open(input)?;
    let bg_removed = remove_green_background(&img);

    // Auto-trim transparent borders so logo fills the file
    let result_img = trim_transparent_borders(&bg_removed, 4);

    let (w, h) = result_img.dimensions();
    let total_pixels = (w as u64) * (h as u64);
    let rgba = result_img.to_rgba8();
    let pixels_removed = rgba.pixels().filter(|p| p[3] == 0).count() as u64;
    let removal_percent = if total_pixels > 0 {
        (pixels_removed as f64 / total_pixels as f64) * 100.0
    } else {
        0.0
    };

    result_img.save(output)?;

    let (orig_w, orig_h) = img.dimensions();
    info!(
        "Green background removed and trimmed: {}x{} -> {}x{} ({:.1}% transparent)",
        orig_w, orig_h, w, h, removal_percent
    );

    Ok(RemoveBgResult {
        output_path: output.to_path_buf(),
        pixels_removed,
        total_pixels,
        removal_percent,
    })
}

/// Trim transparent borders from an image, cropping to the bounding box of
/// non-transparent content plus optional padding.
///
/// This ensures the logo fills the entire file (no wasted transparent space).
pub fn trim_transparent_borders(img: &DynamicImage, padding: u32) -> DynamicImage {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return img.clone();
    }

    let rgba = img.to_rgba8();

    // Find bounding box of non-transparent pixels
    let mut min_x = w;
    let mut min_y = h;
    let mut max_x = 0u32;
    let mut max_y = 0u32;

    for y in 0..h {
        for x in 0..w {
            if rgba.get_pixel(x, y)[3] > 0 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    // All pixels are transparent — return as-is
    if min_x > max_x || min_y > max_y {
        return img.clone();
    }

    // Apply padding (clamped to image bounds)
    let crop_x = min_x.saturating_sub(padding);
    let crop_y = min_y.saturating_sub(padding);
    let crop_w = (max_x + 1 + padding).min(w) - crop_x;
    let crop_h = (max_y + 1 + padding).min(h) - crop_y;

    img.crop_imm(crop_x, crop_y, crop_w, crop_h)
}

/// Remove green (chromakey) background from a DynamicImage in memory.
///
/// Uses HSV color space to detect green pixels and make them transparent.
/// Produces smooth anti-aliased edges at the boundary between subject and background.
pub fn remove_green_background(img: &DynamicImage) -> DynamicImage {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return img.clone();
    }

    let rgba = img.to_rgba8();
    let mut output = rgba.clone();

    for y in 0..h {
        for x in 0..w {
            let pixel = rgba.get_pixel(x, y);
            let green_alpha = compute_green_alpha(pixel[0], pixel[1], pixel[2]);
            if green_alpha < 255 {
                // Combine with existing alpha
                let orig_alpha = pixel[3] as f64 / 255.0;
                let new_alpha = (green_alpha as f64 * orig_alpha) as u8;
                output.put_pixel(x, y, Rgba([pixel[0], pixel[1], pixel[2], new_alpha]));
            }
        }
    }

    DynamicImage::ImageRgba8(output)
}

/// Remove background from a DynamicImage in memory using flood fill.
pub fn remove_background_from_image(img: &DynamicImage, tolerance: u8) -> DynamicImage {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return img.clone();
    }

    let rgba = img.to_rgba8();

    // Sample corner pixels
    let corners = [
        *rgba.get_pixel(0, 0),
        *rgba.get_pixel(w - 1, 0),
        *rgba.get_pixel(0, h - 1),
        *rgba.get_pixel(w - 1, h - 1),
    ];

    let bg_color = find_background_color(&corners);

    // Create visited mask
    let mut mask = vec![false; (w * h) as usize];

    // Flood fill from all four corners
    let tolerance_f64 = tolerance as f64;
    flood_fill(&rgba, &mut mask, w, h, 0, 0, &bg_color, tolerance_f64);
    flood_fill(&rgba, &mut mask, w, h, w - 1, 0, &bg_color, tolerance_f64);
    flood_fill(&rgba, &mut mask, w, h, 0, h - 1, &bg_color, tolerance_f64);
    flood_fill(
        &rgba,
        &mut mask,
        w,
        h,
        w - 1,
        h - 1,
        &bg_color,
        tolerance_f64,
    );

    // Apply mask: set background pixels to transparent
    let mut output = rgba;
    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            if mask[idx] {
                output.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
    }

    DynamicImage::ImageRgba8(output)
}

// ---------------------------------------------------------------------------
// remove.bg API
// ---------------------------------------------------------------------------

async fn remove_background_api(
    input: &Path,
    output: &Path,
    api_key: &str,
) -> Result<RemoveBgResult, RemoveBgError> {
    let image_data = tokio::fs::read(input).await?;

    let client = reqwest::Client::new();
    let form = reqwest::multipart::Form::new()
        .part(
            "image_file",
            reqwest::multipart::Part::bytes(image_data).file_name("image.png"),
        )
        .text("size", "auto");

    info!("Sending image to remove.bg API...");

    let response = client
        .post("https://api.remove.bg/v1.0/removebg")
        .header("X-Api-Key", api_key)
        .multipart(form)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(RemoveBgError::Api(format!(
            "remove.bg API error ({status}): {body}"
        )));
    }

    let result_bytes = response.bytes().await?;
    tokio::fs::write(output, &result_bytes).await?;

    // Load the result to count transparent pixels
    let result_img = image::load_from_memory(&result_bytes)
        .map_err(|e| RemoveBgError::Api(format!("Failed to parse API response: {e}")))?;
    let (w, h) = result_img.dimensions();
    let total_pixels = (w as u64) * (h as u64);
    let rgba = result_img.to_rgba8();
    let pixels_removed = rgba.pixels().filter(|p| p[3] == 0).count() as u64;
    let removal_percent = if total_pixels > 0 {
        (pixels_removed as f64 / total_pixels as f64) * 100.0
    } else {
        0.0
    };

    info!(
        "Background removed via remove.bg API: {:.1}% pixels transparentized",
        removal_percent
    );

    Ok(RemoveBgResult {
        output_path: output.to_path_buf(),
        pixels_removed,
        total_pixels,
        removal_percent,
    })
}

// ---------------------------------------------------------------------------
// Local flood fill
// ---------------------------------------------------------------------------

fn remove_background_local(
    input: &Path,
    output: &Path,
    tolerance: u8,
) -> Result<RemoveBgResult, RemoveBgError> {
    let img = image::open(input)?;
    let result_img = remove_background_from_image(&img, tolerance);

    let (w, h) = result_img.dimensions();
    let total_pixels = (w as u64) * (h as u64);

    let rgba = result_img.to_rgba8();
    let pixels_removed = rgba.pixels().filter(|p| p[3] == 0).count() as u64;
    let removal_percent = if total_pixels > 0 {
        (pixels_removed as f64 / total_pixels as f64) * 100.0
    } else {
        0.0
    };

    result_img.save(output)?;

    info!(
        "Background removed (local): {:.1}% pixels transparentized",
        removal_percent
    );

    Ok(RemoveBgResult {
        output_path: output.to_path_buf(),
        pixels_removed,
        total_pixels,
        removal_percent,
    })
}

// ---------------------------------------------------------------------------
// HSV chromakey helpers
// ---------------------------------------------------------------------------

/// Convert RGB (0-255 each) to HSV (H: 0-360, S: 0-1, V: 0-1).
fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let r = r as f64 / 255.0;
    let g = g as f64 / 255.0;
    let b = b as f64 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let h = if delta < f64::EPSILON {
        0.0
    } else if (max - r).abs() < f64::EPSILON {
        60.0 * (((g - b) / delta) % 6.0)
    } else if (max - g).abs() < f64::EPSILON {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };

    let s = if max < f64::EPSILON { 0.0 } else { delta / max };

    (h, s, max)
}

/// Compute alpha value for a pixel based on its greenness.
///
/// Returns 0 for pure chromakey green (#00FF00) pixels, 255 for non-green pixels,
/// and intermediate values for edge pixels (anti-aliasing).
///
/// Tuned for high-saturation chromakey green (S≈1.0, V≈1.0).
/// Logo colors with lower saturation are preserved.
fn compute_green_alpha(r: u8, g: u8, b: u8) -> u8 {
    let (h, s, v) = rgb_to_hsv(r, g, b);

    // Outside extended green range: fully opaque
    // Tighter than before to avoid removing logo's dark green/teal colors
    if h < 80.0 || h > 160.0 || s < 0.4 || v < 0.25 {
        return 255;
    }

    // Hue score: 1.0 at center of green (H=120), falls to 0.0 at edges
    let h_score = if (95.0..=145.0).contains(&h) {
        1.0
    } else if h < 95.0 {
        (h - 80.0) / 15.0
    } else {
        (160.0 - h) / 15.0
    };

    // Saturation score: requires high saturation (chromakey is S=1.0)
    // 0.0 below 0.4, ramps to 1.0 at 0.7
    let s_score = if s >= 0.7 {
        1.0
    } else {
        (s - 0.4) / 0.3
    };

    // Value score: requires decent brightness (chromakey is V=1.0)
    // 0.0 below 0.25, ramps to 1.0 at 0.5
    let v_score = if v >= 0.5 {
        1.0
    } else {
        (v - 0.25) / 0.25
    };

    let greenness = (h_score * s_score * v_score).clamp(0.0, 1.0);

    // More green = more transparent
    (255.0 * (1.0 - greenness)) as u8
}

// ---------------------------------------------------------------------------
// Flood fill helpers
// ---------------------------------------------------------------------------

/// Find the most likely background color from corner samples.
fn find_background_color(corners: &[Rgba<u8>]) -> Rgba<u8> {
    let mut best_color = corners[0];
    let mut best_count = 0u32;

    for (i, c) in corners.iter().enumerate() {
        let count = corners
            .iter()
            .filter(|other| color_distance(c, other) < 30.0)
            .count() as u32;
        if count > best_count || (count == best_count && i == 0) {
            best_count = count;
            best_color = *c;
        }
    }

    best_color
}

/// BFS flood fill from a starting point.
fn flood_fill(
    img: &RgbaImage,
    mask: &mut [bool],
    w: u32,
    h: u32,
    start_x: u32,
    start_y: u32,
    bg_color: &Rgba<u8>,
    tolerance: f64,
) {
    let start_idx = (start_y * w + start_x) as usize;
    if mask[start_idx] {
        return;
    }

    let start_pixel = img.get_pixel(start_x, start_y);
    if color_distance(start_pixel, bg_color) > tolerance {
        return;
    }

    let mut queue = VecDeque::new();
    queue.push_back((start_x, start_y));
    mask[start_idx] = true;

    while let Some((x, y)) = queue.pop_front() {
        let neighbors = [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ];

        for (nx, ny) in neighbors {
            if nx >= w || ny >= h {
                continue;
            }
            let idx = (ny * w + nx) as usize;
            if mask[idx] {
                continue;
            }

            let pixel = img.get_pixel(nx, ny);
            if color_distance(pixel, bg_color) <= tolerance {
                mask[idx] = true;
                queue.push_back((nx, ny));
            }
        }
    }
}

/// Calculate Euclidean distance between two RGB colors (ignoring alpha).
fn color_distance(a: &Rgba<u8>, b: &Rgba<u8>) -> f64 {
    let dr = a[0] as f64 - b[0] as f64;
    let dg = a[1] as f64 - b[1] as f64;
    let db = a[2] as f64 - b[2] as f64;
    (dr * dr + dg * dg + db * db).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white_image_with_red_center(w: u32, h: u32) -> DynamicImage {
        let mut img = RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]));
        let cx = w / 2;
        let cy = h / 2;
        let size = w.min(h) / 4;
        for y in (cy - size)..=(cy + size) {
            for x in (cx - size)..=(cx + size) {
                if x < w && y < h {
                    img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
                }
            }
        }
        DynamicImage::ImageRgba8(img)
    }

    fn green_image_with_red_center(w: u32, h: u32) -> DynamicImage {
        // Bright green (#00FF00) background with red square in center
        let mut img = RgbaImage::from_pixel(w, h, Rgba([0, 255, 0, 255]));
        let cx = w / 2;
        let cy = h / 2;
        let size = w.min(h) / 4;
        for y in (cy - size)..=(cy + size) {
            for x in (cx - size)..=(cx + size) {
                if x < w && y < h {
                    img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
                }
            }
        }
        DynamicImage::ImageRgba8(img)
    }

    // --- HSV tests ---

    #[test]
    fn test_rgb_to_hsv_red() {
        let (h, s, v) = rgb_to_hsv(255, 0, 0);
        assert!((h - 0.0).abs() < 1.0);
        assert!((s - 1.0).abs() < 0.01);
        assert!((v - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_rgb_to_hsv_green() {
        let (h, s, v) = rgb_to_hsv(0, 255, 0);
        assert!((h - 120.0).abs() < 1.0);
        assert!((s - 1.0).abs() < 0.01);
        assert!((v - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_rgb_to_hsv_blue() {
        let (h, s, v) = rgb_to_hsv(0, 0, 255);
        assert!((h - 240.0).abs() < 1.0);
        assert!((s - 1.0).abs() < 0.01);
        assert!((v - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_rgb_to_hsv_white() {
        let (h, s, v) = rgb_to_hsv(255, 255, 255);
        assert!((s - 0.0).abs() < 0.01);
        assert!((v - 1.0).abs() < 0.01);
        let _ = h; // hue is undefined for white
    }

    #[test]
    fn test_rgb_to_hsv_black() {
        let (h, s, v) = rgb_to_hsv(0, 0, 0);
        assert!((s - 0.0).abs() < 0.01);
        assert!((v - 0.0).abs() < 0.01);
        let _ = h;
    }

    // --- Green alpha tests ---

    #[test]
    fn test_green_alpha_pure_green() {
        // Pure green (#00FF00) should be fully transparent
        let alpha = compute_green_alpha(0, 255, 0);
        assert_eq!(alpha, 0);
    }

    #[test]
    fn test_green_alpha_red() {
        // Red should be fully opaque
        let alpha = compute_green_alpha(255, 0, 0);
        assert_eq!(alpha, 255);
    }

    #[test]
    fn test_green_alpha_white() {
        // White should be fully opaque (low saturation)
        let alpha = compute_green_alpha(255, 255, 255);
        assert_eq!(alpha, 255);
    }

    #[test]
    fn test_green_alpha_black() {
        // Black should be fully opaque (low value)
        let alpha = compute_green_alpha(0, 0, 0);
        assert_eq!(alpha, 255);
    }

    #[test]
    fn test_green_alpha_dark_green_low_saturation() {
        // Dark green (#006400) — S=1.0 but V=0.39 (low brightness)
        // With tighter thresholds, this should still be partially transparent
        // as it has high saturation
        let alpha = compute_green_alpha(0, 100, 0);
        assert!(alpha < 200, "dark saturated green should have reduced alpha, got {alpha}");
    }

    #[test]
    fn test_green_alpha_logo_dark_green() {
        // A typical logo dark green (#2D5A27) — low saturation, should be PRESERVED
        // H≈110, S≈0.57, V≈0.35
        let alpha = compute_green_alpha(45, 90, 39);
        assert!(alpha > 128, "logo dark green should be mostly opaque, got {alpha}");
    }

    #[test]
    fn test_green_alpha_teal() {
        // Teal (#008080) — H=180, outside green hue range, should be preserved
        let alpha = compute_green_alpha(0, 128, 128);
        assert_eq!(alpha, 255, "teal should be fully opaque");
    }

    #[test]
    fn test_green_alpha_yellow_green() {
        // Yellow-green (#ADFF2F) — H≈84, on the edge of green range
        let alpha = compute_green_alpha(173, 255, 47);
        // Should be partially transparent (it's greenish with high saturation)
        assert!(alpha < 255, "yellow-green should have reduced alpha, got {alpha}");
    }

    // --- Trim tests ---

    #[test]
    fn test_trim_transparent_borders_basic() {
        // 100x100 image with a 20x20 red square at center (40-59, 40-59)
        let mut img = RgbaImage::from_pixel(100, 100, Rgba([0, 0, 0, 0]));
        for y in 40..60 {
            for x in 40..60 {
                img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }
        let dyn_img = DynamicImage::ImageRgba8(img);
        let trimmed = trim_transparent_borders(&dyn_img, 0);
        let (w, h) = trimmed.dimensions();
        assert_eq!(w, 20, "trimmed width should be 20, got {w}");
        assert_eq!(h, 20, "trimmed height should be 20, got {h}");
    }

    #[test]
    fn test_trim_transparent_borders_with_padding() {
        let mut img = RgbaImage::from_pixel(100, 100, Rgba([0, 0, 0, 0]));
        for y in 40..60 {
            for x in 40..60 {
                img.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }
        let dyn_img = DynamicImage::ImageRgba8(img);
        let trimmed = trim_transparent_borders(&dyn_img, 5);
        let (w, h) = trimmed.dimensions();
        // 20px content + 5px padding each side = 30
        assert_eq!(w, 30, "trimmed width with padding should be 30, got {w}");
        assert_eq!(h, 30, "trimmed height with padding should be 30, got {h}");
    }

    #[test]
    fn test_trim_transparent_borders_all_transparent() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(50, 50, Rgba([0, 0, 0, 0])));
        let trimmed = trim_transparent_borders(&img, 0);
        // Should return the same image (all transparent)
        assert_eq!(trimmed.dimensions(), (50, 50));
    }

    #[test]
    fn test_trim_transparent_borders_no_transparent() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(50, 50, Rgba([255, 0, 0, 255])));
        let trimmed = trim_transparent_borders(&img, 0);
        assert_eq!(trimmed.dimensions(), (50, 50));
    }

    #[test]
    fn test_trim_transparent_borders_empty() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(0, 0));
        let trimmed = trim_transparent_borders(&img, 0);
        assert_eq!(trimmed.dimensions(), (0, 0));
    }

    // --- Chromakey removal tests ---

    #[test]
    fn test_remove_green_background_basic() {
        let img = green_image_with_red_center(100, 100);
        let result = remove_green_background(&img);
        let rgba = result.to_rgba8();

        // Green corners should be transparent
        assert_eq!(rgba.get_pixel(0, 0)[3], 0, "top-left green should be transparent");
        assert_eq!(rgba.get_pixel(99, 0)[3], 0, "top-right green should be transparent");
        assert_eq!(rgba.get_pixel(0, 99)[3], 0, "bottom-left green should be transparent");
        assert_eq!(rgba.get_pixel(99, 99)[3], 0, "bottom-right green should be transparent");

        // Center red square should be opaque
        assert_eq!(rgba.get_pixel(50, 50)[3], 255, "center red should be opaque");
        assert_eq!(rgba.get_pixel(50, 50)[0], 255, "center should still be red");
    }

    #[test]
    fn test_remove_green_background_empty() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(0, 0));
        let result = remove_green_background(&img);
        assert_eq!(result.dimensions(), (0, 0));
    }

    #[test]
    fn test_remove_green_background_no_green() {
        // All blue image — nothing should change
        let img =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(50, 50, Rgba([0, 0, 255, 255])));
        let result = remove_green_background(&img);
        let rgba = result.to_rgba8();

        for pixel in rgba.pixels() {
            assert_eq!(pixel[3], 255, "non-green pixels should remain opaque");
        }
    }

    #[test]
    fn test_remove_green_background_all_green() {
        // All green image — everything should become transparent
        let img =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(50, 50, Rgba([0, 255, 0, 255])));
        let result = remove_green_background(&img);
        let rgba = result.to_rgba8();

        for pixel in rgba.pixels() {
            assert_eq!(pixel[3], 0, "all green pixels should be transparent");
        }
    }

    // --- Flood fill tests (existing) ---

    #[test]
    fn test_color_distance_same() {
        let a = Rgba([100, 100, 100, 255]);
        let b = Rgba([100, 100, 100, 255]);
        assert_eq!(color_distance(&a, &b), 0.0);
    }

    #[test]
    fn test_color_distance_different() {
        let a = Rgba([0, 0, 0, 255]);
        let b = Rgba([255, 255, 255, 255]);
        let dist = color_distance(&a, &b);
        assert!((dist - 441.67).abs() < 0.1);
    }

    #[test]
    fn test_find_background_color_unanimous() {
        let white = Rgba([255, 255, 255, 255]);
        let corners = [white, white, white, white];
        let bg = find_background_color(&corners);
        assert_eq!(bg, white);
    }

    #[test]
    fn test_find_background_color_majority() {
        let white = Rgba([255, 255, 255, 255]);
        let red = Rgba([255, 0, 0, 255]);
        let corners = [white, white, white, red];
        let bg = find_background_color(&corners);
        assert_eq!(bg, white);
    }

    #[test]
    fn test_remove_background_white_bg() {
        let img = white_image_with_red_center(100, 100);
        let result = remove_background_from_image(&img, 30);
        let rgba = result.to_rgba8();

        assert_eq!(rgba.get_pixel(0, 0)[3], 0);
        assert_eq!(rgba.get_pixel(99, 0)[3], 0);
        assert_eq!(rgba.get_pixel(0, 99)[3], 0);
        assert_eq!(rgba.get_pixel(99, 99)[3], 0);

        assert_eq!(rgba.get_pixel(50, 50)[3], 255);
        assert_eq!(rgba.get_pixel(50, 50)[0], 255);
    }

    #[test]
    fn test_remove_background_zero_tolerance() {
        let img = white_image_with_red_center(100, 100);
        let result = remove_background_from_image(&img, 0);
        let rgba = result.to_rgba8();

        assert_eq!(rgba.get_pixel(0, 0)[3], 0);
        assert_eq!(rgba.get_pixel(50, 50)[3], 255);
    }

    #[test]
    fn test_remove_background_empty_image() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(0, 0));
        let result = remove_background_from_image(&img, 30);
        assert_eq!(result.dimensions(), (0, 0));
    }

    #[test]
    fn test_remove_background_solid_color() {
        let img =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(50, 50, Rgba([0, 0, 255, 255])));
        let result = remove_background_from_image(&img, 30);
        let rgba = result.to_rgba8();

        for pixel in rgba.pixels() {
            assert_eq!(pixel[3], 0);
        }
    }
}
