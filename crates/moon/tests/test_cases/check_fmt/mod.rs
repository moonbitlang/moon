use super::*;

#[test]
fn test_check_fmt() {
    let dir = TestDir::new("check_fmt.in");

    let output = get_stdout(&dir, ["check", "--fmt"]);

    assert!(output.contains("File not formatted: $ROOT/check_fmt.mbt"));
    assert!(output.contains("File not formatted: $ROOT/cmd/main/main.mbt"));

    // Run `moon check --fmt` a second time to verify that the `can_dirty_on_output`
    // behavior works correctly (formatting checks are re-run each time).
    // This tests the idempotent behavior of the warn-only mode.
    let output = get_stdout(&dir, ["check", "--fmt"]);
    assert!(output.contains("File not formatted: $ROOT/check_fmt.mbt"));
    assert!(output.contains("File not formatted: $ROOT/cmd/main/main.mbt"));
}

#[test]
fn test_check_fmt_skips_filtered_paths() {
    let dir = TestDir::new("check_fmt.in");
    std::fs::create_dir_all(dir.join("notes")).unwrap();
    std::fs::write(dir.join("notes/README.txt"), "not a package").unwrap();

    let output = get_stdout(&dir, ["check", "cmd/main", "notes", "--fmt"]);
    assert!(
        output.contains("File not formatted: $ROOT/cmd/main/main.mbt"),
        "stdout: {output}"
    );
    assert!(
        !output.contains("File not formatted: $ROOT/check_fmt.mbt"),
        "stdout: {output}"
    );

    let stderr = get_stderr(&dir, ["check", "cmd/main", "notes", "--fmt", "--verbose"]);
    assert!(stderr.contains("skipping path `notes`"), "stderr: {stderr}");
}
