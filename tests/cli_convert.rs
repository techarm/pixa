mod common;

use assert_cmd::Command;

#[test]
fn convert_png_to_webp_single_file() {
    let (_dir, input) = common::tmp_png(128, 128);
    let output = input.with_file_name("out.webp");

    Command::cargo_bin("pixa")
        .unwrap()
        .args(["convert", input.to_str().unwrap(), output.to_str().unwrap()])
        .assert()
        .success();

    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
}

#[test]
fn convert_png_to_jpeg_single_file() {
    let (_dir, input) = common::tmp_png(128, 128);
    let output = input.with_file_name("out.jpg");

    Command::cargo_bin("pixa")
        .unwrap()
        .args(["convert", input.to_str().unwrap(), output.to_str().unwrap()])
        .assert()
        .success();

    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF]);
}

#[test]
fn convert_unsupported_output_extension_fails() {
    let (_dir, input) = common::tmp_png(32, 32);
    let output = input.with_file_name("out.xyz");

    Command::cargo_bin("pixa")
        .unwrap()
        .args(["convert", input.to_str().unwrap(), output.to_str().unwrap()])
        .assert()
        .failure();
}
