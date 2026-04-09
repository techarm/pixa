//! Gemini Watermark Removal via Reverse Alpha Blending
//!
//! Based on the algorithm from GeminiWatermarkTool by Allen Kuo (MIT License)
//! https://github.com/allenk/GeminiWatermarkTool
//!
//! Gemini applies watermarks using alpha blending:
//!   watermarked = α × logo + (1 - α) × original
//!
//! We reverse this to recover the original:
//!   original = (watermarked - α × logo) / (1 - α)
//!
//! Detection uses a three-stage algorithm:
//!   Stage 1: Spatial NCC - Normalized cross-correlation with alpha map
//!   Stage 2: Gradient NCC - Edge signature correlation (Sobel)
//!   Stage 3: Variance Analysis - Texture dampening detection

use image::{DynamicImage, GenericImageView, RgbImage};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info};

/// Embedded alpha map assets (captured watermark patterns on black background)
const BG_48_PNG: &[u8] = include_bytes!("../assets/watermark_48x48.png");
const BG_96_PNG: &[u8] = include_bytes!("../assets/watermark_96x96.png");

#[derive(Error, Debug)]
pub enum WatermarkError {
    #[error("Failed to decode alpha map: {0}")]
    AlphaMapDecode(String),
    #[error("Image too small for watermark removal: {0}x{1}")]
    ImageTooSmall(u32, u32),
    #[error("Image processing error: {0}")]
    Processing(String),
}

/// Watermark size variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WatermarkSize {
    /// 48×48, for images where W ≤ 1024 or H ≤ 1024
    Small,
    /// 96×96, for images where W > 1024 and H > 1024
    Large,
}

impl WatermarkSize {
    pub fn logo_size(&self) -> u32 {
        match self {
            Self::Small => 48,
            Self::Large => 96,
        }
    }

    pub fn margin(&self) -> u32 {
        match self {
            Self::Small => 32,
            Self::Large => 64,
        }
    }
}

/// Watermark detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    pub detected: bool,
    pub confidence: f32,
    pub size: WatermarkSize,
    pub spatial_score: f32,
    pub gradient_score: f32,
    pub variance_score: f32,
}

/// Alpha map for watermark removal (single-channel float 0.0-1.0)
struct AlphaMap {
    data: Vec<f32>,
    width: u32,
    height: u32,
}

impl AlphaMap {
    /// Calculate alpha map from a background capture image
    /// alpha = max(R, G, B) / 255.0
    fn from_capture(img: &RgbImage) -> Self {
        let width = img.width();
        let height = img.height();
        let mut data = Vec::with_capacity((width * height) as usize);

        for y in 0..height {
            for x in 0..width {
                let pixel = img.get_pixel(x, y);
                let max_channel = pixel[0].max(pixel[1]).max(pixel[2]);
                data.push(max_channel as f32 / 255.0);
            }
        }

        Self {
            data,
            width,
            height,
        }
    }

    fn get(&self, x: u32, y: u32) -> f32 {
        self.data[(y * self.width + x) as usize]
    }
}

/// Main watermark engine
pub struct WatermarkEngine {
    alpha_small: AlphaMap, // 48×48
    alpha_large: AlphaMap, // 96×96
    logo_value: f32,       // Logo brightness (255.0 = white)
}

impl WatermarkEngine {
    /// Create a new engine with embedded alpha maps
    pub fn new() -> Result<Self, WatermarkError> {
        let bg_small = image::load_from_memory(BG_48_PNG)
            .map_err(|e| WatermarkError::AlphaMapDecode(format!("48x48: {e}")))?
            .to_rgb8();
        let bg_large = image::load_from_memory(BG_96_PNG)
            .map_err(|e| WatermarkError::AlphaMapDecode(format!("96x96: {e}")))?
            .to_rgb8();

        let alpha_small = AlphaMap::from_capture(&bg_small);
        let alpha_large = AlphaMap::from_capture(&bg_large);

        debug!(
            "Alpha maps loaded: small={}x{}, large={}x{}",
            alpha_small.width, alpha_small.height, alpha_large.width, alpha_large.height
        );

        Ok(Self {
            alpha_small,
            alpha_large,
            logo_value: 255.0,
        })
    }

