use crate::{TestDir, get_stdout};

#[test]
fn test_skip_included() {
    let dir = TestDir::new("test_filter/skip_test");
    let stdout = get_stdout(&dir, ["test", "--verbose", "--include-skipped"]);
    assert!(
        stdout.contains("hello_0"),
        "Expected hello_0 to be run, got:\n{stdout}"
    );
    assert!(
        stdout.contains("hello_1"),
        "Expected hello_1 to run, got:\n{stdout}"
    );
    assert!(
        stdout.contains("hello_2"),
        "Expected hello_2 to be run, got:\n{stdout}"
    );
    assert!(
        stdout.contains("hello_3"),
        "Expected hello_3 to run, got:\n{stdout}"
    );
}
