//! Image information extraction and display

use image::GenericImageView;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum InfoError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("EXIF error: {0}")]
    Exif(String),
}

/// Comprehensive image information
#[derive(Debug, Serialize, Deserialize)]
pub struct ImageInfo {
    pub file_name: String,
    pub file_size: u64,
    pub file_size_human: String,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub color_type: String,
    pub bit_depth: u32,
    pub has_alpha: bool,
    pub pixel_count: u64,
    pub sha256: String,
    pub exif: Option<HashMap<String, String>>,
}

/// Get comprehensive information about an image
pub fn get_image_info(path: &Path) -> Result<ImageInfo, InfoError> {
    let metadata = fs::metadata(path)?;
    let file_size = metadata.len();

    // Load image for dimensions and color info
    let img = image::open(path)?;
    let (width, height) = img.dimensions();

    let color = img.color();
    let has_alpha = matches!(
        color,
        image::ColorType::La8
            | image::ColorType::Rgba8
            | image::ColorType::La16
            | image::ColorType::Rgba16
            | image::ColorType::Rgba32F
    );
    let bit_depth = color.bytes_per_pixel() as u32 * 8 / color.channel_count() as u32;

    // Detect format from file content
    let format = image::ImageFormat::from_path(path)
        .map(|f| format!("{f:?}").to_uppercase())
        .unwrap_or_else(|_| "Unknown".to_string());

    // SHA256 hash
    let sha256 = compute_sha256(path)?;

    // EXIF data
    let exif = read_exif(path).ok();

    Ok(ImageInfo {
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        file_size,
        file_size_human: format_size(file_size),
        format,
        width,
        height,
        color_type: format!("{color:?}"),
        bit_depth,
        has_alpha,
        pixel_count: width as u64 * height as u64,
        sha256,
        exif,
    })
}

fn compute_sha256(path: &Path) -> Result<String, std::io::Error> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn read_exif(path: &Path) -> Result<HashMap<String, String>, InfoError> {
    let file = std::fs::File::open(path)?;
    let mut buf_reader = std::io::BufReader::new(&file);
    let exif_reader = exif::Reader::new();
    let exif = exif_reader
        .read_from_container(&mut buf_reader)
        .map_err(|e| InfoError::Exif(e.to_string()))?;

    let mut map = HashMap::new();
    for field in exif.fields() {
        let tag_name = format!("{}", field.tag);
        let value = field.display_value().with_unit(&exif).to_string();
        map.insert(tag_name, value);
    }

    Ok(map)
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
