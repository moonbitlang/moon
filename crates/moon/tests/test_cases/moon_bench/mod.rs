use super::*;

const DIAGNOSTIC_RENDER_WARNING: &str = "Warning: Some diagnostics could not be rendered, please run with --no-render to see raw output.";

fn check_build_only_stderr(stderr: String) {
    let stderr = stderr.trim();
    assert!(
        stderr.is_empty() || stderr == DIAGNOSTIC_RENDER_WARNING,
        "unexpected stderr:\n{stderr}"
    );
}

#[test]
fn test_bench_driver_build() {
    let dir = TestDir::new("moon_bench");
    check_build_only_stderr(get_stderr(&dir, ["bench", "--build-only"]));
}

#[test]
fn test_bench_wasi_link() {
    let dir = TestDir::new("moon_bench");
    let output = get_stdout(
        &dir,
        ["bench", "--target", "wasm", "--build-only", "--dry-run"],
    );
    assert!(output.contains("-wasi"));
}

#[test]
fn test_bench_driver_build_js() {
    let dir = TestDir::new("moon_bench");
    check_build_only_stderr(get_stderr(
        &dir,
        ["bench", "--build-only", "--target", "js"],
    ));
}

#[test]
#[cfg(not(windows))]
fn test_bench_driver_build_native() {
    let dir = TestDir::new("moon_bench");
    check_build_only_stderr(get_stderr(
        &dir,
        ["bench", "--build-only", "--target", "native"],
    ));
}

#[test]
#[cfg(not(windows))]
fn test_bench_displays_nanoseconds() {
    let dir = TestDir::new("moon_bench");
    let moon_home = tempfile::tempdir().expect("failed to create temp MOON_HOME");
    let out = get_stdout_with_envs(
        &dir,
        ["bench", "--target", "native"],
        [("MOON_HOME", moon_home.path().to_str().unwrap())],
    );
    // The no-op bench ("bench: without error") should complete in sub-microsecond
    // time, so auto_select_unit should display it in nanoseconds.
    assert!(
        out.contains(" ns"), // "The space before ns distinguishes the unit in 500.00 ns"
        "expected bench output to contain nanosecond display, got:\n{out}"
    );
}
