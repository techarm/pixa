mod common;

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn info_prints_dimensions_and_format() {
    let (_dir, path) = common::tmp_png(128, 64);

    Command::cargo_bin("pixa")
        .unwrap()
        .args(["info", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("128"))
        .stdout(predicate::str::contains("64"))
        .stdout(predicate::str::contains("PNG"));
}

#[test]
fn info_json_output_is_valid_json() {
    let (_dir, path) = common::tmp_png(100, 50);

    let output = Command::cargo_bin("pixa")
        .unwrap()
        .args(["info", path.to_str().unwrap(), "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let s = std::str::from_utf8(&output).expect("utf-8");
    let value: serde_json::Value = serde_json::from_str(s).expect("valid JSON");
    assert_eq!(value["width"], 100);
    assert_eq!(value["height"], 50);
    assert_eq!(value["format"], "PNG");
}

#[test]
fn info_missing_file_exits_non_zero() {
    Command::cargo_bin("pixa")
        .unwrap()
        .args(["info", "/nonexistent/path/never.png"])
        .assert()
        .failure();
}
