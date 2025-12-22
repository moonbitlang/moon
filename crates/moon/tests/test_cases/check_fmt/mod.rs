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
