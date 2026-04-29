//! Detect and crop individual objects from a sheet image.
//!
//! Designed for "expression sheets" or sprite sheets where multiple
//! objects sit on a single-color background, optionally separated by
//! small gaps and accompanied by text labels.
//!
//! The detection pipeline is:
//!   1. Estimate background color from corner patches.
//!   2. Build a foreground mask using RGB distance + 1-px erosion.
//!   3. 2D connected-component labeling on the mask. Each fox / sprite
//!      ends up as a single component because its parts are physically
//!      connected; near-touching neighbours stay separate as long as
//!      even a 1-pixel background gap exists between them.
//!   4. Group small components into the nearest main component when
//!      their x-range falls inside it AND their y-range overlaps it
//!      (handles ears or accessories that erosion split off). Small
//!      components with no y-overlap (text labels below) are dropped.
//!   5. For each component, row projection within its x-range finds
//!      the largest vertical run, trimming any text that happens to
//!      be inside the same connected component.
//!   6. If an expected count is given, fall back to column-min
//!      re-splitting on the widest blob — only triggers when CCs are
//!      truly merged by a 1-pixel bridge.

use image::{DynamicImage, GenericImageView, Rgb, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SplitError {
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(
        "Detected {detected} objects but expected {expected}. \
         Could not reliably reconcile."
    )]
    CountMismatch { detected: usize, expected: usize },
    #[error("No objects detected on the background")]
    NoObjects,
}

