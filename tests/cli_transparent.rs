mod common;

use assert_cmd::Command;
use image::{DynamicImage, GenericImageView, Rgb, RgbImage};
use tempfile::TempDir;

/// A magenta canvas with a black square in the middle — the kind of
/// shape the chroma-key feature is designed to handle.
fn magenta_with_black_square(size: u32) -> DynamicImage {
    let mut img = RgbImage::from_pixel(size, size, Rgb([255, 0, 255]));
    let margin = size / 4;
    for y in margin..size - margin {
        for x in margin..size - margin {
            img.put_pixel(x, y, Rgb([0, 0, 0]));
        }
    }
    DynamicImage::ImageRgb8(img)
}

#[test]
fn transparent_default_writes_sibling_png() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("icon.png");
    common::write_image(&magenta_with_black_square(64), &input);

    Command::cargo_bin("pixa")
        .unwrap()
        .args(["transparent", input.to_str().unwrap()])
        .assert()
        .success();

    let expected = dir.path().join("icon.transparent.png");
    assert!(
        expected.exists(),
        "default output {} should exist",
        expected.display()
    );
}

#[test]
fn transparent_keys_out_corner_pixels() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("icon.png");
    let output = dir.path().join("out.png");
    common::write_image(&magenta_with_black_square(64), &input);

    Command::cargo_bin("pixa")
        .unwrap()
        .args([
            "transparent",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let img = image::open(&output).unwrap().to_rgba8();
    // Corners are pure magenta → must be fully transparent.
    assert_eq!(img.get_pixel(0, 0)[3], 0);
    assert_eq!(img.get_pixel(63, 0)[3], 0);
    assert_eq!(img.get_pixel(0, 63)[3], 0);
    assert_eq!(img.get_pixel(63, 63)[3], 0);
    // Middle is solid black → must stay fully opaque.
    assert_eq!(img.get_pixel(32, 32)[3], 255);
    assert_eq!(img.get_pixel(32, 32)[0], 0);
}

#[test]
fn transparent_respects_explicit_bg() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("icon.png");
    let output = dir.path().join("out.png");
    // A solid-black image — normally auto-detect would pick black as bg.
    common::write_image(
        &DynamicImage::ImageRgb8(RgbImage::from_pixel(32, 32, Rgb([0, 0, 0]))),
        &input,
    );

    Command::cargo_bin("pixa")
        .unwrap()
        .args([
            "transparent",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--bg",
            "#FF00FF",
        ])
        .assert()
        .success();

    // Every pixel is black → far from magenta → everything must be
    // fully opaque.
    let img = image::open(&output).unwrap().to_rgba8();
    for p in img.pixels() {
        assert_eq!(p[3], 255);
    }
}

#[test]
fn transparent_rejects_bad_hex_color() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("icon.png");
    common::write_image(&magenta_with_black_square(32), &input);

    Command::cargo_bin("pixa")
        .unwrap()
        .args([
            "transparent",
            input.to_str().unwrap(),
            "--bg",
            "not-a-color",
        ])
        .assert()
        .failure();
}

#[test]
fn transparent_forces_png_output_extension() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("icon.png");
    let output_jpg = dir.path().join("out.jpg");
    common::write_image(&magenta_with_black_square(32), &input);

    Command::cargo_bin("pixa")
        .unwrap()
        .args([
            "transparent",
            input.to_str().unwrap(),
            "-o",
            output_jpg.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Requested .jpg path should have been coerced to .png since
    // transparency needs PNG.
    let expected = dir.path().join("out.png");
    assert!(
        expected.exists(),
        "PNG output {} expected",
        expected.display()
    );
    let (w, _) = image::open(&expected).unwrap().dimensions();
    assert_eq!(w, 32);
}

#[test]
fn transparent_recursive_processes_directory() {
    let dir = TempDir::new().unwrap();
    let in_dir = dir.path().join("in");
    std::fs::create_dir_all(&in_dir).unwrap();
    for name in &["a.png", "b.png"] {
        common::write_image(&magenta_with_black_square(32), &in_dir.join(name));
    }
    let out_dir = dir.path().join("out");

    Command::cargo_bin("pixa")
        .unwrap()
        .args([
            "transparent",
            in_dir.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "-r",
        ])
        .assert()
        .success();

    assert!(out_dir.join("a.png").exists());
    assert!(out_dir.join("b.png").exists());
    for name in &["a.png", "b.png"] {
        let img = image::open(out_dir.join(name)).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(0, 0)[3], 0);
    }
}
