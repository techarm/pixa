pub mod compress;
pub mod convert;
pub mod detect;
pub mod favicon;
pub mod info;
pub mod remove_watermark;
pub mod split;
pub mod style;

use anyhow::Result;
use std::path::{Path, PathBuf};

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "gif", "tiff", "tif"];

/// Collect image files from a path. If `path` is a file, returns it as-is.
/// If a directory, returns all images inside (recursively if `recursive`).
pub fn collect_inputs(path: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.is_dir() {
        anyhow::bail!("Input not found: {}", path.display());
    }

    let mut files = Vec::new();
    walk(path, recursive, &mut files)?;
    files.sort();
    Ok(files)
}

fn walk(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            if recursive {
                walk(&p, true, out)?;
            }
        } else if p.is_file() {
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if IMAGE_EXTS.contains(&ext.to_lowercase().as_str()) {
                    out.push(p);
                }
            }
        }
    }
    Ok(())
}

/// Compute an output path that mirrors `input` relative to `input_root`
/// under `output_root`. If `output_root` is None, returns `input` (in-place).
pub fn mirror_path(input: &Path, input_root: &Path, output_root: Option<&Path>) -> PathBuf {
    match output_root {
        Some(root) => {
            let rel = input.strip_prefix(input_root).unwrap_or(input);
            root.join(rel)
        }
        None => input.to_path_buf(),
    }
}

pub fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

/// Human-readable byte size (e.g. "1.2 MB"). Consistent with info::format_size.
pub fn format_size(bytes: u64) -> String {
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
