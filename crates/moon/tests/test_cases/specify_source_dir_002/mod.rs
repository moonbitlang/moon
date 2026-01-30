use super::*;

#[test]
fn test_specify_source_dir_002() {
    let dir = TestDir::new("specify_source_dir_002.in");
    check(
        get_err_stdout(&dir, ["test"]),
        expect![[r#"
            [username/hello] test lib/hello_test.mbt:1 ("hello") failed
            expect test failed at $ROOT/src/lib/hello_test.mbt:2:3-2:24
            Diff: (- expected, + actual)
            ----
            +Hello, world!
            ----

            Total tests: 1, passed: 0, failed: 1.
        "#]],
    );

    let output = get_stdout(&dir, ["test", "-u", "--no-parallelize"]);

    assert!(output.contains("Total tests: 1, passed: 1, failed: 0."));

    check(
        read(dir.join("src").join("lib").join("hello_test.mbt")),
        expect![[r#"
            test "hello" {
              inspect(@lib.hello(), content=(#|Hello, world!
              ))
            }
        "#]],
    );
}
