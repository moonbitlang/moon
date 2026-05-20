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

#[cfg(target_os = "macos")]
fn xcrun_recording_shim() -> (tempfile::TempDir, String) {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().expect("failed to create temporary directory");
    let shim_path = tmp.path().join("xcrun");
    std::fs::write(
        &shim_path,
        r#"#!/usr/bin/env sh
set -eu

if [ "$1" = "xctrace" ] && [ "$2" = "version" ]; then
  exit 0
fi

if [ "$1" = "xctrace" ] && [ "$2" = "record" ]; then
  shift 2
  trace=""
  stdout=""
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --output)
        shift
        trace="$1"
        ;;
      --target-stdout)
        shift
        stdout="$1"
        ;;
      --)
        shift
        break
        ;;
    esac
    shift
  done
  mkdir -p "$trace"
  mkdir -p "$(dirname "$stdout")"
  "$@" > "$stdout"
  exit $?
fi

if [ "$1" = "xctrace" ] && [ "$2" = "export" ]; then
  shift 2
  output=""
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --output)
        shift
        output="$1"
        ;;
    esac
    shift
  done
  mkdir -p "$(dirname "$output")"
  cat > "$output" <<'XML'
<trace-query-result><node>
<row><thread-state id="1" fmt="Running">Running</thread-state><weight id="2" fmt="1.00 ms">1000000</weight><stack id="3" fmt="_M0foo"><frame id="5" name="_M0foo" addr="0x1"/></stack></row>
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

#[cfg(target_os = "macos")]
#[test]
fn test_moon_test_profile_explains_failures_are_not_reported() {
    let dir = TestDir::new("hello");
    std::fs::write(
        dir.join("main/main.mbt"),
        r#"fn main {
  println("Hello, world!")
}

test {
  inspect("actual", content="expected")
}
"#,
    )
    .unwrap();
    let (_tmp, path) = xcrun_recording_shim();

    let stdout = get_stdout_with_envs(&dir, ["test", "--profile"], [("PATH", path)]);

    assert!(
        stdout.contains("Profile mode does not report test failures"),
        "profile mode should explicitly explain that test failures are not reported:\n{stdout}"
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn test_moon_run_profile_on_non_macos_reports_error() {
    use crate::get_err_stderr;
    let dir = TestDir::new("hello");
    let output = get_err_stderr(&dir, ["run", "main", "--profile"]);

    assert!(
        output.contains("`moon run --profile` currently supports macOS only"),
        "unexpected non-macos error: {output}"
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn test_moon_test_profile_on_non_macos_reports_error() {
    use crate::get_err_stderr;
    let dir = TestDir::new("hello");
    let output = get_err_stderr(&dir, ["test", "--profile"]);

    assert!(
        output.contains("`moon test --profile` currently supports macOS only"),
        "unexpected non-macos error: {output}"
    );
}