    /// Determine watermark size from image dimensions
    ///
    /// Rules (matching GeminiWatermarkTool):
    ///   W > 1024 AND H > 1024 → Large (96×96, 64px margin)
    ///   Otherwise → Small (48×48, 32px margin)
    pub fn detect_size(width: u32, height: u32) -> WatermarkSize {
        if width > 1024 && height > 1024 {
            WatermarkSize::Large
        } else {
            WatermarkSize::Small
        }
    }

    /// Get the watermark position (top-left corner) for a given image
    fn get_position(img_w: u32, img_h: u32, size: WatermarkSize) -> (u32, u32) {
        let margin = size.margin();
        let logo = size.logo_size();
        (img_w - margin - logo, img_h - margin - logo)
    }

    /// Detect whether a watermark is present using three-stage analysis
    ///
    /// Stage 1: Spatial NCC (weight 0.50) - correlation with alpha map
    /// Stage 2: Gradient NCC (weight 0.30) - Sobel edge signature correlation
    /// Stage 3: Variance Analysis (weight 0.20) - texture dampening detection
    pub fn detect_watermark(
        &self,
        image: &DynamicImage,
        force_size: Option<WatermarkSize>,
    ) -> DetectionResult {
        let rgb = image.to_rgb8();
        let (w, h) = (rgb.width(), rgb.height());
        let size = force_size.unwrap_or_else(|| Self::detect_size(w, h));
        let alpha = self.get_alpha_map(size);
        let (px, py) = Self::get_position(w, h, size);

        // Check bounds
        if px + alpha.width > w || py + alpha.height > h {
            return DetectionResult {
                detected: false,
                confidence: 0.0,
                size,
                spatial_score: 0.0,
                gradient_score: 0.0,
                variance_score: 0.0,
            };
        }

        // Convert watermark region to grayscale float [0, 1]
        let region_gray = extract_gray_region(&rgb, px, py, alpha.width, alpha.height);

        // Stage 1: Spatial NCC
        let spatial_score = compute_ncc(&region_gray, alpha);

        // Circuit breaker: reject early if spatial correlation is too low
        const SPATIAL_THRESHOLD: f32 = 0.25;
        if spatial_score < SPATIAL_THRESHOLD {
            debug!(
                "Detection: spatial={:.3} < {:.2}, rejected",
                spatial_score, SPATIAL_THRESHOLD
            );
            return DetectionResult {
                detected: false,
                confidence: spatial_score * 0.5,
                size,
                spatial_score,
                gradient_score: 0.0,
                variance_score: 0.0,
            };
        }

        // Stage 2: Gradient NCC (Sobel edge signature)
        let gradient_score = self.compute_gradient_ncc(&region_gray, alpha);

        // Stage 3: Variance analysis (texture dampening)
        let variance_score = self.compute_variance_score(&rgb, px, py, size.logo_size());

        // Weighted fusion (matching GeminiWatermarkTool)
        let confidence =
            (spatial_score * 0.50 + gradient_score * 0.30 + variance_score * 0.20).clamp(0.0, 1.0);

        // Detection threshold (matching GeminiWatermarkTool: 0.35)
        const DETECTION_THRESHOLD: f32 = 0.35;
        let detected = confidence >= DETECTION_THRESHOLD;

        debug!(
            "Detection: spatial={:.3}, grad={:.3}, var={:.3} -> conf={:.3} ({})",
            spatial_score,
            gradient_score,
            variance_score,
            confidence,
            if detected { "DETECTED" } else { "not detected" }
        );

        DetectionResult {
            detected,
            confidence,
            size,
            spatial_score,
            gradient_score,
            variance_score,
        }
    }

