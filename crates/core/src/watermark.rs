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

use image::{DynamicImage, GenericImageView, ImageBuffer, Rgb, RgbImage};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info};

/// Embedded alpha map assets (captured watermark patterns on black background)
const BG_48_PNG: &[u8] = include_bytes!("../../../assets/watermark_48x48.png");
const BG_96_PNG: &[u8] = include_bytes!("../../../assets/watermark_96x96.png");

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

    /// Detect whether a watermark is present using NCC + variance analysis
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
                variance_score: 0.0,
            };
        }

        // Stage 1: Spatial NCC - Compare watermark region luminance with alpha map
        let spatial_score = self.compute_spatial_ncc(&rgb, px, py, alpha);

        // Stage 2: Variance analysis - Watermarks dampen background variance
        let variance_score = self.compute_variance_score(&rgb, px, py, alpha);

        // Weighted fusion
        let confidence = (spatial_score * 0.65 + variance_score * 0.35).clamp(0.0, 1.0);
        let detected = confidence >= 0.25;

        debug!(
            "Detection: spatial={:.3}, var={:.3} -> conf={:.3} ({})",
            spatial_score,
            variance_score,
            confidence,
            if detected { "DETECTED" } else { "not detected" }
        );

        DetectionResult {
            detected,
            confidence,
            size,
            spatial_score,
            variance_score,
        }
    }

    /// Compute Normalized Cross-Correlation between image region and alpha map
    fn compute_spatial_ncc(&self, img: &RgbImage, px: u32, py: u32, alpha: &AlphaMap) -> f32 {
        let mut sum_img = 0.0f64;
        let mut sum_alpha = 0.0f64;
        let n = (alpha.width * alpha.height) as f64;

        // Compute means
        for y in 0..alpha.height {
            for x in 0..alpha.width {
                let pixel = img.get_pixel(px + x, py + y);
                let lum =
                    0.299 * pixel[0] as f64 + 0.587 * pixel[1] as f64 + 0.114 * pixel[2] as f64;
                sum_img += lum;
                sum_alpha += alpha.get(x, y) as f64;
            }
        }
        let mean_img = sum_img / n;
        let mean_alpha = sum_alpha / n;

        // Compute NCC
        let mut numerator = 0.0f64;
        let mut denom_img = 0.0f64;
        let mut denom_alpha = 0.0f64;

        for y in 0..alpha.height {
            for x in 0..alpha.width {
                let pixel = img.get_pixel(px + x, py + y);
                let lum =
                    0.299 * pixel[0] as f64 + 0.587 * pixel[1] as f64 + 0.114 * pixel[2] as f64;
                let di = lum - mean_img;
                let da = alpha.get(x, y) as f64 - mean_alpha;
                numerator += di * da;
                denom_img += di * di;
                denom_alpha += da * da;
            }
        }

        let denom = (denom_img * denom_alpha).sqrt();
        if denom < 1e-10 {
            return 0.0;
        }

        (numerator / denom).clamp(0.0, 1.0) as f32
    }

    /// Compute variance dampening score
    fn compute_variance_score(&self, img: &RgbImage, px: u32, py: u32, alpha: &AlphaMap) -> f32 {
        let logo_size = alpha.width;

        // Variance of the watermark region
        let wm_var = self.region_variance(img, px, py, logo_size, logo_size);

        // Reference region: area just above the watermark
        let ref_y = if py >= logo_size { py - logo_size } else { 0 };
        let ref_var = self.region_variance(img, px, ref_y, logo_size, logo_size.min(py));

        if ref_var > 5.0 {
            (1.0 - (wm_var / ref_var) as f32).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn region_variance(&self, img: &RgbImage, x: u32, y: u32, w: u32, h: u32) -> f64 {
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
        (sum_sq / n) - (sum / n).powi(2)
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

#[cfg(test)]
mod tests {
    use super::*;

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
        // 8-bit quantization allows ±1 difference
        assert!(max_diff <= 1, "Max pixel difference: {max_diff}");
    }
}
