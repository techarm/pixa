//! Replace a solid background color with transparency.
//!
//! Intended for AI-generated icon/mascot images where the subject sits
//! on a single uniform color (e.g. magenta `#FF00FF` or chroma green).
//! Generating on a solid color and keying it out afterwards produces
//! more consistent edges than asking the model for a transparent PNG
//! directly.
//!
//! The core algorithm is a hybrid of distance-gating and GIMP-style
//! "color-to-alpha" edge handling:
//!
//!   - Pixels within `tolerance` RGB-distance of the background are
//!     forced to fully transparent (alpha 0). This cleanly keys out
//!     uniform and mildly-noisy backgrounds.
//!   - Pixels beyond `edge_width` distance are forced to fully opaque
//!     (alpha 255) and pass through unchanged. This protects solid
//!     foreground colours from being washed out — important when the
//!     subject's colour happens to share a channel with the background
//!     (blues on a magenta key, for example).
//!   - Pixels in the `(tolerance, tolerance + edge_width)` ring get
//!     the color-to-alpha treatment: per-channel alpha estimation plus
//!     inverse-composite decontamination, so anti-aliased edges stay
//!     smooth and do not show a coloured halo.
//!
//!     observed = alpha * foreground + (1 - alpha) * background
//!     foreground = (observed - (1 - alpha) * background) / alpha

use image::{DynamicImage, GenericImageView, Rgb, RgbaImage};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TransparentError {
    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Empty image")]
    Empty,
}

#[derive(Debug, Clone)]
pub struct TransparentOptions {
    /// Background color to key out. If `None`, auto-detected from the
    /// image's four corner patches.
    pub background: Option<[u8; 3]>,
    /// RGB-space distance at or below which a pixel is forced to alpha
    /// 0. Handles slightly noisy backgrounds (JPEG-compressed solids).
    /// Default: 12.0.
    pub tolerance: f64,
    /// Width of the anti-aliased edge ring (in RGB-space distance)
    /// beyond `tolerance`. Pixels whose distance falls in
    /// `(tolerance, tolerance + edge_width)` get per-pixel alpha and
    /// decontamination via color-to-alpha; pixels beyond this ring are
    /// treated as fully opaque. Default: 90.0.
    pub edge_width: f64,
}

impl Default for TransparentOptions {
    fn default() -> Self {
        Self {
            background: None,
            tolerance: 12.0,
            edge_width: 90.0,
        }
    }
}

/// Pixels per side sampled from each of the four corners when
/// auto-detecting the background color. Matches split.rs's setting so
/// the two modules agree on what "background" means.
pub const CORNER_PATCH: u32 = 20;

/// Result of a transparency pass.
#[derive(Debug, Clone)]
pub struct TransparentResult {
    /// The background color that was actually keyed out (either auto-
    /// detected or supplied via options).
    pub background: [u8; 3],
    /// Number of fully transparent pixels in the output.
    pub transparent_pixels: u64,
    /// Number of partially transparent (edge) pixels in the output.
    pub edge_pixels: u64,
    /// Number of fully opaque pixels in the output.
    pub opaque_pixels: u64,
}

/// Estimate the background color by taking the median of four corner
/// patches.
pub fn estimate_background(img: &image::RgbImage) -> [u8; 3] {
    let (w, h) = (img.width(), img.height());
    let patch = CORNER_PATCH.min(w / 4).min(h / 4).max(1);

    let mut r = Vec::new();
    let mut g = Vec::new();
    let mut b = Vec::new();
    let corners = [
        (0u32, 0u32),
        (w - patch, 0),
        (0, h - patch),
        (w - patch, h - patch),
    ];
    for (cx, cy) in corners {
        for y in cy..cy + patch {
            for x in cx..cx + patch {
                let p = img.get_pixel(x, y);
                r.push(p[0]);
                g.push(p[1]);
                b.push(p[2]);
            }
        }
    }
    [median(&mut r), median(&mut g), median(&mut b)]
}

fn median(v: &mut [u8]) -> u8 {
    v.sort_unstable();
    v[v.len() / 2]
}