#[derive(Debug, Clone, Default)]
pub struct SplitOptions {
    /// Pixels of background padding to add around each detected object,
    /// in addition to the automatic erosion-compensation margin.
    pub padding: u32,
    /// If `Some(n)`, the algorithm will try to reconcile its detection
    /// to exactly `n` objects (by re-splitting the widest blob).
    pub expected_count: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DetectedObject {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitResult {
    /// RGB tuple of the auto-detected background color.
    pub background: [u8; 3],
    pub objects: Vec<DetectedObject>,
    /// `true` if the reconcile pass had to re-split blobs to match the
    /// expected count.
    pub resplit_used: bool,
}

const FG_DISTANCE_THRESHOLD: f64 = 12.0;
const CORNER_PATCH: u32 = 20;
/// Pixels added to each bbox edge to compensate for the 1-px erosion
/// applied during mask building.
const EROSION_COMPENSATE: u32 = 3;
/// Visual breathing margin added to each bbox, as a fraction of the
/// bbox's smaller dimension. Makes crops feel less tight without
/// looking like they have a huge frame.
const VISUAL_MARGIN_RATIO: f64 = 0.04;

/// Detect objects in `img` according to `opts`.
pub fn detect_objects(img: &DynamicImage, opts: &SplitOptions) -> Result<SplitResult, SplitError> {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return Err(SplitError::NoObjects);
    }

    let rgb = img.to_rgb8();
    let bg = estimate_background(&rgb);
    let mask = build_mask(&rgb, bg);
    let mask = erode_3x3(&mask, w, h);

    let mut blobs = find_blobs_cc(&mask, w, h);

    if blobs.is_empty() {
        return Err(SplitError::NoObjects);
    }

    let mut resplit_used = false;
    if let Some(expected) = opts.expected_count {
        if blobs.len() < expected {
            let original = blobs.len();
            let col_count = column_projection(&mask, w, h);
            blobs =
                resplit_to_match(blobs, &col_count, expected).ok_or(SplitError::CountMismatch {
                    detected: original,
                    expected,
                })?;
            resplit_used = true;
        } else if blobs.len() > expected {
            blobs = merge_to_match(blobs, expected);
        }
        if blobs.len() != expected {
            return Err(SplitError::CountMismatch {
                detected: blobs.len(),
                expected,
            });
        }
    }

    // Compute per-blob y-extent via row projection (largest vertical run),
    // then expand each bbox by:
    //   - EROSION_COMPENSATE (constant, fixes mask shrinkage)
    //   - VISUAL_MARGIN_RATIO * min(bw, bh) (proportional breathing room)
    //   - opts.padding (user-requested extra)
    let mut objects = Vec::with_capacity(blobs.len());
    for (idx, (x0, x1)) in blobs.iter().enumerate() {
        if let Some((y0, y1)) = row_extent(&mask, w, h, *x0, *x1) {
            let raw_w = x1 - x0 + 1;
            let raw_h = y1 - y0 + 1;
            let visual = ((raw_w.min(raw_h) as f64) * VISUAL_MARGIN_RATIO).round() as u32;
            let margin = EROSION_COMPENSATE + visual + opts.padding;
            // Clip the cosmetic margin against neighbouring blob edges
            // so the crop never reaches into another sprite's bbox.
            let left_limit = if idx == 0 { 0 } else { blobs[idx - 1].1 + 1 };
            let right_limit = if idx + 1 == blobs.len() {
                w - 1
            } else {
                blobs[idx + 1].0.saturating_sub(1)
            };
            let bx = x0.saturating_sub(margin).max(left_limit);
            let by = y0.saturating_sub(margin);
            let bw = (*x1 + 1 + margin).min(right_limit + 1) - bx;
            let bh = (y1 + 1 + margin).min(h) - by;
            objects.push(DetectedObject {
                x: bx,
                y: by,
                w: bw,
                h: bh,
            });
        }
    }

    if objects.is_empty() {
        return Err(SplitError::NoObjects);
    }

    Ok(SplitResult {
        background: [bg[0], bg[1], bg[2]],
        objects,
        resplit_used,
    })
}

/// Crop a single detected object out of the source image.
pub fn crop(img: &DynamicImage, obj: &DetectedObject) -> DynamicImage {
    img.crop_imm(obj.x, obj.y, obj.w, obj.h)
}

/// Crop `obj` from `img` and place it on a fresh `target_w × target_h`
/// canvas filled with `background`, with the cropped content centered.
///
/// This is the safe way to produce uniform-sized outputs from a sheet
/// where neighbors are close together — unlike expanding the bbox into
/// the source image (which would include neighbor pixels), this only
/// reads from inside the original bbox and pads with the background
/// color.
pub fn crop_padded(
    img: &DynamicImage,
    obj: &DetectedObject,
    target_w: u32,
    target_h: u32,
    background: [u8; 3],
) -> DynamicImage {
    use image::{Rgba, RgbaImage};

    let cropped = img.crop_imm(obj.x, obj.y, obj.w, obj.h).to_rgba8();
    let tw = target_w.max(obj.w);
    let th = target_h.max(obj.h);
    let bg = Rgba([background[0], background[1], background[2], 255]);
    let mut canvas = RgbaImage::from_pixel(tw, th, bg);
    let off_x = (tw - obj.w) / 2;
    let off_y = (th - obj.h) / 2;
    image::imageops::overlay(&mut canvas, &cropped, off_x as i64, off_y as i64);
    DynamicImage::ImageRgba8(canvas)
}

/// Compute the maximum width and height across a slice of detected objects.
pub fn max_dimensions(objects: &[DetectedObject]) -> (u32, u32) {
    let max_w = objects.iter().map(|o| o.w).max().unwrap_or(0);
    let max_h = objects.iter().map(|o| o.h).max().unwrap_or(0);
    (max_w, max_h)
}

// ---------- background / mask ----------

fn estimate_background(img: &image::RgbImage) -> Rgb<u8> {
    let (w, h) = (img.width(), img.height());
    let patch = CORNER_PATCH.min(w / 4).min(h / 4).max(1);

    let mut samples_r = Vec::new();
    let mut samples_g = Vec::new();
    let mut samples_b = Vec::new();
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
                samples_r.push(p[0]);
                samples_g.push(p[1]);
                samples_b.push(p[2]);
            }
        }
    }
    Rgb([
        median(&mut samples_r),
        median(&mut samples_g),
        median(&mut samples_b),
    ])
}

fn median(v: &mut [u8]) -> u8 {
    v.sort_unstable();
    v[v.len() / 2]
}

fn build_mask(img: &image::RgbImage, bg: Rgb<u8>) -> Vec<bool> {
    let (w, h) = (img.width(), img.height());
    let mut out = vec![false; (w * h) as usize];
    let bg = (bg[0] as f64, bg[1] as f64, bg[2] as f64);
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x, y);
            let dr = p[0] as f64 - bg.0;
            let dg = p[1] as f64 - bg.1;
            let db = p[2] as f64 - bg.2;
            let d = (dr * dr + dg * dg + db * db).sqrt();
            if d > FG_DISTANCE_THRESHOLD {
                out[(y * w + x) as usize] = true;
            }
        }
    }
    out
}

