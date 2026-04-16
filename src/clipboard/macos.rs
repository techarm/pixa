//! macOS-native clipboard reader via NSPasteboard.
//!
//! `arboard::get_image()` always decodes to RGBA8, which loses the original
//! encoded bytes. When a user copies a PNG from a browser or screenshot
//! tool, the system pasteboard actually holds the raw PNG bytes under the
//! `public.png` UTI — reading those directly gives `pixa paste` true
//! byte-passthrough with no re-encoding.
//!
//! Finder (Cmd+C on an image file) puts `public.file-url` on the
//! pasteboard instead of the image's bytes — [`read_file_url`] resolves
//! it back to a filesystem path so the rest of pixa can read the source
//! file directly, preserving metadata and avoiding any re-encoding.

use std::path::PathBuf;

use objc2_app_kit::NSPasteboard;
use objc2_foundation::{NSString, NSURL};

use super::ClipboardError;

/// Return the `public.png` payload from the general pasteboard, if present.
/// Returns `Ok(None)` when the clipboard doesn't currently have PNG data
/// (e.g. the user copied a TIFF-only image, text, or nothing).
pub(super) fn read_png_bytes() -> Result<Option<Vec<u8>>, ClipboardError> {
    // SAFETY: NSPasteboard general access is thread-safe per Apple docs.
    // We copy the bytes into an owned Vec before the NSData goes out of
    // scope, so no dangling-pointer concerns.
    unsafe {
        let pasteboard = NSPasteboard::generalPasteboard();
        let png_type = NSString::from_str("public.png");
        let Some(data) = pasteboard.dataForType(&png_type) else {
            return Ok(None);
        };
        let slice = data.as_bytes_unchecked();
        if slice.is_empty() {
            return Ok(None);
        }
        Ok(Some(slice.to_vec()))
    }
}

/// Return the resolved filesystem path behind the pasteboard's
/// `public.file-url` entry, if present.
///
/// Finder puts a `file://` URL here on Cmd+C. Two formats show up in
/// practice:
///
/// - Plain path: `file:///Users/me/pic.png` (what e.g. Preview or our
///   own test fixture emits).
/// - File ID reference: `file:///.file/id=6571367.42756844` (what
///   Finder actually uses — an inode-based handle that's only
///   resolvable through `NSURL`).
///
/// We hand the raw URL string to `NSURL` and read back its `.path`
/// property, which transparently resolves both forms into a POSIX
/// path usable by `std::fs` / `image::open`. Returns `Ok(None)` when
/// no file URL is on the clipboard or the URL can't be resolved.
pub(super) fn read_file_url() -> Result<Option<PathBuf>, ClipboardError> {
    unsafe {
        let pasteboard = NSPasteboard::generalPasteboard();
        let url_type = NSString::from_str("public.file-url");
        let Some(data) = pasteboard.dataForType(&url_type) else {
            return Ok(None);
        };
        let slice = data.as_bytes_unchecked();
        if slice.is_empty() {
            return Ok(None);
        }
        let raw = std::str::from_utf8(slice)
            .unwrap_or("")
            .trim_end_matches('\0');
        if raw.is_empty() {
            return Ok(None);
        }
        let url_str = NSString::from_str(raw);
        let Some(url) = NSURL::URLWithString(&url_str) else {
            return Ok(None);
        };
        // Guard against non-file URLs (e.g. an `https://…` link that an
        // app mistakenly stored under `public.file-url`). `NSURL.path`
        // would still return a plausible-looking path component for
        // those, which would then fail with a confusing open error.
        let Some(scheme) = url.scheme() else {
            return Ok(None);
        };
        if scheme.to_string() != "file" {
            return Ok(None);
        }
        let Some(ns_path) = url.path() else {
            return Ok(None);
        };
        Ok(Some(PathBuf::from(ns_path.to_string())))
    }
}
