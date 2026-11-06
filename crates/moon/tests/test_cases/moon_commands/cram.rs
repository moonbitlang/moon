use std::path::PathBuf;

use super::path_with_fake_bin;
use crate::{TestDir, get_err_stderr_with_envs, get_stdout, get_stdout_with_envs, moon_cmd};

#[test]
fn test_cram_dry_run_builds_native_release_and_prints_delegation() {
    let dir = TestDir::new("hello");
    let stdout = get_stdout(
        &dir,
        ["--dry-run", "cram", "test", "--release", "--", "--list"],
    );

    assert!(
        stdout.contains("./_build/native/release/build/main/main.exe"),
        "dry-run should build the native release executable:\n{stdout}"
    );

    let cram_line = stdout.lines().last().unwrap_or_default();
    let path_separator = if cfg!(windows) { ";" } else { ":" };
    assert!(
        cram_line.contains(&format!(
            "'PATH=./_build/native/release/build/main{path_separator}$PATH'"
        )),
        "dry-run should print moon-cram with the computed PATH:\n{stdout}"
    );
    assert!(
        cram_line.contains("moon-cram") && cram_line.ends_with(" test --list"),
        "dry-run should print moon-cram with forwarded args:\n{stdout}"
    );
}

#[test]
fn test_cram_delegates_with_built_binary_dirs_on_path() {
    let dir = TestDir::new("hello");
    let fake_bin_dir = tempfile::TempDir::new().expect("failed to create fake bin dir");
    let fake_cram = compile_fake_cram_fixture(fake_bin_dir.path());

    let stdout = get_stdout_with_envs(
        &dir,
        ["cram", "test", "--shell", "bash"],
        [(
            "MOON_CRAM_OVERRIDE",
            fake_cram.to_string_lossy().into_owned(),
        )],
    );

    assert!(
        stdout.contains("fake-moon-cram-args=test|--shell|bash"),
        "moon-cram should receive passthrough args:\n{stdout}"
    );
    assert!(
        stdout.contains("fake-moon-cram-path=$ROOT/_build/native/debug/build/main"),
        "moon-cram PATH should include the built executable directory:\n{stdout}"
    );
}

#[test]
fn test_cram_flushes_trace_before_delegation() {
    let dir = TestDir::new("hello");
    let fake_bin_dir = tempfile::TempDir::new().expect("failed to create fake bin dir");
    let fake_cram = compile_fake_cram_fixture(fake_bin_dir.path());

    moon_cmd(&dir)
        .env("MOON_CRAM_OVERRIDE", fake_cram)
        .args(["--trace", "cram", "test", "--shell", "bash"])
        .assert()
        .success();

    let trace = std::fs::read_to_string(dir.join("trace.json")).unwrap();
    let events: serde_json::Value = serde_json::from_str(&trace).unwrap();
    assert!(events.as_array().is_some_and(|events| !events.is_empty()));
}

#[test]
fn test_cram_makes_built_executable_available_on_path() {
    let dir = TestDir::new("cram");

    let stdout = get_stdout(&dir, ["cram", "test", "tests/cram"]);

    assert!(
        stdout.contains("Result: 1 document(s) with 1 testcase(s): 1 succeeded"),
        "moon-cram should run the cram test that invokes the built executable by name:\n{stdout}"
    );
}

#[test]
fn test_cram_test_help_shows_wrapper_options() {
    let dir = TestDir::new_empty();

    let stdout = get_stdout(&dir, ["cram", "test", "--help"]);

    assert!(
        stdout.contains("--release") && stdout.contains("--target <TARGET>"),
        "moon cram test help should show Moon-owned wrapper options:\n{stdout}"
    );
}

#[test]
fn test_cram_parent_help_shows_wrapper_subcommands() {
    let dir = TestDir::new_empty();

    let stdout = get_stdout(&dir, ["cram", "--help"]);

    assert!(
        stdout.contains("test") && stdout.contains("Build native executables"),
        "moon cram help should show Moon-owned wrapper subcommands:\n{stdout}"
    );
}

#[test]
fn test_cram_parent_flag_delegates_to_moon_cram() {
    let dir = TestDir::new_empty();
    let fake_bin_dir = tempfile::TempDir::new().expect("failed to create fake bin dir");
    let fake_cram = compile_fake_cram_fixture(fake_bin_dir.path());

    let stdout = get_stdout_with_envs(
        &dir,
        ["cram", "--version"],
        [(
            "MOON_CRAM_OVERRIDE",
            fake_cram.to_string_lossy().into_owned(),
        )],
    );

    assert!(
        stdout.contains("fake-moon-cram-args=--version"),
        "moon cram parent flags should be handled by moon-cram:\n{stdout}"
    );
}

