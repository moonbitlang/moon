use super::*;

#[test]
fn test_check_fmt() {
    let dir = TestDir::new("check_fmt.in");

    let output = get_stderr(&dir, ["check", "--fmt"]);

    assert!(output.contains("File not formatted: $ROOT/check_fmt.mbt"));
    assert!(output.contains("File not formatted: $ROOT/cmd/main/main.mbt"));

    let output = get_stderr(&dir, ["check", "--fmt"]);
    assert!(output.contains("File not formatted: $ROOT/check_fmt.mbt"));
    assert!(output.contains("File not formatted: $ROOT/cmd/main/main.mbt"));
}
