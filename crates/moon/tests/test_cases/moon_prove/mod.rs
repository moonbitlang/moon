use crate::{
    TestDir, assert_success, get_stderr, get_stdout,
    util::{check, moon_bin, replace_dir, toolchain_root_for_tests},
};
use expect_test::{expect, expect_file};

fn verification_tests_enabled() -> bool {
    std::env::var_os("VERIFICATION_TESTS").is_some()
}

fn skip_unless_verification_tests_enabled(name: &str) -> bool {
    if verification_tests_enabled() {
        return false;
    }

    eprintln!("skipping {name}: set VERIFICATION_TESTS=1 to enable verification tests");
    true
}

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

fn assert_stdout_contains_prelude_proof(stdout: &str, label: &str) {
    let prelude_proof = toolchain_root_for_tests().join("lib").join("prelude_proof");
    if !prelude_proof.is_dir() {
        return;
    }

    assert!(
        stdout.contains("prelude_proof"),
        "{label} output should include `prelude_proof`, got:\n{stdout}"
    );
}

fn assert_invpred_runtime_commands_succeed(dir: &TestDir) {
    let _ = get_stdout(dir, ["check", "invpred"]);
    let _ = get_stdout(dir, ["build", "invpred"]);
    let _ = get_stdout(dir, ["bench", "-p", "invpred", "--build-only"]);
}

#[test]
fn test_moon_prove_dry_run() {
    if skip_unless_verification_tests_enabled("test_moon_prove_dry_run") {
        return;
    }
    let dir = TestDir::new("moon_prove/mixed.in");
    let stdout = get_stdout(&dir, ["prove", "zzok", "--dry-run"]);
    expect_file!["snapshots/zzok.stdout"].assert_eq(&stdout);
    assert_stdout_contains_prelude_proof(&stdout, "dry-run");
}

#[test]
fn test_moon_prove_dry_run_uses_user_supplied_why3_config() {
    if skip_unless_verification_tests_enabled(
        "test_moon_prove_dry_run_uses_user_supplied_why3_config",
    ) {
        return;
    }
    let dir = TestDir::new("moon_prove/mixed.in");
    let stdout = get_stdout(
        &dir,
        [
            "prove",
            "zzok",
            "--why3-config",
            "custom-why3.conf",
            "--dry-run",
        ],
    );
    expect_file!["snapshots/zzok.custom-why3-config.stdout"].assert_eq(&stdout);
    assert!(
        !stdout.contains("./_build/verif/why3.conf"),
        "dry-run should not use generated why3.conf when --why3-config is set, got:\n{stdout}"
    );
}

#[test]
fn test_check_doctest_with_mbtp_uses_imported_proof_api() {
    let dir = TestDir::new("moon_prove/doctest_with_mbtp.in");

    assert_success(&dir, ["check"]);
    assert_is_file(&dir.join("_build/wasm-gc/debug/check/lib/lib.mi"));
    assert_is_file(&dir.join("_build/wasm-gc/debug/check/lib/lib.blackbox_test.mi"));

    let stdout = get_stdout(&dir, ["check", "--dry-run"]);
    expect_file!["snapshots/doctest_with_mbtp.check.stdout"].assert_eq(&stdout);
}

#[test]
fn test_packages_json_includes_mbtp_files() {
    let dir = TestDir::new("moon_prove/doctest_with_mbtp.in");

    let _ = get_stderr(&dir, ["check"]);

    let packages_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("_build/packages.json")).unwrap())
            .unwrap();
    let packages = packages_json["packages"].as_array().unwrap();
    let package = packages
        .iter()
        .find(|pkg| pkg["rel"] == "lib")
        .expect("expected lib package in packages.json");

    let files = package["files"]
        .as_object()
        .expect("expected files object in packages.json");
    let expected_suffix = std::path::Path::new("lib").join("hello.mbtp");
    assert!(
        files
            .keys()
            .any(|path| { std::path::Path::new(path).ends_with(expected_suffix.as_path()) }),
        "expected packages.json files to include lib/hello.mbtp, got:\n{}",
        serde_json::to_string_pretty(package).unwrap()
    );
}

#[test]
fn test_moon_prove_skips_packages_without_proof_enabled() {
    let dir = TestDir::new("moon_prove/selective.in");
    let stdout = get_stdout(&dir, ["prove", "--dry-run"]);
    expect_file!["snapshots/selective.stdout"].assert_eq(&stdout);
    assert!(
        !stdout.contains("./disabled/disabled.mbt"),
        "packages without proof-enabled should be skipped by moon prove, got:\n{stdout}"
    );
}

#[test]
fn test_moon_prove_warns_for_explicit_package_without_proof_enabled() {
    let dir = TestDir::new("moon_prove/selective.in");
    let stderr = get_stderr(&dir, ["prove", "disabled", "--dry-run"]);
    assert!(
        stderr.contains(
            "Package `username/prove_selective/disabled` selected by `disabled` is not proof-enabled; skipping `moon prove` for it."
        ),
        "unexpected stderr:\n{stderr}",
    );
}

