//! Replace a solid background colour with transparency.
//!
//! Intended for AI-generated icon/mascot images where the subject sits
//! on a single uniform colour (e.g. magenta `#FF00FF` or chroma green).
//! Generating on a solid colour and keying it out afterwards produces
//! more consistent results than asking the model for a transparent PNG
//! directly.
//!
//! ## Algorithm
//!
//! 1. Estimate (or use the user-supplied) background colour from four
//!    corner patches.
//! 2. Flood-fill (4-connectivity) from the four image corners through
//!    every pixel whose RGB-space distance from the background is at
//!    or below `tolerance`.
//! 3. Set the alpha of every flooded pixel to 0. Leave every other
//!    pixel completely untouched — colour and alpha are preserved
//!    exactly.
//!
//! That's it. No colour-to-alpha shifting, no spill-suppression
//! gymnastics, no soft alpha ramp. The contract is: "the background
//! becomes transparent, the rest does not change."
//!
//! ## Why connectivity matters
//!
//! A pixel that happens to share the background colour but is buried
//! inside the foreground (e.g. a designed pink sparkle on a magenta-
//! keyed image) is not reachable from the corners and therefore
//! survives. Only pixels that form a continuous bridge of near-bg
//! colour out to an image corner are removed.

use image::{DynamicImage, GenericImageView, RgbaImage};
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
    /// Background colour to key out. If `None`, auto-detected from the
    /// image's four corner patches.
    pub background: Option<[u8; 3]>,
    /// RGB-space distance at or below which a pixel counts as
    /// background for the flood fill. Wider values pick up more of the
    /// AA contamination ring at the subject's outer edge; too wide and
    /// the flood can chew through soft-edged subject regions whose
    /// colour gradient extends down to near-bg values. Default: 200.0,
    /// tuned for the chroma-key-friendly prompt template (no pinks,
    /// purples, or violets on the subject). For arbitrary inputs that
    /// do contain near-key designed colours, lower this (try 160).
    pub tolerance: f64,
}

impl Default for TransparentOptions {
    fn default() -> Self {
        Self {
            background: None,
            tolerance: 200.0,
        }
    }
}

/// Pixels per side sampled from each of the four corners when
/// auto-detecting the background colour.
pub const CORNER_PATCH: u32 = 20;

/// Result of a transparency pass.
#[derive(Debug, Clone)]
pub struct TransparentResult {
    /// The background colour that was actually keyed out.
    pub background: [u8; 3],
    /// Number of pixels turned fully transparent.
    pub transparent_pixels: u64,
    /// Number of pixels left untouched (their alpha was kept as-is).
    pub opaque_pixels: u64,
}

/// Estimate the background colour by taking the median of four corner
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
/// image with the background colour keyed out.
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
    apply_transparency_to_rgba(&mut out, bg, opts.tolerance);

    let mut transparent_pixels = 0u64;
    let mut opaque_pixels = 0u64;
    for p in out.pixels() {
        if p[3] == 0 {
            transparent_pixels += 1;
        } else {
            opaque_pixels += 1;
        }
    }

    Ok((
        out,
        TransparentResult {
            background: bg,
            transparent_pixels,
            opaque_pixels,
        },
    ))
}

