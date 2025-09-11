use crate::{get_stdout, TestDir};

#[test]
fn test_skip_noncontiguous() {
    let dir = TestDir::new("test_filter/skip_test");
    let stdout = get_stdout(&dir, ["test", "--verbose"]);
    assert!(
        !stdout.contains("hello_0"),
        "Expected hello_0 to be skipped, got:\n{stdout}j"
    );
    assert!(
        stdout.contains("hello_1"),
        "Expected hello_1 to run, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("hello_2"),
        "Expected hello_2 to be skipped, got:\n{stdout}"
    );
    assert!(
        stdout.contains("hello_3"),
        "Expected hello_3 to run, got:\n{stdout}"
    );
}
