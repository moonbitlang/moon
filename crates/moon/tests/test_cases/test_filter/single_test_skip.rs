use crate::{get_stdout, TestDir};

#[test]
fn test_single_test_skip() {
    let dir = TestDir::new("test_filter/skip_test");
    let stdout = get_stdout(
        &dir,
        [
            "test",
            "--verbose",
            "-p",
            "username/hello",
            "--file",
            "hello.mbt",
            "-i",
            "0",
        ],
    );
    assert!(
        stdout.contains("hello_0"),
        "Expected hello_0 to be run, got:\n{stdout}"
    );

    let stdout = get_stdout(
        &dir,
        [
            "test",
            "--verbose",
            "-p",
            "username/hello",
            "--file",
            "hello.mbt",
            "-i",
            "1",
        ],
    );
    assert!(
        stdout.contains("hello_1"),
        "Expected hello_1 to be run, got:\n{stdout}"
    );

    let stdout = get_stdout(
        &dir,
        [
            "test",
            "--verbose",
            "-p",
            "username/hello",
            "--file",
            "hello.mbt",
            "-i",
            "2",
        ],
    );
    assert!(
        stdout.contains("hello_2"),
        "Expected hello_2 to be run, got:\n{stdout}"
    );
    let stdout = get_stdout(
        &dir,
        [
            "test",
            "--verbose",
            "-p",
            "username/hello",
            "--file",
            "hello.mbt",
            "-i",
            "3",
        ],
    );
    assert!(
        stdout.contains("hello_3"),
        "Expected hello_3 to be run, got:\n{stdout}"
    );
}
