use crate::test_cases::*;
use crate::util::check;

#[test]
fn native_backend_test_filter() {
    let dir = TestDir::new("native_backend/test_filter");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "native",
                "-p",
                "lib",
                "--file",
                "hello.mbt",
                "-i",
                "3",
                "--sort-input",
            ],
        ),
        expect![[r#"
            test C
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    // Keep one native update run because snapshot updates travel back from the
    // native test process through a different path than other backends.
    let source = dir.join("lib/hello.mbt");
    let snapshot = dir.join("lib/__snapshot__/test.d");
    assert!(!read(&source).contains("content=(#|523"));
    assert!(!snapshot.exists());
    get_stdout(
        &dir,
        [
            "test",
            "--target",
            "native",
            "-p",
            "lib",
            "--file",
            "hello.mbt",
            "-u",
            "--sort-input",
        ],
    );
    let updated_source = read(&source);
    assert!(updated_source.contains("content=(#|523"));
    assert!(updated_source.contains("content=(#|asdfhjas"));
    assert_eq!(read(snapshot), "test D\n");
}
