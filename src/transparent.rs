//! Replace a solid background color with transparency.
//!
//! Intended for AI-generated icon/mascot images where the subject sits
//! on a single uniform color (e.g. magenta `#FF00FF` or chroma green).
//! Generating on a solid color and keying it out afterwards produces
//! more consistent edges than asking the model for a transparent PNG
//! directly.
//!
//! The algorithm is **connectivity-based matting**:
//!
//!   1. From each of the four image corners, flood-fill outward
//!      through pixels whose RGB distance to the background colour is
//!      below `tolerance + edge_width`. The flooded region is the
//!      *true* background — a designed element with a magenta tint
//!      buried inside the foreground (e.g. a cloud's purple drop
//!      shadow) is **not** connected to the corners and therefore
//!      survives as opaque.
//!   2. For flooded pixels:
//!        - distance ≤ `tolerance` → alpha 0
//!        - distance in the edge ring → GIMP-style color-to-alpha
//!          (per-channel alpha + inverse-composite decontamination)
//!        - distance > edge ring (reached by flood but colour is far
//!          from bg) → alpha 255 with colour untouched
//!   3. Non-flooded pixels pass through completely unchanged, even if
//!      their RGB happens to be close to the key colour. This is what
//!      lets designed near-key colours ride through without damage.
//!
//! The color-to-alpha step inverts the alpha-compositing equation:
//! given `observed = alpha * foreground + (1 - alpha) * background`,
//! the decontaminated foreground is
//! `(observed - (1 - alpha) * background) / alpha`.

use image::{DynamicImage, GenericImageView, Rgb, RgbaImage};
use std::collections::VecDeque;
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

/// Apply connectivity-based chroma-key transparency to an RGBA image
/// in place. See the module-level docs for the algorithm.
pub fn apply_transparency_to_rgba(
    img: &mut RgbaImage,
    background: [u8; 3],
    tolerance: f64,
    edge_width: f64,
) {
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return;
    }
    let n = (w as usize) * (h as usize);
    let bg = [
        background[0] as f64,
        background[1] as f64,
        background[2] as f64,
    ];
    let tolerance = tolerance.max(0.0);
    let edge_width = edge_width.max(0.001);
    // Flood-fill reach: the outer bound on how close-to-bg a pixel must
    // be to count as background during connectivity analysis.
    let flood_reach = tolerance + edge_width;

    // Pass 1: per-pixel distance to bg + flood candidacy.
    let mut dist = vec![0.0f64; n];
    let mut is_candidate = vec![false; n];
    for y in 0..h {
        for x in 0..w {
            let i = (y as usize) * (w as usize) + (x as usize);
            let p = img.get_pixel(x, y);
            if p[3] == 0 {
                // Already transparent → treat as bg for flood purposes.
                is_candidate[i] = true;
                continue;
            }
            let dr = p[0] as f64 - bg[0];
            let dg = p[1] as f64 - bg[1];
            let db = p[2] as f64 - bg[2];
            let d = (dr * dr + dg * dg + db * db).sqrt();
            dist[i] = d;
            is_candidate[i] = d < flood_reach;
        }
    }

    // Pass 2: flood-fill from the four corners (4-connectivity) through
    // candidates only.
    let mut flooded = vec![false; n];
    let mut queue: VecDeque<(u32, u32)> = VecDeque::new();
    let corners = [(0u32, 0u32), (w - 1, 0), (0, h - 1), (w - 1, h - 1)];
    for &(x, y) in &corners {
        let i = (y as usize) * (w as usize) + (x as usize);
        if is_candidate[i] && !flooded[i] {
            flooded[i] = true;
            queue.push_back((x, y));
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        let nbrs: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
        for (dx, dy) in nbrs {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                continue;
            }
            let ni = (ny as usize) * (w as usize) + (nx as usize);
            if is_candidate[ni] && !flooded[ni] {
                flooded[ni] = true;
                queue.push_back((nx as u32, ny as u32));
            }
        }
    }

    // Pass 3: classify and write each pixel.
    //
    // Denominator clamp for color-to-alpha: when bg has a near-extreme
    // channel (e.g. G≈0 for pure magenta), tiny observation noise in
    // that channel would otherwise amplify into huge alpha.
    const MIN_DENOM: f64 = 32.0;
    for y in 0..h {
        for x in 0..w {
            let i = (y as usize) * (w as usize) + (x as usize);
            let p = img.get_pixel_mut(x, y);
            let src_alpha = p[3] as f64 / 255.0;
            if src_alpha <= 0.0 {
                *p = image::Rgba([0, 0, 0, 0]);
                continue;
            }

            if !flooded[i] {
                // Interior pixel (not connected to corner bg): pass
                // through unchanged, even if its RGB happens to be
                // close to the key colour.
                let a = (src_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
                *p = image::Rgba([p[0], p[1], p[2], a]);
                continue;
            }

            let d = dist[i];
            if d <= tolerance {
                *p = image::Rgba([0, 0, 0, 0]);
                continue;
            }

            // Flooded, in edge ring → color-to-alpha.
            let obs = [p[0] as f64, p[1] as f64, p[2] as f64];
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
    fn near_bg_pixel_surrounded_by_foreground_stays_opaque() {
        // A solid black square on a magenta background, with one
        // near-magenta pixel placed in the middle of the black square.
        // Because that pixel is NOT connected to the corner bg via a
        // chain of near-bg pixels, flood-fill matting must preserve
        // it instead of keying it out.
        let mut img = solid(20, 20, [255, 0, 255]);
        for y in 5..15 {
            for x in 5..15 {
                img.put_pixel(x, y, Rgb([0, 0, 0]));
            }
        }
        // One interior pixel that happens to be near-magenta — this
        // simulates a designer's pink/magenta detail inside the subject.
        img.put_pixel(10, 10, Rgb([240, 20, 240]));
        let (out, _) = apply_transparency(
            &DynamicImage::ImageRgb8(img),
            &TransparentOptions::default(),
        )
        .unwrap();
        let px = out.get_pixel(10, 10);
        assert_eq!(px[3], 255, "interior near-bg pixel must stay opaque");
        assert_eq!((px[0], px[1], px[2]), (240, 20, 240));
    }

    #[test]
    fn near_bg_pixel_connected_to_corner_gets_keyed_out() {
        // Same magenta-with-near-magenta setup, but the near-magenta
        // region now touches the image border — so it's connected to
        // the corner background and must be keyed out.
        let mut img = solid(20, 20, [255, 0, 255]);
        for x in 0..20 {
            img.put_pixel(x, 0, Rgb([240, 20, 240]));
        }
        let (out, _) = apply_transparency(
            &DynamicImage::ImageRgb8(img),
            &TransparentOptions::default(),
        )
        .unwrap();
        // Top row pixels are connected to the corners via other top-
        // row pixels that are exactly bg → flood reaches them.
        for x in 0..20 {
            let px = out.get_pixel(x, 0);
            assert!(
                px[3] < 128,
                "top-row x={x} should be at least semi-transparent, got alpha={}",
                px[3]
            );
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
