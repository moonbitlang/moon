use super::*;

#[test]
fn test_bench_driver_build() {
    let dir = TestDir::new("moon_bench");
    check(get_stderr(&dir, ["bench", "--build-only"]), expect![""]);
}

#[test]
fn test_bench_auto_export_memory() {
    let dir = TestDir::new("moon_bench");
    let output = get_stdout(
        &dir,
        [
            "--unstable-feature",
            "wasi_auto_export_memory",
            "bench",
            "--build-only",
            "--dry-run",
        ],
    );
    assert!(output.contains("-export-memory-name memory"));
}

#[test]
fn test_bench_driver_build_js() {
    let dir = TestDir::new("moon_bench");
    check(
        get_stderr(&dir, ["bench", "--build-only", "--target", "js"]),
        expect![""],
    );
}

#[test]
#[cfg(not(windows))]
fn test_bench_driver_build_native() {
    let dir = TestDir::new("moon_bench");
    check(
        get_stderr(&dir, ["bench", "--build-only", "--target", "native"]),
        expect![""],
    );
}
