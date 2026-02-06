use super::*;

#[test]
fn test_bench_driver_build() {
    let dir = TestDir::new("moon_bench");
    check(get_stderr(&dir, ["bench", "--build-only"]), expect![""]);
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
#[ignore = "need to fix moonc to not generate C code with warnings"]
fn test_bench_driver_build_native() {
    let dir = TestDir::new("moon_bench");
    check(
        get_stderr(&dir, ["bench", "--build-only", "--target", "native"]),
        expect![""],
    );
}

#[test]
fn test_bench_uses_release_mode_by_default() {
    let dir = TestDir::new("moon_bench");

    // Release by default
    let dry_run = get_stdout(&dir, ["bench", "--dry-run"]);
    println!("bench + dry-run output:\n{}", dry_run);
    assert!(
        dry_run.contains("moonc"),
        "Ensure dry run is executed correctly: dry_run.contains(\"moonc\")"
    );
    assert!(
        !dry_run.contains("-O0"),
        "Ensure release mode is used by default: !dry_run.contains(\"-O0\")"
    );

    // Explicit release
    let dry_run = get_stdout(&dir, ["bench", "--dry-run", "--release"]);
    println!("bench + dry_run + release output:\n{}", dry_run);
    assert!(
        dry_run.contains("moonc"),
        "Ensure dry run is executed correctly: dry_run.contains(\"moonc\")"
    );
    assert!(
        !dry_run.contains("-O0"),
        "Ensure release mode is used when --release is passed: !dry_run.contains(\"-O0\")"
    );

    // Explicit debug mode. Unsure why someone would do this for a bench, but ok.
    let dry_run = get_stdout(&dir, ["bench", "--dry-run", "--debug"]);
    println!("bench + dry-run + debug output:\n{}", dry_run);
    assert!(
        dry_run.contains("moonc"),
        "Ensure dry run is executed correctly: dry_run.contains(\"moonc\")"
    );
    assert!(
        dry_run.contains("-O0"),
        "Ensure debug mode is used when --debug is passed: dry_run.contains(\"-O0\")"
    );
}
