//! Read images from the OS clipboard.
//!
//! Two entry points:
//!
//! - [`read_image`] returns the clipboard contents as a [`DynamicImage`] via
//!   the cross-platform `arboard` crate. This powers `@clipboard` input for
//!   every pixa subcommand.
//! - [`read_native_png`] tries to return the original PNG bytes from the
//!   clipboard without decoding. Implemented on macOS via NSPasteboard;
//!   returns `None` on other platforms. Used by `pixa paste` to preserve
//!   encoder settings and metadata when the source was a real PNG.

use image::DynamicImage;
use std::path::PathBuf;
use thiserror::Error;

mod arboard_impl;
#[cfg(target_os = "macos")]
mod macos;

#[derive(Error, Debug)]
pub enum ClipboardError {
    #[error("Clipboard is empty or does not contain an image")]
    NoImage,
    #[error("Clipboard unavailable (no display server or platform error): {0}")]
    Unavailable(String),
    #[error("Failed to decode clipboard image: {0}")]
    Decode(String),
}

/// Read the clipboard as a `DynamicImage` (RGBA8). Works on all platforms
/// where arboard works. On headless Linux it returns `Unavailable`.
pub fn read_image() -> Result<DynamicImage, ClipboardError> {
    arboard_impl::read_image()
}

/// Return raw PNG bytes directly from the clipboard, if the platform
/// supports it and the clipboard actually has PNG data.
///
/// - macOS: reads `public.png` from NSPasteboard.
/// - Other platforms: always returns `Ok(None)` for now (future work:
///   follow-up issues for Windows and Linux).
pub fn read_native_png() -> Result<Option<Vec<u8>>, ClipboardError> {
    #[cfg(target_os = "macos")]
    {
        macos::read_png_bytes()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(None)
    }
}

/// Return the filesystem path behind a file-URL entry on the clipboard,
/// if present. On macOS this reads `public.file-url` from NSPasteboard
/// — the UTI Finder (and most apps) use when you Cmd+C an image file.
///
/// When the user copied a file rather than raw image data, resolving
/// this path lets pixa open the source file directly, preserving all
/// metadata and avoiding any re-encoding through arboard's decoded
/// RGBA representation.
///
/// Returns `Ok(None)` when no file URL is on the clipboard, when the
/// URL cannot be parsed, or on non-macOS platforms (tracked in
/// follow-up issues for Windows and Linux).
pub fn read_file_url() -> Result<Option<PathBuf>, ClipboardError> {
    #[cfg(target_os = "macos")]
    {
        macos::read_file_url()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(None)
    }
}
