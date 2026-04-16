pub mod clipboard;
pub mod compress;
pub mod convert;
pub mod favicon;
pub mod info;
pub mod split;
pub mod transparent;
pub mod watermark;

pub use clipboard::ClipboardError;
pub use compress::{CompressResult, compress_image};
pub use convert::convert_image;
pub use favicon::{
    FaviconOptions, FaviconResult, generate_favicon_set, generate_favicon_set_from_image,
};
pub use info::{ImageInfo, get_image_info};
pub use watermark::{DetectionResult, WatermarkEngine, WatermarkSize};

/// Supported image formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ImageFormat {
    Jpeg,
    Png,
    WebP,
    Bmp,
    Gif,
    Tiff,
}

impl ImageFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "png" => Some(Self::Png),
            "webp" => Some(Self::WebP),
            "bmp" => Some(Self::Bmp),
            "gif" => Some(Self::Gif),
            "tiff" | "tif" => Some(Self::Tiff),
            _ => None,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::WebP => "webp",
            Self::Bmp => "bmp",
            Self::Gif => "gif",
            Self::Tiff => "tiff",
        }
    }
}