/// Apply connectivity-based chroma-key transparency to an RGBA image
/// in place. See the module-level docs for the algorithm.
pub fn apply_transparency_to_rgba(img: &mut RgbaImage, background: [u8; 3], tolerance: f64) {
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
    let tol_sq = tolerance.max(0.0).powi(2);

    // Pass 1: candidacy mask. A pixel is a flood candidate if either
    // its RGB is within `tolerance` of `bg`, or it is already fully
    // transparent (so previously-keyed regions stay keyed).
    let mut is_candidate = vec![false; n];
    for y in 0..h {
        for x in 0..w {
            let i = (y as usize) * (w as usize) + (x as usize);
            let p = img.get_pixel(x, y);
            if p[3] == 0 {
                is_candidate[i] = true;
                continue;
            }
            let dr = p[0] as f64 - bg[0];
            let dg = p[1] as f64 - bg[1];
            let db = p[2] as f64 - bg[2];
            if dr * dr + dg * dg + db * db <= tol_sq {
                is_candidate[i] = true;
            }
        }
    }

    // Pass 2: flood-fill from the four corners through candidates.
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

    // Pass 3: clear flooded pixels. Leave every other pixel alone.
    for y in 0..h {
        for x in 0..w {
            let i = (y as usize) * (w as usize) + (x as usize);
            if flooded[i] {
                img.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
            }
        }
    }
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
        assert_eq!(res.transparent_pixels as u32, out.width() * out.height());
        for p in out.pixels() {
            assert_eq!(p[3], 0);
        }
    }

    #[test]
    fn foreground_pixel_stays_opaque_and_unchanged() {
        // Magenta background with one solid black pixel in the middle.
        let mut img = solid(20, 20, [255, 0, 255]);
        img.put_pixel(10, 10, Rgb([0, 0, 0]));
        let (out, _) = apply_transparency(
            &DynamicImage::ImageRgb8(img),
            &TransparentOptions::default(),
        )
        .unwrap();
        let px = out.get_pixel(10, 10);
        // Colour must be byte-for-byte the same — no decontamination.
        assert_eq!((px[0], px[1], px[2], px[3]), (0, 0, 0, 255));
    }

    #[test]
    fn user_supplied_background_overrides_auto_detect() {
        // Image's corners are black — auto-detect would pick black —
        // but we explicitly ask for magenta.
        let img = solid(30, 30, [0, 0, 0]);
        let opts = TransparentOptions {
            background: Some([255, 0, 255]),
            ..Default::default()
        };
        let (out, res) = apply_transparency(&DynamicImage::ImageRgb8(img), &opts).unwrap();
        assert_eq!(res.background, [255, 0, 255]);
        // Black is far from magenta → nothing flooded → all opaque,
        // colour unchanged.
        for p in out.pixels() {
            assert_eq!(p[3], 255);
            assert_eq!((p[0], p[1], p[2]), (0, 0, 0));
        }
    }

    #[test]
    fn near_bg_pixel_surrounded_by_foreground_stays_opaque() {
        // Solid black square on a magenta background, with one near-
        // magenta pixel placed deep in the middle of the black square.
        // It is not reachable from the corners, so flood-fill matting
        // must preserve it.
        let mut img = solid(20, 20, [255, 0, 255]);
        for y in 5..15 {
            for x in 5..15 {
                img.put_pixel(x, y, Rgb([0, 0, 0]));
            }
        }
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
        // A near-magenta band along the top row touches the corners,
        // so flood-fill reaches it and removes it.
        let mut img = solid(20, 20, [255, 0, 255]);
        for x in 0..20 {
            img.put_pixel(x, 0, Rgb([240, 20, 240]));
        }
        let (out, _) = apply_transparency(
            &DynamicImage::ImageRgb8(img),
            &TransparentOptions::default(),
        )
        .unwrap();
        for x in 0..20 {
            assert_eq!(
                out.get_pixel(x, 0)[3],
                0,
                "top-row x={x} should have been flooded"
            );
        }
    }

    #[test]
    fn pre_existing_transparent_pixels_stay_transparent() {
        // Source already has alpha=0 pixels in the middle. They must
        // survive as transparent regardless of their RGB.
        let mut rgba = RgbaImage::from_pixel(10, 10, Rgba([255, 0, 255, 255]));
        rgba.put_pixel(5, 5, Rgba([123, 45, 67, 0]));
        let (out, _) = apply_transparency(
            &DynamicImage::ImageRgba8(rgba),
            &TransparentOptions::default(),
        )
        .unwrap();
        assert_eq!(out.get_pixel(5, 5)[3], 0);
    }
}
