//! Parse and load pixa's unified input source.
//!
//! An input can be either a filesystem path (file or directory) or the OS
//! clipboard, denoted by the literal token `@clipboard` (case-insensitive,
//! aliases `@clip` and `@c`).

use anyhow::{Context, Result};
use image::DynamicImage;
use std::path::{Path, PathBuf};

/// Logical input source for an image-processing subcommand.
#[derive(Debug, Clone)]
pub enum ImageSource {
    Path(PathBuf),
    Clipboard,
}

impl ImageSource {
    /// Parse a positional arg into a source. Clipboard aliases:
    /// `@clipboard`, `@clip`, `@c` (case-insensitive).
    pub fn parse(raw: &Path) -> Self {
        if let Some(s) = raw.to_str() {
            let lower = s.to_ascii_lowercase();
            if matches!(lower.as_str(), "@clipboard" | "@clip" | "@c") {
                return Self::Clipboard;
            }
        }
        Self::Path(raw.to_path_buf())
    }

    pub fn is_clipboard(&self) -> bool {
        matches!(self, Self::Clipboard)
    }

    /// Load the source into an in-memory `DynamicImage`. Callers that need
    /// directory semantics must branch on `is_clipboard()` first.
    pub fn load_image(&self) -> Result<DynamicImage> {
        match self {
            // `ClipboardError`'s Display is already a complete user-facing
            // message (e.g. "Clipboard is empty or does not contain an
            // image") — wrapping with `.context()` would only add noise.
            Self::Clipboard => Ok(pixa::clipboard::read_image()?),
            Self::Path(p) => {
                image::open(p).with_context(|| format!("Failed to open: {}", p.display()))
            }
        }
    }

    /// Human-readable label for log and stdout messages.
    pub fn display_label(&self) -> String {
        match self {
            Self::Clipboard => "@clipboard".to_string(),
            Self::Path(p) => p.display().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_clipboard_aliases() {
        assert!(ImageSource::parse(Path::new("@clipboard")).is_clipboard());
        assert!(ImageSource::parse(Path::new("@clip")).is_clipboard());
        assert!(ImageSource::parse(Path::new("@c")).is_clipboard());
    }

    #[test]
    fn parse_clipboard_is_case_insensitive() {
        assert!(ImageSource::parse(Path::new("@CLIPBOARD")).is_clipboard());
        assert!(ImageSource::parse(Path::new("@Clip")).is_clipboard());
        assert!(ImageSource::parse(Path::new("@C")).is_clipboard());
    }

    #[test]
    fn parse_regular_paths_are_not_clipboard() {
        assert!(!ImageSource::parse(Path::new("./file.png")).is_clipboard());
        assert!(!ImageSource::parse(Path::new("/abs/path.jpg")).is_clipboard());
        assert!(!ImageSource::parse(Path::new("not-a-token.png")).is_clipboard());
        // `@clipboardx` is NOT the clipboard token — substring matches
        // must not trigger clipboard handling.
        assert!(!ImageSource::parse(Path::new("@clipboardx")).is_clipboard());
    }

    #[test]
    fn display_label_matches_source() {
        assert_eq!(ImageSource::Clipboard.display_label(), "@clipboard");
        assert_eq!(
            ImageSource::Path(PathBuf::from("foo/bar.png")).display_label(),
            "foo/bar.png"
        );
    }
}
