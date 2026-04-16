//! Detect and crop individual objects from a sheet image.
//!
//! Designed for "expression sheets" or sprite sheets where multiple
//! objects sit on a single-color background, optionally separated by
//! small gaps and accompanied by text labels.
//!
//! The detection pipeline is:
//!   1. Estimate background color from corner patches.
//!   2. Build a foreground mask using RGB distance + 1-px erosion.
//!   3. Column projection to split horizontally into blobs.
//!   4. For each blob, row projection to find the largest vertical run
//!      (this excludes text labels printed below the object).
//!   5. If an expected count is given, re-split the widest blob at its
//!      column-projection minimum until the count matches.

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

const FG_DISTANCE_THRESHOLD: f64 = 30.0;
const GAP_FG_RATIO: f64 = 0.01;
const MIN_GAP_RATIO: f64 = 0.01;
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

    let col_count = column_projection(&mask, w, h);
    let mut blobs = find_blobs(&col_count, w, h);

    if blobs.is_empty() {
        return Err(SplitError::NoObjects);
    }

    let mut resplit_used = false;
    if let Some(expected) = opts.expected_count {
        if blobs.len() < expected {
            let original = blobs.len();
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
    for (x0, x1) in &blobs {
        if let Some((y0, y1)) = row_extent(&mask, w, h, *x0, *x1) {
            let raw_w = x1 - x0 + 1;
            let raw_h = y1 - y0 + 1;
            let visual = ((raw_w.min(raw_h) as f64) * VISUAL_MARGIN_RATIO).round() as u32;
            let margin = EROSION_COMPENSATE + visual + opts.padding;
            let bx = x0.saturating_sub(margin);
            let by = y0.saturating_sub(margin);
            let bw = (*x1 + 1 + margin).min(w) - bx;
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

/// Group columns into (x_start, x_end) intervals using gap detection.
fn find_blobs(col_count: &[u32], w: u32, h: u32) -> Vec<(u32, u32)> {
    let gap_threshold = (h as f64 * GAP_FG_RATIO).max(1.0) as u32;
    let min_gap_width = ((w as f64 * MIN_GAP_RATIO) as u32).max(2);

    // First pass: mark each column as gap or not.
    let is_gap: Vec<bool> = col_count.iter().map(|&c| c < gap_threshold).collect();

    // Walk left-to-right, collapsing short "gap" runs into the surrounding blob.
    let mut blobs = Vec::new();
    let mut i = 0u32;
    while i < w {
        // skip leading gap
        while i < w && is_gap[i as usize] {
            i += 1;
        }
        if i >= w {
            break;
        }
        let start = i;
        let mut end = i;
        loop {
            // Extend through non-gap columns
            while i < w && !is_gap[i as usize] {
                end = i;
                i += 1;
            }
            // Look ahead: is the next gap short enough to bridge?
            let gap_start = i;
            while i < w && is_gap[i as usize] {
                i += 1;
            }
            let gap_len = i - gap_start;
            if i < w && gap_len < min_gap_width {
                // bridge: continue extending the same blob
                continue;
            }
            break;
        }
        blobs.push((start, end));
    }
    blobs
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
