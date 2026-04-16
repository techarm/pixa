//! Integration tests for the clipboard-input feature.
//!
//! These tests clobber the developer's actual clipboard contents while
//! running — the macOS pasteboard has no per-process isolation. Tests
//! are serialized within the file (single test binary) but concurrent
//! with other test files; hence the whole file is gated to macOS and
//! run locally rather than on shared CI.

#![cfg(target_os = "macos")]

mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Cursor;
use tempfile::TempDir;

/// Encode an image as PNG into a Vec<u8> — used to produce a known
/// byte sequence for the byte-passthrough test.
fn encode_png(img: &image::DynamicImage) -> Vec<u8> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .expect("encode test png");
    buf
}

#[test]
fn paste_writes_png_file() {
    let _lock = common::clipboard_lock();
    common::set_clipboard_image(&common::gradient_rgba(64, 48));
    let dir = TempDir::new().unwrap();
    let out = dir.path().join("out.png");

    Command::cargo_bin("pixa")
        .unwrap()
        .args(["paste", out.to_str().unwrap()])
        .assert()
        .success();

    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(
        &bytes[..8],
        &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        "PNG signature"
    );
    let decoded = image::open(&out).unwrap();
    assert_eq!(decoded.width(), 64);
    assert_eq!(decoded.height(), 48);
}

#[test]
fn paste_byte_passthrough_preserves_source_bytes() {
    let _lock = common::clipboard_lock();
    // Write known PNG bytes to the pasteboard under public.png. The
    // paste command must write those exact bytes back, without
    // going through arboard / re-encode.
    let source_png = encode_png(&common::gradient_rgb(40, 30));
    common::set_clipboard_png_bytes(&source_png);

    let dir = TempDir::new().unwrap();
    let out = dir.path().join("out.png");

    Command::cargo_bin("pixa")
        .unwrap()
        .args(["paste", out.to_str().unwrap()])
        .assert()
        .success();

    let written = std::fs::read(&out).unwrap();
    assert_eq!(
        written, source_png,
        "paste must copy source PNG bytes verbatim when target is .png"
    );
}