#[test]
fn test_proof_enabled_suppresses_proof_warnings_for_test_runs() {
    let dir = TestDir::new("moon_prove/warn_suppression.in");

    let dry_run = get_stdout(&dir, ["test", "lib", "--dry-run", "--sort-input"]);
    expect_file!["snapshots/warn_suppression.test.stdout"].assert_eq(&dry_run);
    assert!(
        dry_run.contains("-w -1-2-3-29"),
        "proof-enabled packages should pass proof warning suppressions, got:\n{dry_run}"
    );

    let output = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test", "lib", "--sort-input", "--no-parallelize"])
        .assert()
        .success()
        .get_output()
        .to_owned();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    check(
        replace_dir(&stdout, &dir),
        expect!["Total tests: 1, passed: 1, failed: 0.\n"],
    );
    assert!(
        !stderr.contains("Warning: [0001]")
            && !stderr.contains("Warning: [0002]")
            && !stderr.contains("Warning: [0003]")
            && !stderr.contains("Warning: [0029]"),
        "proof-enabled packages should suppress warnings 1, 2, 3, and 29, got:\n{}",
        replace_dir(&stderr, &dir)
    );
}

#[test]
fn test_moon_prove_generates_artifacts() {
    if skip_unless_verification_tests_enabled("test_moon_prove_generates_artifacts") {
        return;
    }
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
    snapbox::assert_data_eq!(
        replace_dir(&stdout, &dir).trim_end(),
        snapbox::str![[r#"
username/prove/zzok
  Succeeded: [..] goals proved

Summary:
  1 of 1 packages proved
  [..] goals proved"#]],
    );

    assert_is_file(&dir.join("_build/verif/zzok/pkg_8_username_5_prove_4_zzok.mlw"));
    assert_is_file(&dir.join("_build/verif/zzok/zzok.proof.json"));
}

#[test]
fn test_moon_prove_mixed_workspace_failure() {
    if skip_unless_verification_tests_enabled("test_moon_prove_mixed_workspace_failure") {
        return;
    }
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
    snapbox::assert_data_eq!(
        replace_dir(&stdout, &dir).trim_end(),
        snapbox::str![[r#"
failed: moonc prove -error-format json $ROOT/afail/afail.mbt $ROOT/afail/afail.mbtp -whyml-output-path $ROOT/_build/verif/afail/pkg_8_username_5_prove_5_afail.mlw -proof-report-output-path $ROOT/_build/verif/afail/afail.proof.json -why3-config $ROOT/_build/verif/why3.conf -why3-loadpath $MOON_HOME/lib/prelude_proof -pkg username/prove/afail -i $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude -pkg-sources username/prove/afail:$ROOT/afail -workspace-path $ROOT -all-pkgs $ROOT/_build/wasm-gc/debug/prove/all_pkgs.json
username/prove/afail
  Failed: [..]
  WhyML: _build/verif/afail/pkg_8_username_5_prove_5_afail.mlw
  Report: _build/verif/afail/afail.proof.json
  Failed goals: [..]

username/prove/invpred
  Succeeded: [..] goals proved

username/prove/zzok
  Succeeded: [..] goals proved

Summary:
  2 of 3 packages proved
  [..]"#]],
    );

    assert_is_file(&dir.join("_build/verif/zzok/zzok.proof.json"));
    assert_is_file(&dir.join("_build/verif/afail/afail.proof.json"));
    assert_is_file(&dir.join("_build/verif/invpred/invpred.proof.json"));
}

#[test]
fn test_moon_prove_selected_failed_package() {
    if skip_unless_verification_tests_enabled("test_moon_prove_selected_failed_package") {
        return;
    }
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
    expect_file!["snapshots/afail.run.stdout"].assert_eq(&replace_dir(&stdout, &dir));

    assert_is_file(&dir.join("_build/verif/afail/afail.proof.json"));
}

#[test]
fn test_invpred_package_threads_mbtp_into_compile_dry_runs() {
    if skip_unless_verification_tests_enabled(
        "test_invpred_package_threads_mbtp_into_compile_dry_runs",
    ) {
        return;
    }
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
    if skip_unless_verification_tests_enabled("test_invpred_package_runtime_commands_succeed") {
        return;
    }
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
    if skip_unless_verification_tests_enabled("test_invpred_package_prove_succeeds") {
        return;
    }
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

#[test]
fn test_cross_package_prove_dry_run() {
    if skip_unless_verification_tests_enabled("test_cross_package_prove_dry_run") {
        return;
    }
    let dir = TestDir::new("moon_prove/cross_package.in");
    let stdout = get_stdout(&dir, ["prove", "downstream", "--dry-run"]);
    expect_file!["snapshots/cross_package.stdout"].assert_eq(&stdout);
}

#[test]
fn test_cross_package_prove_workspace_failure() {
    if skip_unless_verification_tests_enabled("test_cross_package_prove_workspace_failure") {
        return;
    }
    let dir = TestDir::new("moon_prove/cross_package.in");
    let Some(z3_path) = z3_path() else {
        eprintln!("skipping cross-package workspace moon_prove test: z3 is not available");
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
    expect_file!["snapshots/cross_package.run.stdout"].assert_eq(&replace_dir(&stdout, &dir));
}

#[test]
fn test_cross_package_prove_selected_package_succeeds() {
    if skip_unless_verification_tests_enabled("test_cross_package_prove_selected_package_succeeds")
    {
        return;
    }
    let dir = TestDir::new("moon_prove/cross_package.in");
    let Some(z3_path) = z3_path() else {
        eprintln!("skipping cross-package selected moon_prove test: z3 is not available");
        return;
    };

    let output = snapbox::cmd::Command::new(moon_bin())
        .env("Z3PATH", &z3_path)
        .current_dir(&dir)
        .args(["prove", "downstream", "-j1"])
        .assert()
        .success()
        .get_output()
        .to_owned();
    let stdout = String::from_utf8(output.stdout).unwrap();
    expect_file!["snapshots/cross_package.downstream.run.stdout"]
        .assert_eq(&replace_dir(&stdout, &dir));
}
