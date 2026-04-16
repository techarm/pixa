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

/// Get comprehensive information about an image on disk.
pub fn get_image_info(path: &Path) -> Result<ImageInfo, InfoError> {
    let metadata = fs::metadata(path)?;
    let file_size = metadata.len();

    let img = image::open(path)?;

    let format = image::ImageFormat::from_path(path)
        .map(|f| format!("{f:?}").to_uppercase())
        .unwrap_or_else(|_| "Unknown".to_string());

    let sha256 = compute_sha256(path)?;
    let exif = read_exif(path).ok();

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    Ok(build_info(&img, file_name, file_size, format, sha256, exif))
}

/// Build info from an already-loaded image plus optional encoded bytes.
/// Intended for clipboard input, where there's no file on disk: callers
/// pass the clipboard's encoded bytes (typically PNG, after re-encoding
/// from RGBA) so `file_size` and `sha256` remain meaningful. EXIF is
/// always `None` — the clipboard path strips containers like EXIF well
/// before we see it.
pub fn get_image_info_from_image(
    img: &image::DynamicImage,
    source_label: &str,
    raw_bytes: Option<&[u8]>,
) -> ImageInfo {
    let file_size = raw_bytes.map(|b| b.len() as u64).unwrap_or(0);
    let sha256 = raw_bytes
        .map(|b| {
            let mut hasher = Sha256::new();
            hasher.update(b);
            format!("{:x}", hasher.finalize())
        })
        .unwrap_or_default();
    let format = "RGBA (clipboard)".to_string();

    build_info(
        img,
        source_label.to_string(),
        file_size,
        format,
        sha256,
        None,
    )
}

fn build_info(
    img: &image::DynamicImage,
    file_name: String,
    file_size: u64,
    format: String,
    sha256: String,
    exif: Option<HashMap<String, String>>,
) -> ImageInfo {
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

    ImageInfo {
        file_name,
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
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage, Rgba, RgbaImage};
    use tempfile::TempDir;

    fn write_rgb_png(path: &Path, w: u32, h: u32) {
        let mut img = RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                img.put_pixel(x, y, Rgb([(x % 256) as u8, (y % 256) as u8, 64]));
            }
        }
        DynamicImage::ImageRgb8(img).save(path).unwrap();
    }

    fn write_rgba_png(path: &Path, w: u32, h: u32) {
        let mut img = RgbaImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                img.put_pixel(x, y, Rgba([200, 100, 50, 128]));
            }
        }
        DynamicImage::ImageRgba8(img).save(path).unwrap();
    }

    #[test]
    fn info_reads_png_dimensions() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("img.png");
        write_rgb_png(&path, 128, 64);

        let info = get_image_info(&path).unwrap();
        assert_eq!(info.width, 128);
        assert_eq!(info.height, 64);
        assert_eq!(info.pixel_count, 128 * 64);
    }

    #[test]
    fn info_reports_file_size() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("img.png");
        write_rgb_png(&path, 32, 32);

        let info = get_image_info(&path).unwrap();
        let expected = std::fs::metadata(&path).unwrap().len();
        assert_eq!(info.file_size, expected);
        assert!(info.file_size > 0);
    }

    #[test]
    fn info_format_is_uppercase() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("img.png");
        write_rgb_png(&path, 16, 16);

        let info = get_image_info(&path).unwrap();
        assert_eq!(info.format, "PNG");
    }

    #[test]
    fn info_detects_alpha_channel() {
        let dir = TempDir::new().unwrap();
        let rgba_path = dir.path().join("rgba.png");
        let rgb_path = dir.path().join("rgb.png");
        write_rgba_png(&rgba_path, 16, 16);
        write_rgb_png(&rgb_path, 16, 16);

        assert!(get_image_info(&rgba_path).unwrap().has_alpha);
        assert!(!get_image_info(&rgb_path).unwrap().has_alpha);
    }

    #[test]
    fn info_sha256_is_deterministic() {
        let dir = TempDir::new().unwrap();
        let a = dir.path().join("a.png");
        let b = dir.path().join("b.png");
        write_rgb_png(&a, 16, 16);
        std::fs::copy(&a, &b).unwrap();

        let info_a = get_image_info(&a).unwrap();
        let info_b = get_image_info(&b).unwrap();
        assert_eq!(info_a.sha256, info_b.sha256);
        assert_eq!(info_a.sha256.len(), 64, "SHA-256 hex length");
    }

    #[test]
    fn info_file_name_extracted() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cute-cat.png");
        write_rgb_png(&path, 16, 16);

        let info = get_image_info(&path).unwrap();
        assert_eq!(info.file_name, "cute-cat.png");
    }

    #[test]
    fn format_size_human_readable() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2048), "2.0 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2.00 GB");
    }

    #[test]
    fn info_from_image_uses_raw_bytes_for_sha() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(8, 8));
        let bytes: Vec<u8> = (0..128u8).collect();

        let info = get_image_info_from_image(&img, "@clipboard", Some(&bytes));
        assert_eq!(info.file_name, "@clipboard");
        assert_eq!(info.file_size, bytes.len() as u64);
        assert_eq!(info.width, 8);
        assert_eq!(info.height, 8);
        assert!(info.has_alpha);
        assert!(info.exif.is_none(), "clipboard info has no EXIF");

        // SHA-256 is deterministic for the given bytes.
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let expected = format!("{:x}", hasher.finalize());
        assert_eq!(info.sha256, expected);
    }

    #[test]
    fn info_from_image_without_raw_bytes_zeroes_size_and_hash() {
        let img = DynamicImage::ImageRgb8(RgbImage::new(4, 4));
        let info = get_image_info_from_image(&img, "@clipboard", None);
        assert_eq!(info.file_size, 0);
        assert_eq!(info.sha256, "");
    }
}