#[test]
fn paste_stdout_requires_format() {
    let _lock = common::clipboard_lock();
    common::set_clipboard_image(&common::gradient_rgb(16, 16));
    Command::cargo_bin("pixa")
        .unwrap()
        .args(["paste", "-"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--format"));
}

#[test]
fn paste_stdout_writes_png_bytes() {
    let _lock = common::clipboard_lock();
    common::set_clipboard_image(&common::gradient_rgb(32, 32));
    let output = Command::cargo_bin("pixa")
        .unwrap()
        .args(["paste", "-", "--format", "png"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        &output.stdout[..8],
        &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        "stdout must contain a PNG"
    );
}

#[test]
fn paste_unknown_extension_errors() {
    let _lock = common::clipboard_lock();
    common::set_clipboard_image(&common::gradient_rgb(16, 16));
    let dir = TempDir::new().unwrap();
    let out = dir.path().join("out.xyz");
    Command::cargo_bin("pixa")
        .unwrap()
        .args(["paste", out.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Cannot infer format"));
}

#[test]
fn info_clipboard_reports_dimensions() {
    let _lock = common::clipboard_lock();
    common::set_clipboard_image(&common::gradient_rgb(96, 72));
    Command::cargo_bin("pixa")
        .unwrap()
        .args(["info", "@clipboard"])
        .assert()
        .success()
        .stdout(predicate::str::contains("96×72"))
        .stdout(predicate::str::contains("@clipboard"));
}

#[test]
fn info_clipboard_json_omits_exif() {
    let _lock = common::clipboard_lock();
    common::set_clipboard_image(&common::gradient_rgb(16, 16));
    let output = Command::cargo_bin("pixa")
        .unwrap()
        .args(["info", "@clip", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["exif"].is_null(), "EXIF must be null for clipboard");
    assert_eq!(json["file_name"], "@clipboard");
}

#[test]
fn compress_clipboard_requires_output() {
    let _lock = common::clipboard_lock();
    common::set_clipboard_image(&common::gradient_rgb(32, 32));
    Command::cargo_bin("pixa")
        .unwrap()
        .args(["compress", "@clipboard"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--output is required"));
}

#[test]
fn compress_clipboard_writes_webp() {
    let _lock = common::clipboard_lock();
    common::set_clipboard_image(&common::gradient_rgb(128, 128));
    let dir = TempDir::new().unwrap();
    let out = dir.path().join("out.webp");

    Command::cargo_bin("pixa")
        .unwrap()
        .args(["compress", "@clipboard", "-o", out.to_str().unwrap()])
        .assert()
        .success();

    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
}

#[test]
fn convert_clipboard_alias_c_works() {
    let _lock = common::clipboard_lock();
    common::set_clipboard_image(&common::gradient_rgb(48, 48));
    let dir = TempDir::new().unwrap();
    let out = dir.path().join("out.webp");

    Command::cargo_bin("pixa")
        .unwrap()
        .args(["convert", "@c", out.to_str().unwrap()])
        .assert()
        .success();

    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
}

#[test]
fn convert_clipboard_alias_clip_works() {
    let _lock = common::clipboard_lock();
    common::set_clipboard_image(&common::gradient_rgb(48, 48));
    let dir = TempDir::new().unwrap();
    let out = dir.path().join("out.png");

    Command::cargo_bin("pixa")
        .unwrap()
        .args(["convert", "@clip", out.to_str().unwrap()])
        .assert()
        .success();
    assert!(out.exists());
}

#[test]
fn compress_clipboard_with_recursive_errors() {
    let _lock = common::clipboard_lock();
    common::set_clipboard_image(&common::gradient_rgb(16, 16));
    let dir = TempDir::new().unwrap();
    let out = dir.path().join("out.png");

    Command::cargo_bin("pixa")
        .unwrap()
        .args(["compress", "@clipboard", "-o", out.to_str().unwrap(), "-r"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--recursive"));
}

#[test]
fn detect_clipboard_runs() {
    let _lock = common::clipboard_lock();
    common::set_clipboard_image(&common::gradient_rgb(256, 256));
    // Gradient has no Gemini watermark; detect should run cleanly and
    // report "not detected" with an @clipboard label.
    Command::cargo_bin("pixa")
        .unwrap()
        .args(["detect", "@clipboard"])
        .assert()
        .success()
        .stdout(predicate::str::contains("@clipboard"))
        .stdout(predicate::str::contains("not detected"));
}

#[test]
fn remove_watermark_clipboard_requires_output() {
    let _lock = common::clipboard_lock();
    common::set_clipboard_image(&common::gradient_rgb(64, 64));
    Command::cargo_bin("pixa")
        .unwrap()
        .args(["rw", "@clipboard"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--output is required"));
}

#[test]
fn remove_watermark_clipboard_if_detected_skips_clean_image() {
    let _lock = common::clipboard_lock();
    common::set_clipboard_image(&common::gradient_rgb(256, 256));
    let dir = TempDir::new().unwrap();
    let out = dir.path().join("out.png");
    // Clean gradient has no watermark; --if-detected short-circuits
    // before touching the image, which exercises the clipboard branch
    // end-to-end without needing a real watermarked fixture.
    Command::cargo_bin("pixa")
        .unwrap()
        .args([
            "rw",
            "@clipboard",
            "-o",
            out.to_str().unwrap(),
            "--if-detected",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("no watermark"));
}

#[test]
fn favicon_clipboard_generates_icon_set() {
    let _lock = common::clipboard_lock();
    common::set_clipboard_image(&common::gradient_rgb(256, 256));
    let dir = TempDir::new().unwrap();
    let out_dir = dir.path().join("favs");

    Command::cargo_bin("pixa")
        .unwrap()
        .args(["favicon", "@clipboard", "-o", out_dir.to_str().unwrap()])
        .assert()
        .success();

    assert!(out_dir.join("favicon.ico").exists());
    assert!(out_dir.join("favicon-16x16.png").exists());
    assert!(out_dir.join("apple-touch-icon.png").exists());
}
