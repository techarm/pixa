mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn favicon_generates_full_icon_set() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("logo.png");
    let output = dir.path().join("favicon");
    common::write_image(&common::gradient_rgb(512, 512), &input);

    Command::cargo_bin("pixa")
        .unwrap()
        .args([
            "favicon",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("HTML snippet"));

    for name in [
        "favicon.ico",
        "favicon-16x16.png",
        "favicon-32x32.png",
        "apple-touch-icon.png",
        "android-chrome-192x192.png",
        "android-chrome-512x512.png",
    ] {
        assert!(
            output.join(name).exists(),
            "expected favicon asset {name} to be created"
        );
    }
}

#[test]
fn favicon_rejects_tiny_image() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("tiny.png");
    let output = dir.path().join("out");
    common::write_image(&common::gradient_rgb(8, 8), &input);

    Command::cargo_bin("pixa")
        .unwrap()
        .args([
            "favicon",
            input.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .failure();
}