#[test]
fn test_cram_bare_override_resolves_from_path() {
    let dir = TestDir::new_empty();
    let fake_bin_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/executable-override/bin");
    let fake_cram = fake_bin_dir.join(if cfg!(windows) {
        "fake-moon-cram.cmd"
    } else {
        "fake-moon-cram"
    });

    let stdout = get_stdout_with_envs(
        &dir,
        ["cram", "--version"],
        [
            (
                std::ffi::OsString::from("MOON_CRAM_OVERRIDE"),
                std::ffi::OsString::from("fake-moon-cram"),
            ),
            (
                std::ffi::OsString::from("PATH"),
                path_with_fake_bin(&fake_bin_dir),
            ),
        ],
    );

    let normalized_stdout = stdout.replace('\\', "/");
    let normalized_executable = fake_cram.to_string_lossy().replace('\\', "/");
    assert!(
        normalized_stdout.contains(&format!("fake-moon-cram-argv0={normalized_executable}")),
        "bare moon-cram override should resolve to an explicit path:\n{stdout}"
    );
}

#[test]
fn test_cram_relative_override_resolves_after_change_directory() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cram-override-cwd");

    let early_stdout = get_stdout_with_envs(
        &dir,
        ["-C", "subdir", "cram", "--version"],
        [("MOON_CRAM_OVERRIDE", "bin/fake-moon-cram")],
    );
    assert!(
        early_stdout.contains("fake-moon-cram-location=subdir"),
        "early moon-cram delegation should resolve the override from the -C directory:\n{early_stdout}"
    );

    let parsed_stdout = get_stdout_with_envs(
        &dir,
        ["-C", "subdir", "cram", "list"],
        [("MOON_CRAM_OVERRIDE", "bin/fake-moon-cram")],
    );
    assert!(
        parsed_stdout.contains("fake-moon-cram-location=subdir"),
        "parsed moon-cram delegation should resolve the override from the -C directory:\n{parsed_stdout}"
    );
}

#[test]
fn test_cram_bare_override_on_relative_path_resolves_after_change_directory() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cram-override-cwd");
    let path = std::env::join_paths(std::iter::once(PathBuf::from("bin")).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .expect("failed to build relative PATH");

    let stdout = get_stdout_with_envs(
        &dir,
        ["-C", "subdir", "cram", "--version"],
        [
            (
                std::ffi::OsString::from("MOON_CRAM_OVERRIDE"),
                std::ffi::OsString::from("fake-moon-cram"),
            ),
            (std::ffi::OsString::from("PATH"), path),
        ],
    );

    assert!(
        stdout.contains("fake-moon-cram-location=subdir"),
        "bare moon-cram override on a relative PATH should resolve from the -C directory:\n{stdout}"
    );
}

#[test]
fn test_cram_parent_flag_after_moon_global_delegates_to_moon_cram() {
    let dir = TestDir::new_empty();
    let fake_bin_dir = tempfile::TempDir::new().expect("failed to create fake bin dir");
    let fake_cram = compile_fake_cram_fixture(fake_bin_dir.path());

    let stdout = get_stdout_with_envs(
        &dir,
        ["-q", "cram", "--version"],
        [(
            "MOON_CRAM_OVERRIDE",
            fake_cram.to_string_lossy().into_owned(),
        )],
    );

    assert!(
        stdout.contains("fake-moon-cram-args=--version"),
        "moon cram parent flags should still delegate after Moon globals:\n{stdout}"
    );
}

#[test]
fn test_cram_trace_ownership_follows_delegation_point() {
    let dir = TestDir::new_empty();
    let fake_bin_dir = tempfile::TempDir::new().expect("failed to create fake bin dir");
    let fake_cram = compile_fake_cram_fixture(fake_bin_dir.path());

    let stderr = get_err_stderr_with_envs(
        &dir,
        ["--trace", "cram", "--version"],
        [(
            "MOON_CRAM_OVERRIDE",
            fake_cram.to_string_lossy().into_owned(),
        )],
    );
    assert!(
        stderr.contains("`--trace` is not supported before delegated command `cram`"),
        "Moon-owned trace should be rejected for transparent delegation:\n{stderr}"
    );
    assert!(!dir.join("trace.json").exists());

    let stdout = get_stdout_with_envs(
        &dir,
        ["cram", "--trace"],
        [(
            "MOON_CRAM_OVERRIDE",
            fake_cram.to_string_lossy().into_owned(),
        )],
    );
    assert!(
        stdout.contains("fake-moon-cram-args=--trace"),
        "trace after the delegation point should be forwarded:\n{stdout}"
    );
}

