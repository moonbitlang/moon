use crate::{
    TestDir, get_stdout,
    util::{check, moon_bin, replace_dir},
};
use expect_test::{expect, expect_file};

fn z3_path() -> Option<std::path::PathBuf> {
    std::env::var_os("Z3PATH")
        .map(std::path::PathBuf::from)
        .or_else(|| which::which("z3").ok())
}

fn assert_is_file(path: &std::path::Path) {
    assert!(
        path.is_file(),
        "expected artifact to exist: {}",
        path.display()
    );
}

fn assert_stdout_contains_mbtp(
    dir: &TestDir,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
    needle: &str,
    label: &str,
) {
    let stdout = get_stdout(dir, args);
    assert!(
        stdout.contains(needle),
        "{label} output should include `{needle}`, got:\n{stdout}"
    );
}

fn assert_invpred_runtime_commands_succeed(dir: &TestDir) {
    let _ = get_stdout(dir, ["check", "invpred"]);
    let _ = get_stdout(dir, ["build", "invpred"]);
    let _ = get_stdout(dir, ["bench", "-p", "invpred", "--build-only"]);
}

#[test]
fn test_moon_prove_dry_run() {
    let dir = TestDir::new("moon_prove/mixed.in");
    let stdout = get_stdout(&dir, ["prove", "zzok", "--dry-run"]);
    expect_file!["snapshots/zzok.stdout"].assert_eq(&stdout);
}

#[test]
fn test_moon_prove_generates_artifacts() {
    let dir = TestDir::new("moon_prove/mixed.in");
    let z3_path = z3_path();
    let Some(z3_path) = z3_path else {
        eprintln!("skipping moon_prove artifact test: z3 is not available");
        return;
    };

    let output = snapbox::cmd::Command::new(moon_bin())
        .env("Z3PATH", &z3_path)
        .current_dir(&dir)
        .args(["prove", "zzok"])
        .assert()
        .success()
        .get_output()
        .to_owned();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    expect_file!["snapshots/zzok.run.stdout"].assert_eq(&replace_dir(&stdout, &dir));
    expect_file!["snapshots/zzok.stderr"].assert_eq(&replace_dir(&stderr, &dir));

    assert_is_file(&dir.join("_build/verif/zzok/zzok.mlw"));
    assert_is_file(&dir.join("_build/verif/zzok/zzok.proof.json"));
}

#[test]
fn test_moon_prove_mixed_workspace_failure() {
    let dir = TestDir::new("moon_prove/mixed.in");
    let Some(z3_path) = z3_path() else {
        eprintln!("skipping mixed moon_prove test: z3 is not available");
        return;
    };

    let output = snapbox::cmd::Command::new(moon_bin())
        .env("Z3PATH", &z3_path)
        .current_dir(&dir)
        .args(["prove", "-j1"])
        .assert()
        .failure()
        .get_output()
        .to_owned();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    expect_file!["snapshots/mixed.run.stdout"].assert_eq(&replace_dir(&stdout, &dir));
    expect_file!["snapshots/mixed.stderr"].assert_eq(&replace_dir(&stderr, &dir));

    assert_is_file(&dir.join("_build/verif/zzok/zzok.proof.json"));
    assert_is_file(&dir.join("_build/verif/afail/afail.proof.json"));
    assert_is_file(&dir.join("_build/verif/invpred/invpred.proof.json"));
}

#[test]
fn test_moon_prove_selected_failed_package() {
    let dir = TestDir::new("moon_prove/mixed.in");
    let Some(z3_path) = z3_path() else {
        eprintln!("skipping selected failed moon_prove test: z3 is not available");
        return;
    };

    let output = snapbox::cmd::Command::new(moon_bin())
        .env("Z3PATH", &z3_path)
        .current_dir(&dir)
        .args(["prove", "afail", "-j1"])
        .assert()
        .failure()
        .get_output()
        .to_owned();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    expect_file!["snapshots/afail.run.stdout"].assert_eq(&replace_dir(&stdout, &dir));
    expect_file!["snapshots/afail.stderr"].assert_eq(&replace_dir(&stderr, &dir));

    assert_is_file(&dir.join("_build/verif/afail/afail.proof.json"));
}

#[test]
fn test_invpred_package_threads_mbtp_into_compile_dry_runs() {
    let dir = TestDir::new("moon_prove/mixed.in");

    for (label, args) in [
        (
            "check",
            vec!["check", "invpred", "--dry-run", "--sort-input"],
        ),
        (
            "build",
            vec!["build", "invpred", "--dry-run", "--sort-input"],
        ),
        ("test", vec!["test", "invpred", "--dry-run", "--sort-input"]),
        ("prove", vec!["prove", "invpred", "--dry-run"]),
        (
            "bench",
            vec![
                "bench",
                "-p",
                "invpred",
                "--build-only",
                "--dry-run",
                "--sort-input",
            ],
        ),
        ("bundle", vec!["bundle", "--dry-run", "--sort-input"]),
    ] {
        assert_stdout_contains_mbtp(&dir, args, "./invpred/invpred.mbtp", label);
    }
}

#[test]
fn test_invpred_package_runtime_commands_succeed() {
    let dir = TestDir::new("moon_prove/mixed.in");
    assert_invpred_runtime_commands_succeed(&dir);
    check(
        get_stdout(
            &dir,
            ["test", "invpred", "--sort-input", "--no-parallelize"],
        ),
        expect!["Total tests: 1, passed: 1, failed: 0.\n"],
    );
}

#[test]
fn test_invpred_package_prove_succeeds() {
    let dir = TestDir::new("moon_prove/mixed.in");
    let Some(z3_path) = z3_path() else {
        eprintln!("skipping invpred moon_prove test: z3 is not available");
        return;
    };

    assert_invpred_runtime_commands_succeed(&dir);

    let _ = snapbox::cmd::Command::new(moon_bin())
        .env("Z3PATH", &z3_path)
        .current_dir(&dir)
        .args(["prove", "invpred"])
        .assert()
        .success();

    assert_is_file(&dir.join("_build/verif/invpred/invpred.proof.json"));
}
