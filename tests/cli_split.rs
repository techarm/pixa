mod common;

use assert_cmd::Command;
use tempfile::TempDir;

#[test]
fn split_sheet_produces_named_outputs() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("sheet.png");
    let output = dir.path().join("out");
    common::write_image(&common::sheet(3), &input);

    Command::cargo_bin("pixa")
        .unwrap()
        .args([
            "split",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--names",
            "a,b,c",
        ])
        .assert()
        .success();

    assert!(output.join("a.png").exists());
    assert!(output.join("b.png").exists());
    assert!(output.join("c.png").exists());
}

#[test]
fn split_without_names_uses_numbered_outputs() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("sheet.png");
    let output = dir.path().join("out");
    common::write_image(&common::sheet(2), &input);

    Command::cargo_bin("pixa")
        .unwrap()
        .args([
            "split",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(output.join("1.png").exists());
    assert!(output.join("2.png").exists());
}

#[test]
fn split_preview_flag_writes_preview_image() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("sheet.png");
    let output = dir.path().join("out");
    common::write_image(&common::sheet(3), &input);

    Command::cargo_bin("pixa")
        .unwrap()
        .args([
            "split",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--names",
            "a,b,c",
            "--preview",
        ])
        .assert()
        .success();

    // Preview is written alongside the input file as <stem>-preview.png
    let preview = input.with_file_name("sheet-preview.png");
    assert!(
        preview.exists(),
        "preview {} should exist",
        preview.display()
    );
}

#[test]
fn split_output_sizes_are_uniform() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("sheet.png");
    let output = dir.path().join("out");
    common::write_image(&common::sheet(3), &input);

    Command::cargo_bin("pixa")
        .unwrap()
        .args([
            "split",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
            "--names",
            "a,b,c",
        ])
        .assert()
        .success();

    use image::GenericImageView;
    let a = image::open(output.join("a.png")).unwrap().dimensions();
    let b = image::open(output.join("b.png")).unwrap().dimensions();
    let c = image::open(output.join("c.png")).unwrap().dimensions();
    assert_eq!(a, b);
    assert_eq!(b, c);
}