    /// Stage 2: Gradient NCC using Sobel edge detection
    ///
    /// Compute gradient magnitude for both image region and alpha map,
    /// then calculate NCC between them.
    fn compute_gradient_ncc(&self, region_gray: &GrayRegion, alpha: &AlphaMap) -> f32 {
        let w = region_gray.width as usize;
        let h = region_gray.height as usize;

        if w < 3 || h < 3 {
            return 0.0;
        }

        // Compute gradient magnitude for image region
        let img_grad = compute_gradient_magnitude(&region_gray.data, w, h);

        // Compute gradient magnitude for alpha map
        let alpha_grad = compute_gradient_magnitude(&alpha.data, w, h);

        // NCC between gradient magnitudes (on the (w-2)×(h-2) interior)
        let grad_w = w - 2;
        let grad_h = h - 2;
        let n = (grad_w * grad_h) as f64;

        if n < 1.0 {
            return 0.0;
        }

        let mut sum_img = 0.0f64;
        let mut sum_alpha = 0.0f64;
        for i in 0..img_grad.len() {
            sum_img += img_grad[i] as f64;
            sum_alpha += alpha_grad[i] as f64;
        }
        let mean_img = sum_img / n;
        let mean_alpha = sum_alpha / n;

        let mut numerator = 0.0f64;
        let mut denom_img = 0.0f64;
        let mut denom_alpha = 0.0f64;
        for i in 0..img_grad.len() {
            let di = img_grad[i] as f64 - mean_img;
            let da = alpha_grad[i] as f64 - mean_alpha;
            numerator += di * da;
            denom_img += di * di;
            denom_alpha += da * da;
        }

        let denom = (denom_img * denom_alpha).sqrt();
        if denom < 1e-10 {
            return 0.0;
        }

        (numerator / denom).clamp(0.0, 1.0) as f32
    }

