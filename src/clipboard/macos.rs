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

/// Return the resolved filesystem path behind the pasteboard's
/// `public.file-url` entry, if present.
///
/// Finder puts a `file://` URL here on Cmd+C. The data is a UTF-8
/// encoded URL string with `%XX` escapes for non-ASCII characters.
/// Returns `Ok(None)` when no file URL is on the clipboard, or when
/// the URL cannot be parsed back to a path.
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
        Ok(parse_file_url(raw))
    }
}

/// Parse a `file://[host]/path` URL into a filesystem `PathBuf`, decoding
/// percent-escaped bytes. Returns `None` for non-`file://` URLs.
fn parse_file_url(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    // macOS Finder emits `file:///abs/path` — no authority component.
    // Accept both `file:///path` and `file://localhost/path`.
    let path_part = rest.strip_prefix("localhost").unwrap_or(rest);
    Some(PathBuf::from(percent_decode(path_part)))
}

/// Decode `%XX` byte escapes in a URL path, preserving UTF-8 byte
/// sequences (e.g. `%E3%81%82` → `あ`).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_file_url() {
        let p = parse_file_url("file:///Users/me/pic.png").unwrap();
        assert_eq!(p, PathBuf::from("/Users/me/pic.png"));
    }

    #[test]
    fn parses_localhost_prefix() {
        let p = parse_file_url("file://localhost/Users/me/pic.png").unwrap();
        assert_eq!(p, PathBuf::from("/Users/me/pic.png"));
    }

    #[test]
    fn decodes_percent_escapes() {
        let p = parse_file_url("file:///tmp/photo%20%281%29.jpg").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/photo (1).jpg"));
    }

    #[test]
    fn decodes_utf8_percent_escapes() {
        // "あ" in UTF-8 is E3 81 82.
        let p = parse_file_url("file:///tmp/%E3%81%82.png").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/あ.png"));
    }

    #[test]
    fn rejects_non_file_url() {
        assert!(parse_file_url("https://example.com/pic.png").is_none());
    }
}
