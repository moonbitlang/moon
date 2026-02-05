use super::*;

#[test]
fn test_single_file_front_matter_import_ok() {
    let dir = TestDir::new("moon_test_single_file.in");
    let _ = get_stdout(&dir, ["check", "front_matter_import_ok.mbt.md"]);
}

#[test]
fn test_single_file_front_matter_import_missing_dep() {
    let dir = TestDir::new("moon_test_single_file.in");
    let stderr = get_err_stderr(&dir, ["check", "front_matter_import_missing_dep.mbt.md"]);
    assert!(stderr.contains("module 'moonbitlang/x' must include a version in moonbit.import"));
}

#[test]
fn test_single_file_front_matter_import_replaces_import_all() {
    let dir = TestDir::new("moon_test_single_file.in");
    let stdout = get_stdout(
        &dir,
        [
            "check",
            "front_matter_import_missing_pkg.mbt.md",
            "--dry-run",
        ],
    );
    assert!(stdout.contains("stack/stack.mi:xstack"));
    assert!(!stdout.contains("crypto/crypto.mi"));
}

#[test]
fn test_single_file_front_matter_import_module_root() {
    let dir = TestDir::new("moon_test_single_file.in");
    let stdout = get_stdout(&dir, ["test", "t.mbt.md", "--no-parallelize"]);
    assert!(stdout.contains("Total tests: 2, passed: 2, failed: 0."));
}
