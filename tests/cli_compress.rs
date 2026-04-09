mod common;

use assert_cmd::Command;
use image::GenericImageView;
use tempfile::TempDir;

#[test]
fn compress_single_file_explicit_output() {
    let (_dir, input) = common::tmp_png(256, 256);
    let output = input.with_file_name("out.webp");

    Command::cargo_bin("pixa")
        .unwrap()
        .args([
            "compress",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(output.exists(), "output webp should be created");
    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
}

#[test]
fn compress_without_output_writes_min_suffix() {
    let (_dir, input) = common::tmp_png(200, 200);

    Command::cargo_bin("pixa")
        .unwrap()
        .args(["compress", input.to_str().unwrap()])
        .assert()
        .success();

    let expected = input.with_file_name("input.min.png");
    assert!(
        expected.exists(),
        "expected {} to exist",
        expected.display()
    );
}

#[test]
fn compress_with_max_edge_resizes() {
    let (_dir, input) = common::tmp_png(2000, 1000);
    let output = input.with_file_name("out.webp");

    Command::cargo_bin("pixa")
        .unwrap()
        .args([
            "compress",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--max",
            "800",
        ])
        .assert()
        .success();

    let decoded = image::open(&output).unwrap();
    let (w, h) = decoded.dimensions();
    assert!(w <= 800 && h <= 800, "got {w}x{h}");
    assert!(w == 800 || h == 800, "one edge should equal the limit");
}

#[test]
fn compress_recursive_mirrors_directory() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("in");
    let dst = dir.path().join("out");
    std::fs::create_dir_all(src.join("sub")).unwrap();

    common::write_image(&common::gradient_rgb(200, 200), &src.join("a.png"));
    common::write_image(&common::gradient_rgb(150, 150), &src.join("sub/b.png"));

    Command::cargo_bin("pixa")
        .unwrap()
        .args([
            "compress",
            src.to_str().unwrap(),
            "-r",
            "-o",
            dst.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(dst.join("a.png").exists());
    assert!(dst.join("sub/b.png").exists());
}

#[test]
fn compress_missing_input_fails() {
    Command::cargo_bin("pixa")
        .unwrap()
        .args(["compress", "/nonexistent/image.png"])
        .assert()
        .failure();
}