#[test]
fn test_cram_non_test_subcommand_delegates_to_moon_cram() {
    let dir = TestDir::new_empty();
    let fake_bin_dir = tempfile::TempDir::new().expect("failed to create fake bin dir");
    let fake_cram = compile_fake_cram_fixture(fake_bin_dir.path());

    let stdout = get_stdout_with_envs(
        &dir,
        ["cram", "list", "--json"],
        [(
            "MOON_CRAM_OVERRIDE",
            fake_cram.to_string_lossy().into_owned(),
        )],
    );

    assert!(
        stdout.contains("fake-moon-cram-args=list|--json"),
        "non-test cram invocations should be handled by moon-cram:\n{stdout}"
    );
}

#[test]
fn test_cram_build_path_error_does_not_delegate_to_moon_cram() {
    let dir = TestDir::new_empty();
    let fake_bin_dir = tempfile::TempDir::new().expect("failed to create fake bin dir");
    let fake_cram = compile_fake_cram_fixture(fake_bin_dir.path());

    let stderr = get_err_stderr_with_envs(
        &dir,
        ["build", "cram", "--bad-flag"],
        [(
            "MOON_CRAM_OVERRIDE",
            fake_cram.to_string_lossy().into_owned(),
        )],
    );

    assert!(
        stderr.contains("unexpected argument '--bad-flag'"),
        "cram path under moon build should stay a build parse error:\n{stderr}"
    );
}

#[test]
fn test_cram_rejects_unresolved_runner_before_build_path() {
    let dir = TestDir::new_empty();
    std::fs::write(
        dir.join("moon.mod"),
        r#"
name = "username/cram-shadow"
"#,
    )
    .expect("failed to write module manifest");
    std::fs::create_dir_all(dir.join("moon-cram")).expect("failed to create main package");
    std::fs::write(
        dir.join("moon-cram/moon.pkg"),
        r#"
pkgtype(kind: "executable")
"#,
    )
    .expect("failed to write package manifest");
    std::fs::write(
        dir.join("moon-cram/main.mbt"),
        "fn main { println(\"shadowed\") }\n",
    )
    .expect("failed to write main source");

    let empty_path = tempfile::TempDir::new().expect("failed to create empty PATH dir");
    let stderr = get_err_stderr_with_envs(
        &dir,
        ["cram", "test"],
        [
            ("MOON_CRAM_OVERRIDE".to_string(), "moon-cram".to_string()),
            (
                "PATH".to_string(),
                empty_path.path().to_string_lossy().into_owned(),
            ),
        ],
    );

    assert!(
        stderr.contains("no such subcommand: `cram`"),
        "unresolved moon-cram should fail before built executable dirs enter PATH:\n{stderr}"
    );
}

#[test]
fn test_cram_build_failure_prevents_delegation() {
    let dir = TestDir::new_empty();
    std::fs::write(
        dir.join("moon.mod"),
        r#"
name = "username/fail"
"#,
    )
    .expect("failed to write module manifest");
    std::fs::create_dir_all(dir.join("main")).expect("failed to create main package");
    std::fs::write(
        dir.join("main/moon.pkg"),
        r#"
pkgtype(kind: "executable")
"#,
    )
    .expect("failed to write package manifest");
    std::fs::write(dir.join("main/main.mbt"), "fn main { missing }\n")
        .expect("failed to write invalid source");

    let fake_bin_dir = tempfile::TempDir::new().expect("failed to create fake bin dir");
    let fake_cram = compile_fake_cram_fixture(fake_bin_dir.path());
    let marker = fake_bin_dir.path().join("moon-cram-ran");

    let _stderr = get_err_stderr_with_envs(
        &dir,
        ["cram", "test"],
        [
            (
                "MOON_CRAM_OVERRIDE",
                fake_cram.to_string_lossy().into_owned(),
            ),
            (
                "FAKE_MOON_CRAM_MARKER",
                marker.to_string_lossy().into_owned(),
            ),
        ],
    );

    assert!(
        !marker.exists(),
        "moon-cram should not run after build failure"
    );
}

fn compile_fake_cram_fixture(fake_bin_dir: &std::path::Path) -> PathBuf {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_moon_cram.rs");
    let executable = fake_bin_dir.join(format!("fake-moon-cram{}", std::env::consts::EXE_SUFFIX));
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = std::process::Command::new(rustc)
        .arg("--edition=2021")
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .status()
        .expect("failed to compile fake moon-cram");
    assert!(status.success(), "failed to compile fake moon-cram");
    executable
}