/// Parse a `#RRGGBB` or `RRGGBB` string into an RGB triple.
pub fn parse_hex_color(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some([r, g, b])
}

/// Apply transparency to `img` according to `opts`, returning an RGBA
/// image with the background color keyed out.
pub fn apply_transparency(
    img: &DynamicImage,
    opts: &TransparentOptions,
) -> Result<(RgbaImage, TransparentResult), TransparentError> {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return Err(TransparentError::Empty);
    }

    let rgb = img.to_rgb8();
    let bg = opts.background.unwrap_or_else(|| estimate_background(&rgb));

    let mut out = img.to_rgba8();
    apply_transparency_to_rgba(&mut out, bg, opts.tolerance, opts.edge_width);

    let mut transparent_pixels = 0u64;
    let mut edge_pixels = 0u64;
    let mut opaque_pixels = 0u64;
    for p in out.pixels() {
        match p[3] {
            0 => transparent_pixels += 1,
            255 => opaque_pixels += 1,
            _ => edge_pixels += 1,
        }
    }

    Ok((
        out,
        TransparentResult {
            background: bg,
            transparent_pixels,
            edge_pixels,
            opaque_pixels,
        },
    ))
}

/// Apply chroma-key transparency to an RGBA image in place. See the
/// module-level docs for the algorithm.
pub fn apply_transparency_to_rgba(
    img: &mut RgbaImage,
    background: [u8; 3],
    tolerance: f64,
    edge_width: f64,
) {
    let bg = [
        background[0] as f64,
        background[1] as f64,
        background[2] as f64,
    ];
    let tolerance = tolerance.max(0.0);
    let edge_width = edge_width.max(0.001);
    let edge_max = tolerance + edge_width;

    for p in img.pixels_mut() {
        let src_alpha = p[3] as f64 / 255.0;
        if src_alpha <= 0.0 {
            *p = image::Rgba([0, 0, 0, 0]);
            continue;
        }

        let obs = [p[0] as f64, p[1] as f64, p[2] as f64];
        let dr = obs[0] - bg[0];
        let dg = obs[1] - bg[1];
        let db = obs[2] - bg[2];
        let dist = (dr * dr + dg * dg + db * db).sqrt();

        // Core: fully transparent within tolerance.
        if dist <= tolerance {
            *p = image::Rgba([0, 0, 0, 0]);
            continue;
        }

        // Far from bg: fully opaque, colour untouched.
        if dist >= edge_max {
            // Only need to update alpha if the source had pre-existing
            // partial alpha. Pass through otherwise.
            let a = (src_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
            *p = image::Rgba([p[0], p[1], p[2], a]);
            continue;
        }

        // Edge ring: use color-to-alpha to compute per-pixel alpha and
        // then invert the compositing equation to decontaminate RGB.
        // We clamp the denominator to `MIN_DENOM` so that channels
        // where the background is near an extreme (e.g. G≈0 for pure
        // magenta) don't amplify PNG/JPEG noise into spurious opacity.
        const MIN_DENOM: f64 = 32.0;
        let mut alpha_f: f64 = 0.0;
        for c in 0..3 {
            let a = if obs[c] > bg[c] {
                let denom = (255.0 - bg[c]).max(MIN_DENOM);
                (obs[c] - bg[c]) / denom
            } else if obs[c] < bg[c] {
                let denom = bg[c].max(MIN_DENOM);
                (bg[c] - obs[c]) / denom
            } else {
                0.0
            };
            if a > alpha_f {
                alpha_f = a;
            }
        }
        alpha_f = alpha_f.clamp(0.0, 1.0) * src_alpha;

        if alpha_f <= 0.0 {
            *p = image::Rgba([0, 0, 0, 0]);
            continue;
        }

        let one_minus_a = 1.0 - alpha_f;
        let fr = (obs[0] - one_minus_a * bg[0]) / alpha_f;
        let fg = (obs[1] - one_minus_a * bg[1]) / alpha_f;
        let fb = (obs[2] - one_minus_a * bg[2]) / alpha_f;
        let r = fr.clamp(0.0, 255.0).round() as u8;
        let g = fg.clamp(0.0, 255.0).round() as u8;
        let b = fb.clamp(0.0, 255.0).round() as u8;
        let alpha = (alpha_f * 255.0).round().clamp(0.0, 255.0) as u8;
        *p = image::Rgba([r, g, b, alpha]);
    }
}

/// Wrap a raw RGB triple as an `image::Rgb` — helper for callers that
/// already hold `split::SplitResult::background`.
pub fn rgb_from_triple(c: [u8; 3]) -> Rgb<u8> {
    Rgb(c)
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage, Rgba, RgbaImage};

    fn solid(w: u32, h: u32, c: [u8; 3]) -> RgbImage {
        RgbImage::from_pixel(w, h, Rgb(c))
    }

    #[test]
    fn parse_hex_color_handles_leading_hash() {
        assert_eq!(parse_hex_color("#FF00FF"), Some([255, 0, 255]));
        assert_eq!(parse_hex_color("ff00ff"), Some([255, 0, 255]));
        assert_eq!(parse_hex_color("#fffff"), None);
        assert_eq!(parse_hex_color("not a color"), None);
    }

    #[test]
    fn estimate_background_on_uniform_image() {
        let img = solid(100, 100, [255, 0, 255]);
        assert_eq!(estimate_background(&img), [255, 0, 255]);
    }

    #[test]
    fn pure_background_becomes_fully_transparent() {
        let img = DynamicImage::ImageRgb8(solid(40, 40, [255, 0, 255]));
        let (out, res) = apply_transparency(&img, &TransparentOptions::default()).unwrap();
        assert_eq!(res.background, [255, 0, 255]);
        // Every pixel is pure background, so all should be transparent.
        assert_eq!(res.opaque_pixels, 0);
        assert_eq!(res.edge_pixels, 0);
        assert_eq!(res.transparent_pixels as u32, out.width() * out.height());
        for p in out.pixels() {
            assert_eq!(p[3], 0);
        }
    }

    #[test]
    fn foreground_pixel_stays_opaque() {
        // Magenta background with one solid black pixel in the middle.
        let mut img = solid(20, 20, [255, 0, 255]);
        img.put_pixel(10, 10, Rgb([0, 0, 0]));
        let (out, res) = apply_transparency(
            &DynamicImage::ImageRgb8(img),
            &TransparentOptions::default(),
        )
        .unwrap();
        assert_eq!(res.background, [255, 0, 255]);
        let px = out.get_pixel(10, 10);
        assert_eq!(px[3], 255);
        assert_eq!(px[0], 0);
        assert_eq!(px[1], 0);
        assert_eq!(px[2], 0);
    }

    #[test]
    fn user_supplied_background_overrides_auto_detect() {
        // Image's corners are black — without override auto-detect
        // would pick black — but we explicitly ask for magenta.
        let img = solid(30, 30, [0, 0, 0]);
        let opts = TransparentOptions {
            background: Some([255, 0, 255]),
            ..Default::default()
        };
        let (out, res) = apply_transparency(&DynamicImage::ImageRgb8(img), &opts).unwrap();
        assert_eq!(res.background, [255, 0, 255]);
        // Black is far from magenta → fully opaque.
        for p in out.pixels() {
            assert_eq!(p[3], 255);
        }
    }

    #[test]
    fn respects_pre_existing_alpha() {
        // Source already has alpha=128 on foreground pixels.
        let mut rgba = RgbaImage::from_pixel(10, 10, Rgba([255, 0, 255, 255]));
        rgba.put_pixel(5, 5, Rgba([0, 0, 0, 128]));
        let (out, _) = apply_transparency(
            &DynamicImage::ImageRgba8(rgba),
            &TransparentOptions::default(),
        )
        .unwrap();
        let px = out.get_pixel(5, 5);
        // 255 (mask) * 128 (src) / 255 = 128
        assert_eq!(px[3], 128);
    }
}
