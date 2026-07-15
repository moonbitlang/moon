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
fn profile_cwd_shim() -> (tempfile::TempDir, String) {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().expect("failed to create temporary directory");
    let shim_path = tmp.path().join("xcrun");
    std::fs::write(
        &shim_path,
        r#"#!/usr/bin/env sh
if [ "$1" = "xctrace" ] && [ "$2" = "version" ]; then
  exit 0
fi

if [ "$1" = "xctrace" ] && [ "$2" = "record" ]; then
  pwd > "$MOON_PROFILE_CWD_MARKER"
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--" ]; then
      shift
      "$@" >/dev/null 2>/dev/null
      exit $?
    fi
    shift
  done
  exit 0
fi

if [ "$1" = "xctrace" ] && [ "$2" = "export" ]; then
  output=""
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--output" ]; then
      shift
      output="$1"
    fi
    shift
  done
  cat > "$output" <<'XML'
<trace-query-result><node>
<row><thread-state id="1" fmt="Running">Running</thread-state><weight id="2" fmt="1.00 ms">1000000</weight><stack id="3" fmt="_M0profile"><frame id="4" name="_M0profile" addr="0x1"/></stack></row>
</node></trace-query-result>
XML
  exit 0
fi

exit 1
"#,
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
fn profile_cwd_shim() -> (tempfile::TempDir, String) {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().expect("failed to create temporary directory");
    let shim_path = tmp.path().join("perf");
    std::fs::write(
        &shim_path,
        r#"#!/usr/bin/env sh
if [ "$1" = "record" ]; then
  pwd > "$MOON_PROFILE_CWD_MARKER"
  data_path=""
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "-o" ]; then
      shift
      data_path="$1"
    elif [ "$1" = "--" ]; then
      shift
      "$@" >/dev/null 2>/dev/null
      status=$?
      : > "$data_path"
      exit $status
    fi
    shift
  done
  : > "$data_path"
  exit 0
fi

if [ "$1" = "script" ]; then
  cat <<'PERF'
            profile 1234 [001] 10.000000: cpu-clock:
        400000000001 _M0profile+0x14 (/tmp/main.exe)
PERF
  exit 0
fi

exit 1
"#,
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

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn test_moon_test_profile_runs_profiler_from_module_root() {
    let dir = TestDir::new("run_profile/profile_test_cwd");
    let spawn_dir = dir.join("spawn");
    std::fs::create_dir(&spawn_dir).expect("failed to create spawn directory");
    let marker = dir.join("profile-cwd.txt");
    let (_tmp, path) = profile_cwd_shim();

    let output = moon_process_cmd(&spawn_dir)
        .env("PATH", path)
        .env("MOON_PROFILE_CWD_MARKER", &marker)
        .args(["-C", "..", "test", "--profile"])
        .output()
        .expect("failed to run profiled tests");
    assert!(
        output.status.success(),
        "profiled tests failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let recorded = std::fs::read_to_string(&marker).expect("failed to read profiler cwd marker");
    let recorded = std::fs::canonicalize(recorded.trim()).expect("failed to canonicalize marker");
    let expected = std::fs::canonicalize(dir.as_ref()).expect("failed to canonicalize test dir");
    assert_eq!(recorded, expected);
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
