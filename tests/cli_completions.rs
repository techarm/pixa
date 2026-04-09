use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn completions_zsh_outputs_valid_script() {
    Command::cargo_bin("pixa")
        .unwrap()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("#compdef pixa"))
        .stdout(predicate::str::contains("compress"))
        .stdout(predicate::str::contains("split"));
}

#[test]
fn completions_bash_outputs_valid_script() {
    Command::cargo_bin("pixa")
        .unwrap()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete"))
        .stdout(predicate::str::contains("pixa"));
}

#[test]
fn completions_fish_outputs_valid_script() {
    Command::cargo_bin("pixa")
        .unwrap()
        .args(["completions", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete"))
        .stdout(predicate::str::contains("pixa"));
}

#[test]
fn completions_invalid_shell_fails() {
    Command::cargo_bin("pixa")
        .unwrap()
        .args(["completions", "notashell"])
        .assert()
        .failure();
}
