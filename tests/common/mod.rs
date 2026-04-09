//! Shared helpers for CLI integration tests.
//!
//! These tests exercise the `pixa` binary as a whole by invoking it
//! via `assert_cmd`, rather than calling library functions directly.
//! They live in `tests/` so each file is compiled as a separate
//! integration-test binary.

#![allow(dead_code)]

use image::{DynamicImage, Rgb, RgbImage, Rgba, RgbaImage};
use std::path::Path;
use tempfile::TempDir;

/// Build a deterministic RGB gradient image for use as compress /
/// convert / info input.
pub fn gradient_rgb(w: u32, h: u32) -> DynamicImage {
    let mut img = RgbImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            img.put_pixel(
                x,
                y,
                Rgb([
                    ((x * 255 / w.max(1)) % 256) as u8,
                    ((y * 255 / h.max(1)) % 256) as u8,
                    (((x + y) * 127 / (w + h).max(1)) % 256) as u8,
                ]),
            );
        }
    }
    DynamicImage::ImageRgb8(img)
}

/// RGBA variant that forces a non-trivial alpha channel, for tests
/// that need to verify alpha handling.
pub fn gradient_rgba(w: u32, h: u32) -> DynamicImage {
    let mut img = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            img.put_pixel(
                x,
                y,
                Rgba([200, 100, 50, ((x + y) * 255 / (w + h).max(1)) as u8]),
            );
        }
    }
    DynamicImage::ImageRgba8(img)
}

/// Build a very simple "sprite sheet" for split tests: `count` dark
/// squares on a solid cream background, evenly spaced horizontally.
pub fn sheet(count: u32) -> DynamicImage {
    let block = 60u32;
    let gap = 40u32;
    let margin = 40u32;
    let width = margin * 2 + count * block + (count - 1) * gap;
    let height = 200u32;

    let mut img = RgbImage::from_pixel(width, height, Rgb([246, 239, 221]));
    for i in 0..count {
        let x0 = margin + i * (block + gap);
        let y0 = 70;
        for y in y0..y0 + block {
            for x in x0..x0 + block {
                img.put_pixel(x, y, Rgb([50, 50, 50]));
            }
        }
    }
    DynamicImage::ImageRgb8(img)
}

/// Write an image to disk, returning its path.
pub fn write_image(img: &DynamicImage, path: &Path) {
    img.save(path).expect("write test image");
}

/// Shorthand: create a tempdir and write a gradient PNG to it.
pub fn tmp_png(w: u32, h: u32) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("input.png");
    write_image(&gradient_rgb(w, h), &path);
    (dir, path)
}
