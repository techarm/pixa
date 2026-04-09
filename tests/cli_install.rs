use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn install_skills_writes_skill_md() {
    let fake_home = TempDir::new().unwrap();

    Command::cargo_bin("pixa")
        .unwrap()
        .env("HOME", fake_home.path())
        .env("USERPROFILE", fake_home.path())
        .args(["install", "--skills"])
        .assert()
        .success()
        .stdout(predicate::str::contains("skill installed"));

    let skill = fake_home.path().join(".claude/skills/pixa/SKILL.md");
    assert!(skill.exists(), "{} should exist", skill.display());

    let content = std::fs::read_to_string(&skill).unwrap();
    assert!(content.contains("name: pixa"));
    assert!(content.contains("description:"));
}

#[test]
fn install_skills_refuses_to_overwrite_without_force() {
    let fake_home = TempDir::new().unwrap();

    // first install succeeds
    Command::cargo_bin("pixa")
        .unwrap()
        .env("HOME", fake_home.path())
        .env("USERPROFILE", fake_home.path())
        .args(["install", "--skills"])
        .assert()
        .success();

    // second install without --force: still exits 0 but warns
    Command::cargo_bin("pixa")
        .unwrap()
        .env("HOME", fake_home.path())
        .env("USERPROFILE", fake_home.path())
        .args(["install", "--skills"])
        .assert()
        .success()
        .stderr(predicate::str::contains("already installed"));
}

#[test]
fn install_skills_force_overwrites() {
    let fake_home = TempDir::new().unwrap();

    Command::cargo_bin("pixa")
        .unwrap()
        .env("HOME", fake_home.path())
        .env("USERPROFILE", fake_home.path())
        .args(["install", "--skills"])
        .assert()
        .success();

    Command::cargo_bin("pixa")
        .unwrap()
        .env("HOME", fake_home.path())
        .env("USERPROFILE", fake_home.path())
        .args(["install", "--skills", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains("skill installed"));
}

#[test]
fn install_without_flag_fails() {
    Command::cargo_bin("pixa")
        .unwrap()
        .arg("install")
        .assert()
        .failure();
}
