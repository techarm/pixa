//! Image format conversion

use image::DynamicImage;
use std::path::Path;
use thiserror::Error;
use tracing::info;

use crate::ImageFormat;

#[derive(Error, Debug)]
pub enum ConvertError {
    #[error("Unsupported input format: {0}")]
    UnsupportedInput(String),
    #[error("Unsupported output format: {0}")]
    UnsupportedOutput(String),
    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("WebP encoding error")]
    WebpEncode,
}

/// Convert an image from one format to another
pub fn convert_image(input: &Path, output: &Path) -> Result<(), ConvertError> {
    let in_ext = input.extension().and_then(|e| e.to_str()).unwrap_or("");
    let out_ext = output.extension().and_then(|e| e.to_str()).unwrap_or("");

    let out_format = ImageFormat::from_extension(out_ext)
        .ok_or_else(|| ConvertError::UnsupportedOutput(out_ext.to_string()))?;

    info!(
        "Converting: {} ({}) -> {} ({})",
        input.display(),
        in_ext,
        output.display(),
        out_format.extension()
    );

    let img = image::open(input)?;

    match out_format {
        ImageFormat::WebP => {
            save_as_webp(&img, output, 90.0)?;
        }
        ImageFormat::Jpeg => {
            // Convert RGBA to RGB for JPEG (no alpha support)
            let rgb = img.to_rgb8();
            rgb.save(output)?;
        }
        _ => {
            img.save(output)?;
        }
    }

    Ok(())
}

fn save_as_webp(img: &DynamicImage, output: &Path, quality: f32) -> Result<(), ConvertError> {
    let mem = if img.color().has_alpha() {
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        webp::Encoder::from_rgba(rgba.as_raw(), w, h).encode(quality)
    } else {
        let rgb = img.to_rgb8();
        let (w, h) = (rgb.width(), rgb.height());
        webp::Encoder::from_rgb(rgb.as_raw(), w, h).encode(quality)
    };

    std::fs::write(output, &*mem)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage, Rgba, RgbaImage};
    use tempfile::TempDir;

    fn write_png(path: &Path, w: u32, h: u32) {
        let mut img = RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                img.put_pixel(x, y, Rgb([(x % 256) as u8, (y % 256) as u8, 128]));
            }
        }
        DynamicImage::ImageRgb8(img)
            .save(path)
            .expect("write test png");
    }

    fn write_png_with_alpha(path: &Path, w: u32, h: u32) {
        let mut img = RgbaImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                // Left half opaque red, right half fully transparent.
                let alpha = if x < w / 2 { 255 } else { 0 };
                img.put_pixel(x, y, Rgba([255, 0, 0, alpha]));
            }
        }
        DynamicImage::ImageRgba8(img)
            .save(path)
            .expect("write test png");
    }

    #[test]
    fn convert_png_to_jpeg() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("in.png");
        let output = dir.path().join("out.jpg");
        write_png(&input, 64, 64);

        convert_image(&input, &output).unwrap();
        let bytes = std::fs::read(&output).unwrap();
        assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF], "JPEG magic");
    }

    #[test]
    fn convert_png_to_webp() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("in.png");
        let output = dir.path().join("out.webp");
        write_png(&input, 64, 64);

        convert_image(&input, &output).unwrap();
        let bytes = std::fs::read(&output).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WEBP");
    }

    #[test]
    fn convert_transparent_png_to_webp_preserves_alpha() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("in.png");
        let output = dir.path().join("out.webp");
        write_png_with_alpha(&input, 64, 64);

        convert_image(&input, &output).unwrap();

        let decoded = image::open(&output).unwrap();
        assert!(
            decoded.color().has_alpha(),
            "webp output must retain alpha channel"
        );
        let rgba = decoded.to_rgba8();
        assert_eq!(rgba.get_pixel(0, 0)[3], 255, "opaque half stays opaque");
        assert_eq!(
            rgba.get_pixel(63, 0)[3],
            0,
            "transparent half stays transparent"
        );
    }

    #[test]
    fn convert_png_to_png_roundtrip() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("in.png");
        let output = dir.path().join("out.png");
        write_png(&input, 32, 32);

        convert_image(&input, &output).unwrap();
        let decoded = image::open(&output).unwrap();
        assert_eq!(decoded.width(), 32);
        assert_eq!(decoded.height(), 32);
    }

    #[test]
    fn convert_unsupported_output_errors() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("in.png");
        let output = dir.path().join("out.xyz");
        write_png(&input, 16, 16);

        let err = convert_image(&input, &output).unwrap_err();
        assert!(matches!(err, ConvertError::UnsupportedOutput(_)));
    }

    #[test]
    fn convert_missing_input_errors() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("does-not-exist.png");
        let output = dir.path().join("out.webp");

        assert!(convert_image(&input, &output).is_err());
    }
}
