//! macOS-native clipboard reader via NSPasteboard.
//!
//! `arboard::get_image()` always decodes to RGBA8, which loses the original
//! encoded bytes. When a user copies a PNG from a browser or screenshot
//! tool, the system pasteboard actually holds the raw PNG bytes under the
//! `public.png` UTI — reading those directly gives `pixa paste` true
//! byte-passthrough with no re-encoding.

use objc2_app_kit::NSPasteboard;
use objc2_foundation::NSString;

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
