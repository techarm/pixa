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
//! 3. Set the alpha of every flooded pixel to 0. RGB is preserved so
//!    downstream resampling does not bleed a black halo across the
//!    transparency boundary.
//! 4. Optional `despill`: for pixels within `despill_band` of the
//!    flooded region, subtract the bg-aligned colour component so AA
//!    contamination (pink fringes on a magenta key etc.) is
//!    neutralised without touching alpha or interior pixels.
//! 5. Optional `shrink`: morphologically erode the opaque region by
//!    `shrink` pixels. Removes the outermost — and typically most
//!    contaminated — ring at the cost of a tiny silhouette shrinkage.
//!
//! Core contract: "the background becomes transparent; the rest does
//! not change." `despill` and `shrink` are opt-in refinements for
//! images whose AI-generated edges carry leftover bg tint.
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
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
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
    /// Enable channel-based spill suppression on the edge band.
    /// Neutralises bg-colour contamination on the outermost opaque
    /// pixels (e.g. the pink fringes left by AA on a magenta key)
    /// without changing alpha or interior pixels.
    pub despill: bool,
    /// Radius of the edge band (in pixels) where spill suppression is
    /// applied, measured outward from the flooded region. Ignored
    /// when `despill` is false. Default: 3.
    pub despill_band: u32,
    /// Morphologically erode the opaque region by this many pixels
    /// after flood (and after despill). Removes the outermost AA ring
    /// entirely; useful when contamination is heavy. Default: 0.
    pub shrink: u32,
}

impl Default for TransparentOptions {
    fn default() -> Self {
        Self {
            background: None,
            tolerance: 200.0,
            despill: false,
            despill_band: 3,
            shrink: 0,
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
    apply_transparency_to_rgba(
        &mut out,
        bg,
        opts.tolerance,
        opts.despill,
        opts.despill_band,
        opts.shrink,
    );

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
pub fn apply_transparency_to_rgba(
    img: &mut RgbaImage,
    background: [u8; 3],
    tolerance: f64,
    despill: bool,
    despill_band: u32,
    shrink: u32,
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
    let mut flooded = flood_from_corners(&is_candidate, w, h);

    // Pass 3 (optional): despill — channel-based bg-spill suppression
    // on the edge band. Applied BEFORE shrink so we operate on the
    // original contaminated pixels before any are eroded away.
    if despill && despill_band > 0 {
        apply_despill(img, &flooded, bg, despill_band, w, h);
    }

    // Pass 4 (optional): shrink — morphologically erode the opaque
    // region by growing the flooded region outward by `shrink` pixels.
    if shrink > 0 {
        grow_flood(&mut flooded, shrink, w, h);
    }

    // Pass 5: clear alpha on every flooded pixel; everything else is
    // untouched. RGB of flooded pixels is preserved so that downstream
    // resampling (e.g. `pixa compress --max`) does not bleed a black
    // halo from (0,0,0) transparent pixels into the opaque edges —
    // a common pitfall when alpha-ignorant filters blend RGB across
    // the transparency boundary.
    for y in 0..h {
        for x in 0..w {
            let i = (y as usize) * (w as usize) + (x as usize);
            if flooded[i] {
                let mut p = *img.get_pixel(x, y);
                p[3] = 0;
                img.put_pixel(x, y, p);
            }
        }
    }
}

/// 4-connected flood fill from the four image corners, propagating
/// only through `is_candidate` pixels.
fn flood_from_corners(is_candidate: &[bool], w: u32, h: u32) -> Vec<bool> {
    let n = (w as usize) * (h as usize);
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
    flooded
}

/// Grow the flooded region outward by `radius` 4-connected steps,
/// regardless of pixel colour. Used for `--shrink`.
fn grow_flood(flooded: &mut [bool], radius: u32, w: u32, h: u32) {
    // BFS with distance tracking; stop once dist exceeds radius.
    let n = (w as usize) * (h as usize);
    let mut dist = vec![u32::MAX; n];
    let mut queue: VecDeque<(u32, u32)> = VecDeque::new();
    for y in 0..h {
        for x in 0..w {
            let i = (y as usize) * (w as usize) + (x as usize);
            if flooded[i] {
                dist[i] = 0;
                queue.push_back((x, y));
            }
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        let i = (y as usize) * (w as usize) + (x as usize);
        let d_here = dist[i];
        if d_here >= radius {
            continue;
        }
        let nbrs: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
        for (dx, dy) in nbrs {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                continue;
            }
            let ni = (ny as usize) * (w as usize) + (nx as usize);
            if dist[ni] > d_here + 1 {
                dist[ni] = d_here + 1;
                flooded[ni] = true;
                queue.push_back((nx as u32, ny as u32));
            }
        }
    }
}

/// Channel-based bg-spill suppression for pixels within `band`
/// 4-connected steps of the flooded region. For a bg colour like
/// magenta (R and B high, G low), each edge-band pixel has its
/// high-bg-aligned channels reduced by the excess they carry over
/// the low-bg-aligned channels — cancelling AI-generated AA
/// contamination without touching alpha or interior pixels.
fn apply_despill(img: &mut RgbaImage, flooded: &[bool], bg: [f64; 3], band: u32, w: u32, h: u32) {
    // Classify each bg channel as "high" (> 128) or "low" (<= 128).
    let mut high_ch: Vec<usize> = Vec::new();
    let mut low_ch: Vec<usize> = Vec::new();
    for (c, &v) in bg.iter().enumerate() {
        if v > 128.0 {
            high_ch.push(c);
        } else {
            low_ch.push(c);
        }
    }
    if high_ch.is_empty() || low_ch.is_empty() {
        // Monochrome bg (pure black/white/grey) has no directional
        // spill signature — nothing to suppress.
        return;
    }

    let n = (w as usize) * (h as usize);
    // BFS outward from the flooded boundary to find edge-band pixels.
    let mut dist = vec![u32::MAX; n];
    let mut queue: VecDeque<(u32, u32)> = VecDeque::new();
    for y in 0..h {
        for x in 0..w {
            let i = (y as usize) * (w as usize) + (x as usize);
            if flooded[i] {
                dist[i] = 0;
                queue.push_back((x, y));
            }
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        let i = (y as usize) * (w as usize) + (x as usize);
        let d_here = dist[i];
        if d_here >= band {
            continue;
        }
        let nbrs: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
        for (dx, dy) in nbrs {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                continue;
            }
            let ni = (ny as usize) * (w as usize) + (nx as usize);
            if flooded[ni] {
                continue;
            }
            if dist[ni] > d_here + 1 {
                dist[ni] = d_here + 1;
                queue.push_back((nx as u32, ny as u32));
            }
        }
    }

    // Apply channel suppression on every pixel inside the edge band.
    for y in 0..h {
        for x in 0..w {
            let i = (y as usize) * (w as usize) + (x as usize);
            if flooded[i] || dist[i] == u32::MAX {
                continue;
            }
            let p = img.get_pixel(x, y);
            if p[3] == 0 {
                continue;
            }
            let high_avg = high_ch.iter().map(|&c| p[c] as f64).sum::<f64>() / high_ch.len() as f64;
            let low_avg = low_ch.iter().map(|&c| p[c] as f64).sum::<f64>() / low_ch.len() as f64;
            let spill = (high_avg - low_avg).max(0.0);
            if spill <= 0.0 {
                continue;
            }
            let mut new_rgba = [p[0], p[1], p[2], p[3]];
            for &c in &high_ch {
                new_rgba[c] = (p[c] as f64 - spill).clamp(0.0, 255.0).round() as u8;
            }
            img.put_pixel(x, y, image::Rgba(new_rgba));
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
