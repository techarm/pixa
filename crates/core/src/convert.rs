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
    let rgb = img.to_rgb8();
    let (w, h) = (rgb.width(), rgb.height());

    let encoder = webp::Encoder::from_rgb(rgb.as_raw(), w, h);
    let mem = encoder.encode(quality);

    std::fs::write(output, &*mem)?;
    Ok(())
}