    /// Stage 3: Variance dampening score using standard deviation
    ///
    /// Watermarks reduce texture variance in the affected region.
    /// Compare stddev of watermark region with reference region above it.
    fn compute_variance_score(&self, img: &RgbImage, px: u32, py: u32, logo_size: u32) -> f32 {
        // Reference region: area just above the watermark, limited to logo_size height
        let ref_h = py.min(logo_size);

        // Require minimum reference region (matching C++: ref_h > 8)
        if ref_h <= 8 {
            return 0.0;
        }

        let ref_y = py - ref_h;

        // Standard deviation of watermark region
        let wm_stddev = region_stddev(img, px, py, logo_size, logo_size);

        // Standard deviation of reference region
        let ref_stddev = region_stddev(img, px, ref_y, logo_size, ref_h);

        // Threshold on reference stddev (matching C++: > 5.0 for stddev)
        if ref_stddev > 5.0 {
            (1.0 - (wm_stddev / ref_stddev) as f32).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn get_alpha_map(&self, size: WatermarkSize) -> &AlphaMap {
        match size {
            WatermarkSize::Small => &self.alpha_small,
            WatermarkSize::Large => &self.alpha_large,
        }
    }

    /// Remove watermark from an image
    pub fn remove_watermark(
        &self,
        image: &mut DynamicImage,
        force_size: Option<WatermarkSize>,
    ) -> Result<(), WatermarkError> {
        let (w, h) = image.dimensions();
        let size = force_size.unwrap_or_else(|| Self::detect_size(w, h));
        let alpha = self.get_alpha_map(size);
        let (px, py) = Self::get_position(w, h, size);

        if px + alpha.width > w || py + alpha.height > h {
            return Err(WatermarkError::ImageTooSmall(w, h));
        }

        info!(
            "Removing {}x{} watermark at ({}, {})",
            alpha.width, alpha.height, px, py
        );

        let mut rgb = image.to_rgb8();
        self.apply_reverse_alpha_blend(&mut rgb, px, py, alpha);
        *image = DynamicImage::ImageRgb8(rgb);

        Ok(())
    }

    /// Core algorithm: Reverse Alpha Blending
    ///
    /// original = (watermarked - α × logo_value) / (1 - α)
    fn apply_reverse_alpha_blend(
        &self,
        image: &mut RgbImage,
        px: u32,
        py: u32,
        alpha_map: &AlphaMap,
    ) {
        const ALPHA_THRESHOLD: f32 = 0.002; // Ignore negligible alpha
        const MAX_ALPHA: f32 = 0.99; // Avoid division by near-zero

        for y in 0..alpha_map.height {
            for x in 0..alpha_map.width {
                let alpha = alpha_map.get(x, y);

                if alpha < ALPHA_THRESHOLD {
                    continue;
                }

                let alpha = alpha.min(MAX_ALPHA);
                let one_minus_alpha = 1.0 - alpha;
                let pixel = image.get_pixel_mut(px + x, py + y);

                for c in 0..3 {
                    let watermarked = pixel[c] as f32;
                    let original = (watermarked - alpha * self.logo_value) / one_minus_alpha;
                    pixel[c] = original.clamp(0.0, 255.0) as u8;
                }
            }
        }
    }

    /// Add watermark to an image (for testing/preview)
    pub fn add_watermark(
        &self,
        image: &mut DynamicImage,
        force_size: Option<WatermarkSize>,
    ) -> Result<(), WatermarkError> {
        let (w, h) = image.dimensions();
        let size = force_size.unwrap_or_else(|| Self::detect_size(w, h));
        let alpha = self.get_alpha_map(size);
        let (px, py) = Self::get_position(w, h, size);

        if px + alpha.width > w || py + alpha.height > h {
            return Err(WatermarkError::ImageTooSmall(w, h));
        }

        let mut rgb = image.to_rgb8();
        self.apply_alpha_blend(&mut rgb, px, py, alpha);
        *image = DynamicImage::ImageRgb8(rgb);

        Ok(())
    }

    /// Forward alpha blending (same as Gemini)
    /// result = α × logo + (1 - α) × original
    fn apply_alpha_blend(&self, image: &mut RgbImage, px: u32, py: u32, alpha_map: &AlphaMap) {
        const ALPHA_THRESHOLD: f32 = 0.002;

        for y in 0..alpha_map.height {
            for x in 0..alpha_map.width {
                let alpha = alpha_map.get(x, y);

                if alpha < ALPHA_THRESHOLD {
                    continue;
                }

                let one_minus_alpha = 1.0 - alpha;
                let pixel = image.get_pixel_mut(px + x, py + y);

                for c in 0..3 {
                    let original = pixel[c] as f32;
                    let result = alpha * self.logo_value + one_minus_alpha * original;
                    pixel[c] = result.clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
}

impl Default for WatermarkEngine {
    fn default() -> Self {
        Self::new().expect("Failed to initialize watermark engine")
    }
}

// =============================================================================
// Helper types and functions
// =============================================================================

/// Grayscale region extracted from image (float [0, 1])
struct GrayRegion {
    data: Vec<f32>,
    width: u32,
    height: u32,
}

/// Extract a grayscale region from an RGB image, normalized to [0, 1]
fn extract_gray_region(img: &RgbImage, x: u32, y: u32, w: u32, h: u32) -> GrayRegion {
    let mut data = Vec::with_capacity((w * h) as usize);
    for dy in 0..h {
        for dx in 0..w {
            let pixel = img.get_pixel(x + dx, y + dy);
            let lum = (0.299 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.114 * pixel[2] as f32)
                / 255.0;
            data.push(lum);
        }
    }
    GrayRegion {
        data,
        width: w,
        height: h,
    }
}

/// Compute Normalized Cross-Correlation between a grayscale region and an alpha map
fn compute_ncc(region: &GrayRegion, alpha: &AlphaMap) -> f32 {
    let n = (alpha.width * alpha.height) as f64;

    let mut sum_img = 0.0f64;
    let mut sum_alpha = 0.0f64;

    for i in 0..region.data.len() {
        sum_img += region.data[i] as f64;
        sum_alpha += alpha.data[i] as f64;
    }

    let mean_img = sum_img / n;
    let mean_alpha = sum_alpha / n;

    let mut numerator = 0.0f64;
    let mut denom_img = 0.0f64;
    let mut denom_alpha = 0.0f64;

    for i in 0..region.data.len() {
        let di = region.data[i] as f64 - mean_img;
        let da = alpha.data[i] as f64 - mean_alpha;
        numerator += di * da;
        denom_img += di * di;
        denom_alpha += da * da;
    }

    let denom = (denom_img * denom_alpha).sqrt();
    if denom < 1e-10 {
        return 0.0;
    }

    (numerator / denom).clamp(0.0, 1.0) as f32
}

/// Compute gradient magnitude using Sobel operators
///
/// Returns a (w-2)×(h-2) gradient magnitude map (interior pixels only)
///
/// Sobel kernels:
///   Gx = [[-1, 0, 1], [-2, 0, 2], [-1, 0, 1]]
///   Gy = [[-1, -2, -1], [0, 0, 0], [1, 2, 1]]
fn compute_gradient_magnitude(data: &[f32], w: usize, h: usize) -> Vec<f32> {
    let out_w = w - 2;
    let out_h = h - 2;
    let mut grad = Vec::with_capacity(out_w * out_h);

    for y in 1..h - 1 {
        for x in 1..w - 1 {
            // Sobel Gx
            let gx = -data[(y - 1) * w + (x - 1)] + data[(y - 1) * w + (x + 1)]
                - 2.0 * data[y * w + (x - 1)]
                + 2.0 * data[y * w + (x + 1)]
                - data[(y + 1) * w + (x - 1)]
                + data[(y + 1) * w + (x + 1)];

            // Sobel Gy
            let gy = -data[(y - 1) * w + (x - 1)]
                - 2.0 * data[(y - 1) * w + x]
                - data[(y - 1) * w + (x + 1)]
                + data[(y + 1) * w + (x - 1)]
                + 2.0 * data[(y + 1) * w + x]
                + data[(y + 1) * w + (x + 1)];

            grad.push((gx * gx + gy * gy).sqrt());
        }
    }

    grad
}

/// Compute standard deviation of luminance in a region
fn region_stddev(img: &RgbImage, x: u32, y: u32, w: u32, h: u32) -> f64 {
    if w == 0 || h == 0 {
        return 0.0;
    }
    let (iw, ih) = (img.width(), img.height());
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let mut count = 0u64;

    for dy in 0..h {
        for dx in 0..w {
            let cx = x + dx;
            let cy = y + dy;
            if cx < iw && cy < ih {
                let p = img.get_pixel(cx, cy);
                let lum = 0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64;
                sum += lum;
                sum_sq += lum * lum;
                count += 1;
            }
        }
    }

    if count < 2 {
        return 0.0;
    }
    let n = count as f64;
    let variance = (sum_sq / n) - (sum / n).powi(2);
    // Return standard deviation (matching C++ cv::meanStdDev)
    variance.max(0.0).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageBuffer;
    use image::Rgb;

    #[test]
    fn test_size_detection() {
        assert_eq!(WatermarkEngine::detect_size(800, 600), WatermarkSize::Small);
        assert_eq!(
            WatermarkEngine::detect_size(1024, 1024),
            WatermarkSize::Small
        );
        assert_eq!(
            WatermarkEngine::detect_size(1920, 1080),
            WatermarkSize::Large
        );
        assert_eq!(
            WatermarkEngine::detect_size(800, 1200),
            WatermarkSize::Small
        );
    }

    #[test]
    fn test_engine_creation() {
        let engine = WatermarkEngine::new();
        assert!(engine.is_ok());
    }

    #[test]
    fn test_roundtrip() {
        let engine = WatermarkEngine::new().unwrap();
        // Create a test image
        let mut img =
            DynamicImage::ImageRgb8(ImageBuffer::from_fn(512, 512, |_, _| Rgb([128u8, 100, 80])));
        let original = img.clone();

        // Add then remove watermark
        engine
            .add_watermark(&mut img, Some(WatermarkSize::Small))
            .unwrap();
        engine
            .remove_watermark(&mut img, Some(WatermarkSize::Small))
            .unwrap();

        // Compare - should be very close (within rounding error)
        let orig_rgb = original.to_rgb8();
        let result_rgb = img.to_rgb8();
        let mut max_diff = 0u8;
        for (o, r) in orig_rgb.pixels().zip(result_rgb.pixels()) {
            for c in 0..3 {
                let diff = (o[c] as i16 - r[c] as i16).unsigned_abs() as u8;
                max_diff = max_diff.max(diff);
            }
        }
        // 8-bit quantization allows ±2 difference (double rounding: blend + reverse)
        assert!(max_diff <= 2, "Max pixel difference: {max_diff}");
    }

    #[test]
    fn test_sobel_gradient() {
        // Simple 5x5 image with a sharp edge
        let data = vec![
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0,
            1.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0,
        ];
        let grad = compute_gradient_magnitude(&data, 5, 5);
        // Interior is 3x3, gradient should be non-zero at edges
        assert_eq!(grad.len(), 9);
        assert!(grad.iter().any(|&g| g > 0.0), "Should detect edges");
    }
}
