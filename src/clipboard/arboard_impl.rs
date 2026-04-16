//! Cross-platform clipboard reader via the `arboard` crate.
//!
//! arboard returns decoded RGBA pixels, not the original encoded bytes.
//! That is enough for every pixa processing command but precludes true
//! byte-passthrough — see `macos.rs` for the native macOS path.

use image::{DynamicImage, RgbaImage};

use super::ClipboardError;

pub(super) fn read_image() -> Result<DynamicImage, ClipboardError> {
    let mut cb =
        arboard::Clipboard::new().map_err(|e| ClipboardError::Unavailable(e.to_string()))?;
    let data = cb.get_image().map_err(map_get_err)?;
    let rgba = RgbaImage::from_raw(
        data.width as u32,
        data.height as u32,
        data.bytes.into_owned(),
    )
    .ok_or_else(|| {
        ClipboardError::Decode(format!(
            "arboard returned {} bytes for {}x{} image",
            data.width * data.height * 4,
            data.width,
            data.height
        ))
    })?;
    Ok(DynamicImage::ImageRgba8(rgba))
}

fn map_get_err(e: arboard::Error) -> ClipboardError {
    match e {
        arboard::Error::ContentNotAvailable => ClipboardError::NoImage,
        other => ClipboardError::Unavailable(other.to_string()),
    }
}
