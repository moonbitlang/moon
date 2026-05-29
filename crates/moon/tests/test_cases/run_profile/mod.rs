use super::*;

#[cfg(target_os = "macos")]
fn xcrun_version_shim() -> (tempfile::TempDir, String) {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().expect("failed to create temporary directory");
    let shim_path = tmp.path().join("xcrun");
    std::fs::write(
        &shim_path,
        "#!/usr/bin/env sh\nif [ \"$1\" = \"xctrace\" ] && [ \"$2\" = \"version\" ]; then\n  exit 0\nfi\nexit 0\n",
    )
    .unwrap();
    let mut perms = std::fs::metadata(&shim_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&shim_path, perms).unwrap();

    let path = std::env::var("PATH")
        .map(|value| format!("{}:{}", tmp.path().display(), value))
        .unwrap_or_else(|_| tmp.path().display().to_string());
    (tmp, path)
}

#[cfg(target_os = "linux")]
fn perf_path_shim() -> (tempfile::TempDir, String) {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().expect("failed to create temporary directory");
    let shim_path = tmp.path().join("perf");
    std::fs::write(&shim_path, "#!/usr/bin/env sh\nexit 0\n").unwrap();
    let mut perms = std::fs::metadata(&shim_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&shim_path, perms).unwrap();

    let path = std::env::var("PATH")
        .map(|value| format!("{}:{}", tmp.path().display(), value))
        .unwrap_or_else(|_| tmp.path().display().to_string());
    (tmp, path)
}

#[cfg(target_os = "macos")]
#[test]
fn test_moon_run_profile_dry_run_prints_xctrace_commands() {
    use crate::dry_run_utils::line_with;

    let dir = TestDir::new("hello");
    let (_tmp, path) = xcrun_version_shim();

    let output = get_stdout_with_envs(
        &dir,
        ["run", "main", "--profile", "--dry-run"],
        [("PATH", path)],
    );

    let record_cmd = line_with(
        &output,
        "xcrun xctrace record",
        &[
            "--quiet",
            "--template",
            "Time",
            "Profiler",
            "--no-prompt",
            "--output",
        ],
    );
    let export_cmd = line_with(&output, "xcrun xctrace export", &["--quiet", "--input"]);

    assert!(
        record_cmd.contains("--target-stdout"),
        "record command missing --target-stdout: {record_cmd}"
    );
    assert!(
        record_cmd.contains("--launch"),
        "record command missing --launch: {record_cmd}"
    );
    assert!(
        record_cmd.contains("_build/native/release/profile/main"),
        "record command missing profile output path: {record_cmd}"
    );
    assert!(
        export_cmd.contains("time-profile.xml"),
        "export command missing time-profile.xml output: {export_cmd}"
    );
    assert!(
        !output.lines().any(|line| line
            .trim_start()
            .starts_with("./_build/native/release/build/main/main.exe")),
        "profile dry-run should not print the standalone executable invocation:\n{output}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_moon_test_profile_dry_run_uses_native_release_graph() {
    let dir = TestDir::new("hello");
    let (_tmp, path) = xcrun_version_shim();
    let output = get_stdout_with_envs(&dir, ["test", "--profile", "--dry-run"], [("PATH", path)]);

    assert!(
        output.contains("_build/native/release/test"),
        "profile dry-run should use the native release test graph:\n{output}"
    );
    assert!(
        !output.contains("xcrun xctrace"),
        "test profile dry-run should not print xctrace commands without generated test metadata:\n{output}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn test_moon_run_profile_dry_run_prints_perf_commands() {
    use crate::dry_run_utils::line_with;

    let dir = TestDir::new("hello");
    let (_tmp, path) = perf_path_shim();

    let output = get_stdout_with_envs(
        &dir,
        ["run", "main", "--profile", "--dry-run"],
        [("PATH", path)],
    );

    let record_cmd = line_with(
        &output,
        "perf record",
        &[
            "--quiet",
            "-F",
            "100",
            "-e",
            "cpu-clock",
            "--call-graph",
            "dwarf",
            "-o",
        ],
    );
    let script_cmd = line_with(&output, "perf script", &["-i"]);

    assert!(
        record_cmd.contains("perf.data"),
        "record command missing perf data output path: {record_cmd}"
    );
    assert!(
        record_cmd.contains("stdout.txt"),
        "record command missing stdout redirection: {record_cmd}"
    );
    assert!(
        record_cmd.contains("stderr.txt"),
        "record command missing stderr redirection: {record_cmd}"
    );
    assert!(
        script_cmd.contains("perf-script.txt"),
        "script command missing exported script output: {script_cmd}"
    );
    assert!(
        !output.lines().any(|line| line
            .trim_start()
            .starts_with("./_build/native/release/build/main/main.exe")),
        "profile dry-run should not print the standalone executable invocation:\n{output}"
    );
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
#[test]
fn test_moon_run_profile_on_non_macos_reports_error() {
    use crate::get_err_stderr;
    let dir = TestDir::new("hello");
    let output = get_err_stderr(&dir, ["run", "main", "--profile"]);

    assert!(
        output.contains("`moon run --profile` currently supports macOS and Linux only"),
        "unexpected unsupported-platform error: {output}"
    );
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
#[test]
fn test_moon_test_profile_on_non_macos_reports_error() {
    use crate::get_err_stderr;
    let dir = TestDir::new("hello");
    let output = get_err_stderr(&dir, ["test", "--profile"]);

    assert!(
        output.contains("`moon test --profile` currently supports macOS and Linux only"),
        "unexpected unsupported-platform error: {output}"
    );
}