/// 3x3 minimum filter (erosion). A pixel stays foreground only if all
/// 8 neighbors are also foreground.
fn erode_3x3(mask: &[bool], w: u32, h: u32) -> Vec<bool> {
    let mut out = vec![false; mask.len()];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = (y * w + x) as usize;
            if !mask[i] {
                continue;
            }
            let mut all = true;
            'outer: for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let nx = (x as i32 + dx) as u32;
                    let ny = (y as i32 + dy) as u32;
                    if !mask[(ny * w + nx) as usize] {
                        all = false;
                        break 'outer;
                    }
                }
            }
            out[i] = all;
        }
    }
    out
}

// ---------- projections / blobs ----------

fn column_projection(mask: &[bool], w: u32, h: u32) -> Vec<u32> {
    let mut out = vec![0u32; w as usize];
    for y in 0..h {
        for x in 0..w {
            if mask[(y * w + x) as usize] {
                out[x as usize] += 1;
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy)]
struct Component {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    area: u32,
}

/// 2D connected-component labeling via iterative DFS, 8-connected.
/// Each foreground pixel ends up in exactly one component.
fn connected_components(mask: &[bool], w: u32, h: u32) -> Vec<Component> {
    let mut visited = vec![false; mask.len()];
    let mut stack: Vec<(u32, u32)> = Vec::new();
    let mut comps = Vec::new();
    for sy in 0..h {
        for sx in 0..w {
            let si = (sy * w + sx) as usize;
            if !mask[si] || visited[si] {
                continue;
            }
            visited[si] = true;
            stack.clear();
            stack.push((sx, sy));
            let mut x0 = sx;
            let mut x1 = sx;
            let mut y0 = sy;
            let mut y1 = sy;
            let mut area = 0u32;
            while let Some((cx, cy)) = stack.pop() {
                area += 1;
                if cx < x0 {
                    x0 = cx;
                }
                if cx > x1 {
                    x1 = cx;
                }
                if cy < y0 {
                    y0 = cy;
                }
                if cy > y1 {
                    y1 = cy;
                }
                let xmin = cx.saturating_sub(1);
                let xmax = (cx + 1).min(w - 1);
                let ymin = cy.saturating_sub(1);
                let ymax = (cy + 1).min(h - 1);
                for ny in ymin..=ymax {
                    for nx in xmin..=xmax {
                        let ni = (ny * w + nx) as usize;
                        if mask[ni] && !visited[ni] {
                            visited[ni] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
            }
            comps.push(Component {
                x0,
                y0,
                x1,
                y1,
                area,
            });
        }
    }
    comps
}

fn uf_find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Group connected components into per-object x-ranges.
///
/// Two components are unified when their x-ranges overlap by at least
/// 50% of the narrower component AND their y-ranges overlap. This
/// rejoins a sprite that erosion fragmented (e.g. fox head + hoodie
/// split at a thin neck), while keeping a text label underneath
/// separate (no y-overlap with the sprite above it). After grouping,
/// small leftover groups are dropped as labels / noise.
fn find_blobs_cc(mask: &[bool], w: u32, h: u32) -> Vec<(u32, u32)> {
    let raw = connected_components(mask, w, h);
    if raw.is_empty() {
        return Vec::new();
    }

    // Drop dust / single-pixel JPEG noise: components below 0.01% of
    // image area are never sprites.
    let noise_threshold = (((w as u64) * (h as u64)) as f64 * 0.0001).max(8.0) as u32;
    let comps: Vec<Component> = raw
        .into_iter()
        .filter(|c| c.area > noise_threshold)
        .collect();
    if comps.is_empty() {
        return Vec::new();
    }

    let n = comps.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for i in 0..n {
        for j in (i + 1)..n {
            let a = &comps[i];
            let b = &comps[j];
            // x-overlap relative to narrower component
            let xo_start = a.x0.max(b.x0);
            let xo_end = a.x1.min(b.x1);
            if xo_end < xo_start {
                continue;
            }
            let xo = xo_end - xo_start + 1;
            let smaller_x = (a.x1 - a.x0 + 1).min(b.x1 - b.x0 + 1);
            if (xo as f64) < (smaller_x as f64) * 0.5 {
                continue;
            }
            // y-overlap must be > 0 to fuse — keeps text labels apart.
            if a.y1 < b.y0 || b.y1 < a.y0 {
                continue;
            }
            let ra = uf_find(&mut parent, i);
            let rb = uf_find(&mut parent, j);
            if ra != rb {
                parent[ra] = rb;
            }
        }
    }

    // Collapse each union-find class into a merged Component.
    let mut groups: std::collections::HashMap<usize, Component> = std::collections::HashMap::new();
    for (i, &c) in comps.iter().enumerate() {
        let r = uf_find(&mut parent, i);
        groups
            .entry(r)
            .and_modify(|m| {
                m.x0 = m.x0.min(c.x0);
                m.x1 = m.x1.max(c.x1);
                m.y0 = m.y0.min(c.y0);
                m.y1 = m.y1.max(c.y1);
                m.area = m.area.saturating_add(c.area);
            })
            .or_insert(c);
    }
    let mut grouped: Vec<Component> = groups.into_values().collect();

    // Drop a group if it's both (a) noticeably smaller than the
    // largest group AND (b) doesn't y-overlap any larger group.
    // Captures text labels printed below the row of sprites without
    // dropping legitimately smaller sprites that line up with the
    // others.
    let max_area = grouped.iter().map(|c| c.area).max().unwrap_or(0);
    let small_cutoff = ((max_area as f64) * 0.30) as u32;
    let snapshot = grouped.clone();
    grouped.retain(|c| {
        if c.area >= small_cutoff {
            return true;
        }
        // Only large neighbours count. This stops a row of text glyphs
        // from vouching for each other.
        snapshot
            .iter()
            .any(|o| o.area >= small_cutoff && !(c.y1 < o.y0 || c.y0 > o.y1))
    });

    grouped.sort_by_key(|c| c.x0);

    // Re-attribute spillover columns: when two adjacent sprites'
    // anti-aliased outlines bleed into each other's connected
    // component, the spillover lives near the boundary as a sparse
    // tail. For each adjacent pair, find the column-density valley in
    // the overlap zone and clip the left group's right edge / the
    // right group's left edge to it. This pulls back any low-density
    // tail that crossed the natural gap between the sprites.
    let col_count = column_projection(mask, w, h);
    for i in 0..grouped.len().saturating_sub(1) {
        let a = grouped[i];
        let b = grouped[i + 1];
        if a.x1 < b.x0 {
            continue; // groups already disjoint
        }
        let lo = b.x0;
        let hi = a.x1;
        let (mut min_x, mut min_v) = (lo, col_count[lo as usize]);
        for x in lo..=hi {
            let v = col_count[x as usize];
            if v < min_v {
                min_v = v;
                min_x = x;
            }
        }
        grouped[i].x1 = min_x.saturating_sub(1).max(a.x0);
        grouped[i + 1].x0 = (min_x + 1).min(b.x1);
    }

    grouped.into_iter().map(|c| (c.x0, c.x1)).collect()
}

/// For columns x in [x0, x1], find the largest connected vertical run
/// of "non-empty" rows. A row is "non-empty" if it has at least one
/// foreground pixel within [x0, x1].
fn row_extent(mask: &[bool], w: u32, h: u32, x0: u32, x1: u32) -> Option<(u32, u32)> {
    let row_has = |y: u32| -> bool {
        for x in x0..=x1 {
            if mask[(y * w + x) as usize] {
                return true;
            }
        }
        false
    };

    let mut best: Option<(u32, u32)> = None;
    let mut best_len = 0u32;
    let mut cur_start: Option<u32> = None;

    for y in 0..h {
        if row_has(y) {
            cur_start.get_or_insert(y);
        } else if let Some(s) = cur_start.take() {
            let len = y - s;
            if len > best_len {
                best_len = len;
                best = Some((s, y - 1));
            }
        }
    }
    if let Some(s) = cur_start {
        let len = h - s;
        if len > best_len {
            best = Some((s, h - 1));
        }
    }
    best
}

// ---------- reconciliation ----------

fn resplit_to_match(
    mut blobs: Vec<(u32, u32)>,
    col_count: &[u32],
    expected: usize,
) -> Option<Vec<(u32, u32)>> {
    while blobs.len() < expected {
        // Pick the widest blob.
        let (idx, _) = blobs.iter().enumerate().max_by_key(|(_, (s, e))| e - s)?;
        let (s, e) = blobs[idx];
        let width = e - s + 1;
        if width < 4 {
            return None;
        }

        // Find the column with minimum count in the middle 60% of the blob.
        let inner_start = s + width / 5;
        let inner_end = e.saturating_sub(width / 5);
        if inner_start >= inner_end {
            return None;
        }
        let mut min_idx = inner_start;
        let mut min_val = col_count[inner_start as usize];
        for x in inner_start..=inner_end {
            if col_count[x as usize] < min_val {
                min_val = col_count[x as usize];
                min_idx = x;
            }
        }

        // Split: [s..min_idx-1] and [min_idx+1..e]
        if min_idx == s || min_idx == e {
            return None;
        }
        blobs.remove(idx);
        blobs.insert(idx, (min_idx + 1, e));
        blobs.insert(idx, (s, min_idx.saturating_sub(1)));
    }
    Some(blobs)
}

fn merge_to_match(mut blobs: Vec<(u32, u32)>, expected: usize) -> Vec<(u32, u32)> {
    while blobs.len() > expected {
        // Find the smallest blob and merge it into its closer neighbor.
        let (idx, _) = blobs
            .iter()
            .enumerate()
            .min_by_key(|(_, (s, e))| e - s)
            .unwrap();
        let (s, e) = blobs[idx];
        if blobs.len() == 1 {
            break;
        }
        let merge_left = if idx == 0 {
            false
        } else if idx == blobs.len() - 1 {
            true
        } else {
            let left_dist = s - blobs[idx - 1].1;
            let right_dist = blobs[idx + 1].0 - e;
            left_dist < right_dist
        };
        if merge_left {
            let (ls, _) = blobs[idx - 1];
            blobs[idx - 1] = (ls, e);
            blobs.remove(idx);
        } else {
            let (_, re) = blobs[idx + 1];
            blobs[idx + 1] = (s, re);
            blobs.remove(idx);
        }
    }
    blobs
}

// ---------- preview ----------

const PREVIEW_COLORS: &[[u8; 3]] = &[
    [255, 107, 107],
    [78, 205, 196],
    [255, 217, 61],
    [156, 107, 255],
    [107, 203, 119],
];

/// What `write_preview` draws on top of the source image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewStyle {
    /// Tight per-object bounding boxes (what the algorithm detected).
    Detected,
    /// The uniform max_w × max_h frame each saved PNG corresponds to,
    /// centered on each object.
    Output,
    /// Both — detected as a thin solid line, output as a thicker line.
    Both,
}

/// Draw colored rectangles around each detected object on a copy of
/// `img` and save to `path`.
pub fn write_preview(
    img: &DynamicImage,
    result: &SplitResult,
    style: PreviewStyle,
    path: &Path,
) -> Result<(), SplitError> {
    let (iw, ih) = img.dimensions();
    let mut canvas: RgbaImage = img.to_rgba8();
    let base_stroke = ((iw.min(ih) as f32) * 0.005).max(2.0) as u32;
    let (max_w, max_h) = max_dimensions(&result.objects);

    for (i, obj) in result.objects.iter().enumerate() {
        let c = PREVIEW_COLORS[i % PREVIEW_COLORS.len()];
        match style {
            PreviewStyle::Detected => {
                draw_rect(&mut canvas, obj, base_stroke, c);
            }
            PreviewStyle::Output => {
                let frame = output_frame(obj, max_w, max_h, iw, ih);
                draw_rect(&mut canvas, &frame, base_stroke, c);
            }
            PreviewStyle::Both => {
                // output: solid line at base stroke
                let frame = output_frame(obj, max_w, max_h, iw, ih);
                draw_rect(&mut canvas, &frame, base_stroke, c);
                // detected: dashed line, same stroke width
                draw_dashed_rect(&mut canvas, obj, base_stroke, c);
            }
        }
    }
    canvas.save(path)?;
    Ok(())
}

/// Compute the output-frame rectangle for one detected object: a
/// `max_w × max_h` box centered on the object, clipped to the image.
fn output_frame(
    obj: &DetectedObject,
    max_w: u32,
    max_h: u32,
    image_w: u32,
    image_h: u32,
) -> DetectedObject {
    let cx = obj.x + obj.w / 2;
    let cy = obj.y + obj.h / 2;
    let half_w = max_w / 2;
    let half_h = max_h / 2;
    let x = cx.saturating_sub(half_w);
    let y = cy.saturating_sub(half_h);
    let w = (x + max_w).min(image_w) - x;
    let h = (y + max_h).min(image_h) - y;
    DetectedObject { x, y, w, h }
}

/// Draw a dashed rectangle outline. Dash pattern is `DASH_ON` filled
/// pixels then `DASH_OFF` skipped pixels, repeating along each side.
fn draw_dashed_rect(canvas: &mut RgbaImage, obj: &DetectedObject, stroke: u32, color: [u8; 3]) {
    const DASH_ON: u32 = 12;
    const DASH_OFF: u32 = 8;
    let (w, h) = (canvas.width(), canvas.height());
    let x0 = obj.x;
    let y0 = obj.y;
    let x1 = (obj.x + obj.w).min(w).saturating_sub(1);
    let y1 = (obj.y + obj.h).min(h).saturating_sub(1);
    let rgba = Rgba([color[0], color[1], color[2], 255]);
    let dash_on = |i: u32| (i % (DASH_ON + DASH_OFF)) < DASH_ON;

    for t in 0..stroke {
        for x in x0..=x1 {
            if !dash_on(x - x0) {
                continue;
            }
            if y0 + t < h {
                canvas.put_pixel(x, y0 + t, rgba);
            }
            if y1 >= t {
                canvas.put_pixel(x, y1 - t, rgba);
            }
        }
        for y in y0..=y1 {
            if !dash_on(y - y0) {
                continue;
            }
            if x0 + t < w {
                canvas.put_pixel(x0 + t, y, rgba);
            }
            if x1 >= t {
                canvas.put_pixel(x1 - t, y, rgba);
            }
        }
    }
}

fn draw_rect(canvas: &mut RgbaImage, obj: &DetectedObject, stroke: u32, color: [u8; 3]) {
    let (w, h) = (canvas.width(), canvas.height());
    let x0 = obj.x;
    let y0 = obj.y;
    let x1 = (obj.x + obj.w).min(w).saturating_sub(1);
    let y1 = (obj.y + obj.h).min(h).saturating_sub(1);
    let rgba = Rgba([color[0], color[1], color[2], 255]);
    for t in 0..stroke {
        // top + bottom
        for x in x0..=x1 {
            if y0 + t < h {
                canvas.put_pixel(x, y0 + t, rgba);
            }
            if y1 >= t {
                canvas.put_pixel(x, y1 - t, rgba);
            }
        }
        // left + right
        for y in y0..=y1 {
            if x0 + t < w {
                canvas.put_pixel(x0 + t, y, rgba);
            }
            if x1 >= t {
                canvas.put_pixel(x1 - t, y, rgba);
            }
        }
    }
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn solid_bg(w: u32, h: u32) -> RgbImage {
        RgbImage::from_pixel(w, h, Rgb([246, 239, 221]))
    }

    fn fill_rect(img: &mut RgbImage, x: u32, y: u32, w: u32, h: u32, c: [u8; 3]) {
        for yy in y..y + h {
            for xx in x..x + w {
                img.put_pixel(xx, yy, Rgb(c));
            }
        }
    }

    #[test]
    fn estimate_background_uses_corners() {
        let img = solid_bg(100, 100);
        let bg = estimate_background(&img);
        assert_eq!(bg, Rgb([246, 239, 221]));
    }

    #[test]
    fn detect_two_clear_blobs() {
        let mut img = solid_bg(200, 100);
        fill_rect(&mut img, 20, 20, 40, 60, [50, 50, 50]);
        fill_rect(&mut img, 120, 20, 40, 60, [50, 50, 50]);
        let dy = DynamicImage::ImageRgb8(img);
        let res = detect_objects(&dy, &SplitOptions::default()).unwrap();
        assert_eq!(res.objects.len(), 2);
    }

    #[test]
    fn excludes_text_label_below_object() {
        // Object at top, separate "label" strip below with a gap.
        let mut img = solid_bg(100, 200);
        fill_rect(&mut img, 20, 20, 60, 80, [50, 50, 50]); // object
        fill_rect(&mut img, 30, 150, 40, 10, [80, 80, 80]); // label
        let dy = DynamicImage::ImageRgb8(img);
        let res = detect_objects(&dy, &SplitOptions::default()).unwrap();
        assert_eq!(res.objects.len(), 1);
        let o = &res.objects[0];
        // Should be the object, not stretched down to the label.
        assert!(o.h < 100, "height {} should exclude label", o.h);
    }

    #[test]
    fn resplit_a_solid_block_into_two() {
        // A single wide solid block; expected=2 forces re-split
        // at the minimum-count column (middle, since uniform).
        let mut img = solid_bg(200, 100);
        fill_rect(&mut img, 20, 20, 160, 60, [50, 50, 50]);
        let dy = DynamicImage::ImageRgb8(img);
        let opts = SplitOptions {
            expected_count: Some(2),
            ..Default::default()
        };
        let res = detect_objects(&dy, &opts).unwrap();
        assert_eq!(res.objects.len(), 2);
        assert!(res.resplit_used);
    }
}
